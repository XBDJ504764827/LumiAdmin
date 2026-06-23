// 全球封禁同步服务
// 从 KZTimer GlobalAPI (https://kztimerglobal.com/api/v2.0/bans) 同步封禁记录，
// 并在 ban_records 中创建本地封禁，使被全球封禁的玩家无法进入服务器。
//
// 同步策略（只同步增量）：
//   1. global_bans 表以 kzt_ban_id 为 UNIQUE 键
//   2. 同步时拉取 KZTimer 活跃封禁，仅对本地 global_bans 表中不存在的 kzt_ban_id 创建记录
//   3. 已存在的 global_bans 记录：仅更新元数据（名称/备注等），不改变 ban_records 状态
//   4. 管理员在 LumiAdmin 中解封后，永远不会被同步重新封禁（per-steam_id 标记）
//   5. 不再做"全量比对过期"逻辑：旧封禁在 KZTimer 过期后，本地不自动解封
//      （避免解封后又被重新封禁的问题）
//
// 封禁时间与解封时间：
//   - 同步创建的 ban_records 的 created_at 使用 KZTimer 封禁的 created_on
//   - expires_at 使用 KZTimer 封禁的 expires_on
//   - 这样本地封禁时间与全球 API 封禁时间一致
use crate::{db::Database, services::observability_service};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Postgres;
use sqlx::QueryBuilder;
use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};
use uuid::Uuid;

/// KZTimer API 返回的封禁记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KZTBan {
    pub id: i64,
    pub ban_type: String,
    pub expires_on: Option<String>,
    pub steamid64: String,
    pub player_name: Option<String>,
    pub steam_id: Option<String>,
    pub notes: Option<String>,
    pub stats: Option<String>,
    pub server_id: Option<i64>,
    pub created_on: Option<String>,
    pub updated_on: Option<String>,
}

/// 本地 global_bans 行
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct GlobalBanRow {
    pub id: Uuid,
    pub kzt_ban_id: i64,
    pub steam_id64: String,
    pub player_name: Option<String>,
    pub steam_id: Option<String>,
    pub ban_type: String,
    pub notes: Option<String>,
    pub stats: Option<String>,
    pub server_id: Option<i64>,
    pub expires_on: Option<String>,
    pub created_on: Option<String>,
    pub updated_on: Option<String>,
    pub is_expired: bool,
    pub local_ban_id: Option<Uuid>,
    pub manual_unbanned: bool,
    pub synced_at: DateTime<Utc>,
}

/// 实时封禁项（KZTimer 数据 + 本地状态）
#[derive(Debug, Serialize)]
pub struct LiveBanItem {
    /// KZTimer 封禁原始数据
    pub ban: KZTBan,
    /// 本地 ban_records 中的封禁 ID（如果已同步创建）
    pub local_ban_id: Option<Uuid>,
    /// 管理员是否已手动解封
    pub manual_unbanned: bool,
}

/// 实时封禁列表结果
#[derive(Debug, Serialize)]
pub struct LiveBanListResult {
    pub items: Vec<LiveBanItem>,
    pub page: i64,
    pub page_size: i64,
    pub total: i64,
    /// 当前页是否满（用于判断是否有下一页）
    pub has_more: bool,
    /// 数据来源：local_cache=本地同步表
    pub source: String,
    /// 当使用降级数据时给前端展示的提示
    pub warning: Option<String>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct LocalGlobalBanRecord {
    pub kzt_ban_id: i64,
    pub steam_id64: String,
    pub player_name: Option<String>,
    pub steam_id: Option<String>,
    pub ban_type: String,
    pub notes: Option<String>,
    pub stats: Option<String>,
    pub server_id: Option<i64>,
    pub expires_on: Option<String>,
    pub created_on: Option<String>,
    pub updated_on: Option<String>,
    pub local_ban_id: Option<Uuid>,
    pub manual_unbanned: bool,
}

impl LocalGlobalBanRecord {
    fn into_live_item(self) -> LiveBanItem {
        LiveBanItem {
            ban: KZTBan {
                id: self.kzt_ban_id,
                ban_type: self.ban_type,
                expires_on: self.expires_on,
                steamid64: self.steam_id64,
                player_name: self.player_name,
                steam_id: self.steam_id,
                notes: self.notes,
                stats: self.stats,
                server_id: self.server_id,
                created_on: self.created_on,
                updated_on: self.updated_on,
            },
            local_ban_id: self.local_ban_id,
            manual_unbanned: self.manual_unbanned,
        }
    }

    fn to_public_json(&self) -> serde_json::Value {
        serde_json::json!({
            "id": self.kzt_ban_id,
            "ban_type": self.ban_type,
            "notes": self.notes,
            "stats": self.stats,
            "expires_on": self.expires_on,
            "created_on": self.created_on,
            "updated_on": self.updated_on,
            "player_name": self.player_name,
            "steamid64": self.steam_id64,
            "steam_id": self.steam_id,
            "server_id": self.server_id,
        })
    }
}

// =====================================================
// HTTP 请求（带超时 + 重试）
// =====================================================

static KZT_RATE_LIMIT_UNTIL: OnceLock<Mutex<Option<Instant>>> = OnceLock::new();
static GLOBAL_BAN_SYNC_LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();

fn kzt_rate_limit_until() -> &'static Mutex<Option<Instant>> {
    KZT_RATE_LIMIT_UNTIL.get_or_init(|| Mutex::new(None))
}

fn global_ban_sync_lock() -> &'static tokio::sync::Mutex<()> {
    GLOBAL_BAN_SYNC_LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

fn kzt_cooldown_remaining() -> Option<Duration> {
    let mut guard = kzt_rate_limit_until().lock().ok()?;
    let until = *guard;
    match until {
        Some(instant) if instant > Instant::now() => Some(instant.duration_since(Instant::now())),
        Some(_) => {
            *guard = None;
            None
        }
        None => None,
    }
}

fn set_kzt_cooldown(duration: Duration, reason: &str) {
    let until = Instant::now() + duration;
    if let Ok(mut guard) = kzt_rate_limit_until().lock() {
        match *guard {
            Some(current) if current > until => {}
            _ => *guard = Some(until),
        }
    }
    tracing::warn!(
        cooldown_secs = duration.as_secs(),
        reason,
        "KZTimer API 进入限流冷却"
    );
}

fn retry_after_duration(headers: &reqwest::header::HeaderMap) -> Duration {
    headers
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.trim().parse::<u64>().ok())
        .map(Duration::from_secs)
        .unwrap_or_else(|| Duration::from_secs(60))
}

/// 从 KZTimer API 拉取封禁列表（带超时和重试）
/// `expired_filter`: None=全部, Some(true)=仅过期, Some(false)=仅活跃
async fn fetch_kzt_bans(
    offset: i64,
    limit: i64,
    expired_filter: Option<bool>,
) -> anyhow::Result<Vec<KZTBan>> {
    if let Some(remaining) = kzt_cooldown_remaining() {
        anyhow::bail!(
            "KZTimer API 正在限流冷却，请 {} 秒后重试",
            remaining.as_secs().max(1)
        );
    }

    let client = crate::http_client::http_client();
    let url = match expired_filter {
        Some(v) => format!(
            "https://kztimerglobal.com/api/v2.0/bans?isExpired={}&limit={}&offset={}",
            v, limit, offset
        ),
        None => format!(
            "https://kztimerglobal.com/api/v2.0/bans?limit={}&offset={}",
            limit, offset
        ),
    };

    let mut last_err: Option<String> = None;
    for attempt in 0..3u8 {
        let req_result =
            tokio::time::timeout(std::time::Duration::from_secs(20), client.get(&url).send()).await;

        match req_result {
            Ok(Ok(resp)) => {
                let status = resp.status();
                if status.is_success() {
                    let bans: Vec<KZTBan> = resp.json().await?;
                    return Ok(bans);
                }
                if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
                    let cooldown = retry_after_duration(resp.headers());
                    set_kzt_cooldown(cooldown, "429 Too Many Requests");
                    anyhow::bail!(
                        "KZTimer API 返回 429 Too Many Requests，已暂停请求 {} 秒",
                        cooldown.as_secs()
                    );
                }
                let err_str = format!("status={}", status);
                tracing::warn!(attempt, offset, %err_str, "KZTimer API 请求返回非成功状态");
                last_err = Some(format!("KZTimer API 返回 {status}"));
            }
            Ok(Err(e)) => {
                let err_str = e.to_string();
                tracing::warn!(attempt, offset, %err_str, "KZTimer API 请求失败");
                last_err = Some(err_str);
            }
            Err(_) => {
                let err_str = format!("KZTimer API 请求超时 (20s), offset={}", offset);
                tracing::warn!(attempt, offset, %err_str, "KZTimer API 请求超时");
                last_err = Some(err_str);
            }
        }

        if attempt < 2 {
            tokio::time::sleep(std::time::Duration::from_secs(2 + attempt as u64)).await;
        }
    }

    anyhow::bail!(last_err.unwrap_or_else(|| "KZTimer API 请求失败".to_string()))
}

// =====================================================
// 本地同步表查询（前端展示用）
// =====================================================

/// 获取本地同步的 KZTimer 活跃封禁列表并合并本地封禁状态（前端展示用）。
/// KZTimer 实时请求只由后台同步/手动同步触发，避免页面访问打爆外部限额。
pub async fn fetch_live_global_bans(
    db: &Database,
    page: i64,
    page_size: i64,
) -> anyhow::Result<LiveBanListResult> {
    let offset = (page - 1) * page_size;
    let rows: Vec<LocalGlobalBanRecord> = sqlx::query_as(
        r#"SELECT
              kzt_ban_id::BIGINT,
              steam_id64,
              player_name,
              steam_id,
              ban_type,
              notes,
              stats,
              server_id::BIGINT,
              expires_on,
              created_on,
              updated_on,
              local_ban_id,
              manual_unbanned
           FROM global_bans
           WHERE is_expired = false
           ORDER BY created_on DESC NULLS LAST, synced_at DESC
           LIMIT $1 OFFSET $2"#,
    )
    .bind(page_size + 1)
    .bind(offset)
    .fetch_all(&db.pool)
    .await?;

    let has_more = rows.len() as i64 > page_size;
    let items: Vec<LiveBanItem> = rows
        .into_iter()
        .take(page_size as usize)
        .map(LocalGlobalBanRecord::into_live_item)
        .collect();

    Ok(LiveBanListResult {
        items,
        page,
        page_size,
        total: 0,
        has_more,
        source: "local_cache".to_string(),
        warning: None,
    })
}

/// 按 SteamID 搜索玩家全球封禁。
/// 查询来源为后台同步维护的 global_bans 表，不在交互请求中直连 KZTimer。
#[derive(Debug, Serialize)]
pub struct PlayerBanSearchResult {
    pub items: Vec<LiveBanItem>,
    /// 数据来源：local_cache=本地同步表, none=未命中
    pub source: String,
}

pub async fn search_player_bans(
    db: &Database,
    resolver: &crate::services::steam_service::SteamResolver,
    steam_input: &str,
) -> anyhow::Result<PlayerBanSearchResult> {
    let identity = resolver.resolve(steam_input).await?;
    let steamid64 = &identity.steamid64;

    let local_rows = load_local_global_bans_for_steamids(db, &[steamid64.clone()]).await?;
    let items: Vec<LiveBanItem> = local_rows
        .into_iter()
        .map(LocalGlobalBanRecord::into_live_item)
        .collect();
    let source = if items.is_empty() {
        "none".to_string()
    } else {
        "local_cache".to_string()
    };

    Ok(PlayerBanSearchResult { items, source })
}

async fn load_local_global_bans_for_steamids(
    db: &Database,
    steamids: &[String],
) -> anyhow::Result<Vec<LocalGlobalBanRecord>> {
    if steamids.is_empty() {
        return Ok(Vec::new());
    }

    sqlx::query_as(
        r#"SELECT
              kzt_ban_id::BIGINT,
              steam_id64,
              player_name,
              steam_id,
              ban_type,
              notes,
              stats,
              server_id::BIGINT,
              expires_on,
              created_on,
              updated_on,
              local_ban_id,
              manual_unbanned
           FROM global_bans
           WHERE steam_id64 = ANY($1) AND is_expired = false
           ORDER BY steam_id64 ASC, created_on DESC NULLS LAST, synced_at DESC
           LIMIT 3000"#,
    )
    .bind(steamids)
    .fetch_all(&db.pool)
    .await
    .map_err(Into::into)
}

pub async fn public_global_bans_for_steamid(
    db: &Database,
    steamid64: &str,
) -> anyhow::Result<Vec<serde_json::Value>> {
    let rows = load_local_global_bans_for_steamids(db, &[steamid64.trim().to_string()]).await?;
    Ok(rows
        .iter()
        .map(LocalGlobalBanRecord::to_public_json)
        .collect())
}

pub async fn public_global_bans_batch(
    db: &Database,
    steamids: &[String],
) -> anyhow::Result<HashMap<String, serde_json::Value>> {
    let rows = load_local_global_bans_for_steamids(db, steamids).await?;
    let mut grouped: HashMap<String, Vec<serde_json::Value>> = HashMap::new();
    for row in rows {
        grouped
            .entry(row.steam_id64.clone())
            .or_default()
            .push(row.to_public_json());
    }

    let results = steamids
        .iter()
        .map(|steamid| {
            let bans = grouped.remove(steamid).unwrap_or_default();
            (steamid.clone(), serde_json::json!(bans))
        })
        .collect();
    Ok(results)
}

// =====================================================
// 后台同步逻辑（只同步增量）
// =====================================================

/// 核心同步逻辑（增量同步）：
///   - 读取 sync_since（部署这版后端的时间点）
///   - 拉取 KZTimer 所有活跃封禁（isExpired=false）
///   - 仅同步 created_on >= sync_since 的新增全球封禁到 LumiAdmin
///   - 已存在的记录：仅更新元数据，不改变 ban_records 状态
///   - 不再自动解封过期的全球封禁（避免解封后又被重新封禁）
pub async fn sync_global_bans(db: &Database) -> anyhow::Result<SyncResult> {
    let Ok(_guard) = global_ban_sync_lock().try_lock() else {
        tracing::warn!("全球封禁同步跳过：已有同步任务正在运行");
        anyhow::bail!("全球封禁同步正在运行，请稍后重试");
    };

    sync_global_bans_locked(db).await
}

async fn sync_global_bans_locked(db: &Database) -> anyhow::Result<SyncResult> {
    let mut result = SyncResult::default();

    // 0) 读取同步起始时间（部署这版后端的时间）
    let sync_since: DateTime<Utc> =
        sqlx::query_as("SELECT sync_since FROM global_ban_config WHERE id = true")
            .fetch_one(&db.pool)
            .await
            .map(|(v,): (DateTime<Utc>,)| v)
            .unwrap_or_else(|_| Utc::now());

    // 1) 拉取 KZTimer 所有活跃封禁（分页）
    let mut all_bans: Vec<KZTBan> = Vec::new();
    for offset in (0..10000).step_by(500) {
        let bans = match fetch_kzt_bans(offset, 500, Some(false)).await {
            Ok(bans) => bans,
            Err(e) if all_bans.is_empty() => {
                tracing::warn!(
                    offset,
                    error = %e,
                    "KZTimer 全球封禁首个分页拉取失败，同步无法继续"
                );
                return Err(e);
            }
            Err(e) => {
                tracing::warn!(
                    offset,
                    already_fetched = all_bans.len(),
                    error = %e,
                    "KZTimer 全球封禁分页拉取失败，将使用已拉取数据继续同步"
                );
                break;
            }
        };
        let len = bans.len();
        all_bans.extend(bans);
        if len < 500 {
            break; // 最后一页
        }
    }
    result.total_fetched = all_bans.len() as i64;

    // 2) 收集 API 返回的所有 kzt_ban_id
    let api_ban_ids: std::collections::HashSet<i64> = all_bans.iter().map(|b| b.id).collect();

    // 3) 批量查询本地已存在的 kzt_ban_id（避免逐条查询）
    let existing_ids: std::collections::HashSet<i64> = if api_ban_ids.is_empty() {
        std::collections::HashSet::new()
    } else {
        let id_vec: Vec<i64> = api_ban_ids.iter().copied().collect();
        let rows: Vec<(i64,)> =
            sqlx::query_as("SELECT kzt_ban_id::BIGINT FROM global_bans WHERE kzt_ban_id = ANY($1)")
                .bind(&id_vec)
                .fetch_all(&db.pool)
                .await?;
        rows.into_iter().map(|r| r.0).collect()
    };
    // 4) 查询已被管理员解封的 kzt_ban_id（per-ban 标记）
    //    管理员解封只标记该条全球封禁记录，后续该玩家如果被 KZTimer 发了新的封禁（不同 kzt_ban_id）
    //    仍会被同步到 LumiAdmin 中封禁
    let unbanned_kzt_ids: std::collections::HashSet<i64> = if api_ban_ids.is_empty() {
        std::collections::HashSet::new()
    } else {
        let id_vec: Vec<i64> = api_ban_ids.iter().copied().collect();
        let rows: Vec<(i64,)> = sqlx::query_as(
            "SELECT kzt_ban_id::BIGINT FROM global_bans WHERE manual_unbanned = true AND kzt_ban_id = ANY($1)",
        )
        .bind(&id_vec)
        .fetch_all(&db.pool)
        .await?;
        rows.into_iter().map(|r| r.0).collect()
    };
    // 5) 批量处理新封禁（INSERT global_bans + 按需创建 ban_records）
    //    分批批量 INSERT，避免逐条 N+1
    let new_bans: Vec<&KZTBan> = all_bans
        .iter()
        .filter(|b| !existing_ids.contains(&b.id))
        .collect();

    let mut new_ban_local_ids: Vec<(i64, Option<Uuid>)> = Vec::new();
    for ban in &new_bans {
        let is_ban_unbanned = unbanned_kzt_ids.contains(&ban.id);
        let ban_created = ban
            .created_on
            .as_deref()
            .and_then(|c| parse_kzt_datetime(c).ok());

        let local_id = if is_ban_unbanned {
            None
        } else if ban_created.is_none() {
            tracing::warn!(
                kzt_ban_id = ban.id,
                steam_id = %ban.steamid64,
                created_on = ?ban.created_on,
                "跳过新增全球封禁：created_on 缺失或无法解析"
            );
            None
        } else if ban_created.map(|dt| dt < sync_since).unwrap_or(true) {
            None
        } else {
            match create_local_ban(
                db,
                &ban.steamid64,
                &ban.player_name,
                &ban.ban_type,
                &ban.notes,
                ban.created_on.as_deref(),
                ban.expires_on.as_deref(),
            )
            .await
            {
                Ok(Some(local_id)) => Some(local_id),
                Ok(None) => None,
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        kzt_ban_id = ban.id,
                        steam_id = %ban.steamid64,
                        "新增全球封禁创建本地封禁失败"
                    );
                    None
                }
            }
        };

        if local_id.is_some() {
            result.new_bans += 1;
        }
        new_ban_local_ids.push((ban.id, local_id));
    }

    // 建立 kzt_ban_id → local_ban_id 的快速查询映射
    let local_id_map: std::collections::HashMap<i64, Option<Uuid>> =
        new_ban_local_ids.into_iter().collect();

    // 批量 INSERT global_bans（每批 500 条）
    for chunk in new_bans.chunks(500) {
        if chunk.is_empty() {
            continue;
        }
        let ids: Vec<Uuid> = (0..chunk.len()).map(|_| Uuid::new_v4()).collect();
        let kzt_ids: Vec<i64> = chunk.iter().map(|b| b.id).collect();
        let steam_id64s: Vec<&str> = chunk.iter().map(|b| b.steamid64.as_str()).collect();
        let player_names: Vec<Option<&str>> =
            chunk.iter().map(|b| b.player_name.as_deref()).collect();
        let steam_ids: Vec<Option<&str>> = chunk.iter().map(|b| b.steam_id.as_deref()).collect();
        let ban_types: Vec<&str> = chunk.iter().map(|b| b.ban_type.as_str()).collect();
        let notes_vec: Vec<Option<&str>> = chunk.iter().map(|b| b.notes.as_deref()).collect();
        let stats_vec: Vec<Option<&str>> = chunk.iter().map(|b| b.stats.as_deref()).collect();
        let server_ids: Vec<Option<i64>> = chunk.iter().map(|b| b.server_id).collect();
        let expires_vec: Vec<Option<&str>> =
            chunk.iter().map(|b| b.expires_on.as_deref()).collect();
        let created_vec: Vec<Option<&str>> =
            chunk.iter().map(|b| b.created_on.as_deref()).collect();
        let updated_vec: Vec<Option<&str>> =
            chunk.iter().map(|b| b.updated_on.as_deref()).collect();
        let local_ids: Vec<Option<Uuid>> = chunk
            .iter()
            .map(|b| local_id_map.get(&b.id).copied().unwrap_or(None))
            .collect();
        let manual_flags: Vec<bool> = chunk
            .iter()
            .map(|b| {
                unbanned_kzt_ids.contains(&b.id)
                    || b.created_on
                        .as_deref()
                        .and_then(|c| parse_kzt_datetime(c).ok())
                        .map(|dt| dt < sync_since)
                        .unwrap_or(true)
            })
            .collect();

        let insert_result = sqlx::query(
            r#"INSERT INTO global_bans (
                id, kzt_ban_id, steam_id64, player_name, steam_id, ban_type,
                notes, stats, server_id, expires_on, created_on, updated_on,
                is_expired, local_ban_id, manual_unbanned, synced_at
               )
               SELECT u.id, u.kzt_ban_id, u.steam_id64, u.player_name, u.steam_id, u.ban_type,
                      u.notes, u.stats, u.server_id, u.expires_on, u.created_on, u.updated_on,
                      false, u.local_ban_id, u.manual_unbanned, now()
               FROM UNNEST($1::UUID[], $2::INTEGER[], $3::TEXT[], $4::TEXT[], $5::TEXT[], $6::TEXT[],
                           $7::TEXT[], $8::TEXT[], $9::INTEGER[], $10::TEXT[], $11::TEXT[], $12::TEXT[],
                           $13::UUID[], $14::BOOLEAN[])
                    AS u(id, kzt_ban_id, steam_id64, player_name, steam_id, ban_type,
                         notes, stats, server_id, expires_on, created_on, updated_on,
                         local_ban_id, manual_unbanned)
               ON CONFLICT (kzt_ban_id) DO NOTHING"#,
        )
        .bind(&ids)
        .bind(&kzt_ids)
        .bind(&steam_id64s)
        .bind(&player_names)
        .bind(&steam_ids)
        .bind(&ban_types)
        .bind(&notes_vec)
        .bind(&stats_vec)
        .bind(&server_ids)
        .bind(&expires_vec)
        .bind(&created_vec)
        .bind(&updated_vec)
        .bind(&local_ids)
        .bind(&manual_flags)
        .execute(&db.pool)
        .await?;
        let _ = insert_result;
    }

    // 6) 批量更新已存在记录的元数据
    let existing_bans: Vec<&KZTBan> = all_bans
        .iter()
        .filter(|b| existing_ids.contains(&b.id))
        .collect();

    for chunk in existing_bans.chunks(500) {
        if chunk.is_empty() {
            continue;
        }
        let kzt_ids: Vec<i64> = chunk.iter().map(|b| b.id).collect();
        let player_names: Vec<Option<&str>> =
            chunk.iter().map(|b| b.player_name.as_deref()).collect();
        let ban_types: Vec<&str> = chunk.iter().map(|b| b.ban_type.as_str()).collect();
        let notes_vec: Vec<Option<&str>> = chunk.iter().map(|b| b.notes.as_deref()).collect();
        let stats_vec: Vec<Option<&str>> = chunk.iter().map(|b| b.stats.as_deref()).collect();
        let expires_vec: Vec<Option<&str>> =
            chunk.iter().map(|b| b.expires_on.as_deref()).collect();
        let updated_vec: Vec<Option<&str>> =
            chunk.iter().map(|b| b.updated_on.as_deref()).collect();

        let update_result = sqlx::query(
            r#"UPDATE global_bans gb
               SET player_name = u.player_name, ban_type = u.ban_type, notes = u.notes,
                   stats = u.stats, expires_on = u.expires_on, updated_on = u.updated_on,
                   synced_at = now()
               FROM UNNEST($1::INTEGER[], $2::TEXT[], $3::TEXT[], $4::TEXT[], $5::TEXT[], $6::TEXT[], $7::TEXT[])
                    AS u(kzt_ban_id, player_name, ban_type, notes, stats, expires_on, updated_on)
               WHERE gb.kzt_ban_id = u.kzt_ban_id"#,
        )
        .bind(&kzt_ids)
        .bind(&player_names)
        .bind(&ban_types)
        .bind(&notes_vec)
        .bind(&stats_vec)
        .bind(&expires_vec)
        .bind(&updated_vec)
        .execute(&db.pool)
        .await?;
        let _ = update_result;
    }

    // 7) 补偿已有 global_bans 但缺失活跃 ban_records 的记录。
    //    旧逻辑只在 kzt_ban_id 首次插入时创建本地封禁；如果当时创建失败、
    //    或记录已存在但 local_ban_id 为空，后续同步只更新元数据，不会再补建。
    let repair_stats = ensure_missing_local_bans(db, &all_bans, sync_since).await?;
    result.repaired_local_bans = repair_stats.repaired;
    result.new_bans += repair_stats.repaired;

    Ok(result)
}

#[derive(Debug, Default)]
struct LocalBanRepairStats {
    repaired: i64,
    skipped_missing_state: i64,
    skipped_active_local_ban: i64,
    skipped_manual_unbanned: i64,
    skipped_before_sync_since: i64,
    skipped_create_none: i64,
    create_errors: i64,
}

async fn ensure_missing_local_bans(
    db: &Database,
    all_bans: &[KZTBan],
    sync_since: DateTime<Utc>,
) -> anyhow::Result<LocalBanRepairStats> {
    let mut stats = LocalBanRepairStats::default();
    if all_bans.is_empty() {
        return Ok(stats);
    }

    let api_ban_ids: Vec<i64> = all_bans.iter().map(|b| b.id).collect();
    let rows: Vec<(i64, Option<Uuid>, bool, Option<String>)> = sqlx::query_as(
        r#"SELECT gb.kzt_ban_id::BIGINT, gb.local_ban_id, gb.manual_unbanned, br.status
           FROM global_bans gb
           LEFT JOIN ban_records br ON br.id = gb.local_ban_id
           WHERE gb.kzt_ban_id = ANY($1)"#,
    )
    .bind(&api_ban_ids)
    .fetch_all(&db.pool)
    .await?;

    let local_states: std::collections::HashMap<i64, (Option<Uuid>, bool, Option<String>)> = rows
        .into_iter()
        .map(
            |(kzt_ban_id, local_ban_id, manual_unbanned, local_status)| {
                (kzt_ban_id, (local_ban_id, manual_unbanned, local_status))
            },
        )
        .collect();

    for ban in all_bans {
        let Some((local_ban_id, manual_unbanned, local_status)) = local_states.get(&ban.id) else {
            stats.skipped_missing_state += 1;
            continue;
        };
        if *manual_unbanned {
            stats.skipped_manual_unbanned += 1;
            continue;
        }
        let has_active_local_ban =
            local_ban_id.is_some() && local_status.as_deref() == Some("active");
        if has_active_local_ban {
            stats.skipped_active_local_ban += 1;
            continue;
        }

        let ban_created = ban
            .created_on
            .as_deref()
            .and_then(|c| parse_kzt_datetime(c).ok());
        let should_sync = ban_created.map(|dt| dt >= sync_since).unwrap_or(false);
        if !should_sync {
            stats.skipped_before_sync_since += 1;
            continue;
        }

        match create_local_ban(
            db,
            &ban.steamid64,
            &ban.player_name,
            &ban.ban_type,
            &ban.notes,
            ban.created_on.as_deref(),
            ban.expires_on.as_deref(),
        )
        .await
        {
            Ok(Some(local_id)) => {
                sqlx::query(
                    r#"UPDATE global_bans
                       SET local_ban_id = $2, manual_unbanned = false, synced_at = now()
                       WHERE kzt_ban_id = $1 AND manual_unbanned = false"#,
                )
                .bind(ban.id)
                .bind(local_id)
                .execute(&db.pool)
                .await?;
                stats.repaired += 1;
            }
            Ok(None) => {
                stats.skipped_create_none += 1;
            }
            Err(e) => {
                stats.create_errors += 1;
                tracing::warn!(
                    error = %e,
                    kzt_ban_id = ban.id,
                    steam_id = %ban.steamid64,
                    "补建全球封禁本地封禁失败"
                );
            }
        }
    }

    Ok(stats)
}

/// 在 ban_records 中创建本地封禁（source = "global_ban"）
/// 封禁时间使用 KZTimer 的 created_on，到期时间使用 KZTimer 的 expires_on
async fn create_local_ban(
    db: &Database,
    steam_id64: &str,
    player_name: &Option<String>,
    ban_type: &str,
    notes: &Option<String>,
    created_on: Option<&str>,
    expires_on: Option<&str>,
) -> anyhow::Result<Option<Uuid>> {
    // 已有 global_ban 活跃封禁 → 复用
    let existing_global: Option<(Uuid,)> = sqlx::query_as(
        "SELECT id FROM ban_records WHERE steam_id = $1 AND source = 'global_ban' AND status = 'active'"
    )
    .bind(steam_id64)
    .fetch_optional(&db.pool)
    .await?;
    if let Some((existing_id,)) = existing_global {
        return Ok(Some(existing_id));
    }

    // 有其他来源的活跃封禁 → 不创建（LumiAdmin 封禁优先）
    let existing_other: Option<(Uuid,)> = sqlx::query_as(
        "SELECT id FROM ban_records WHERE steam_id = $1 AND source != 'global_ban' AND status = 'active'"
    )
    .bind(steam_id64)
    .fetch_optional(&db.pool)
    .await?;
    if existing_other.is_some() {
        tracing::info!(
            steam_id = steam_id64,
            "玩家已有 LumiAdmin 活跃封禁，跳过创建全球封禁来源记录"
        );
        return Ok(None);
    }

    // 解析封禁时间和到期时间（与 KZTimer 全球封禁保持一致）
    let (duration_minutes, reason, created_at, expires_at) =
        build_ban_meta(ban_type, notes, created_on, expires_on);

    let id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO ban_records (
               id, player, steam_id, ban_type, duration_minutes,
               reason, status, operator_name, source, created_at, expires_at
           )
           VALUES ($1, $2, $3, 'steam', $4, $5, 'active', 'KZTimer Global', 'global_ban', $6, $7)
           ON CONFLICT DO NOTHING"#,
    )
    .bind(id)
    .bind(player_name)
    .bind(steam_id64)
    .bind(duration_minutes)
    .bind(reason)
    .bind(created_at)
    .bind(expires_at)
    .execute(&db.pool)
    .await?;

    Ok(Some(id))
}

/// 构建封禁时长（分钟）、理由文本、封禁时间和到期时间
/// - 永久封禁（expires_on 为 9999 年或为空）：duration_minutes=0，expires_at=NULL
/// - 临时封禁：按 created_on → expires_on 计算 duration_minutes 和 expires_at
/// - created_at 始终使用 KZTimer 的 created_on（解析失败时回退到 now()）
fn build_ban_meta(
    ban_type: &str,
    notes: &Option<String>,
    created_on: Option<&str>,
    expires_on: Option<&str>,
) -> (i64, String, Option<DateTime<Utc>>, Option<DateTime<Utc>>) {
    let reason = format!(
        "[全球封禁] {}{}",
        ban_type,
        notes
            .as_deref()
            .map(|n| format!(" - {}", n))
            .unwrap_or_default()
    );

    // 解析封禁时间（失败则用 now()）
    let created_at = created_on
        .and_then(|c| parse_kzt_datetime(c).ok())
        .or_else(|| Some(Utc::now()));

    // 解析到期时间
    let expires_dt = expires_on.and_then(|e| parse_kzt_datetime(e).ok());

    // 判断是否永久封禁
    let is_permanent = expires_on.map(|e| e.starts_with("9999")).unwrap_or(true); // expires_on 为空也视为永久

    if is_permanent {
        // 永久封禁：duration_minutes=0，expires_at=NULL
        return (0, reason, created_at, None);
    }

    // 临时封禁：计算 duration_minutes
    if let (Some(c), Some(e)) = (created_at, expires_dt) {
        if e > c {
            let minutes = ((e - c).num_minutes()).max(1);
            return (minutes, reason, Some(c), Some(e));
        }
    }

    // 解析失败或时间不合理 → 按永久处理
    (0, reason, created_at, None)
}

/// 解析 KZTimer API 返回的日期时间字符串
fn parse_kzt_datetime(s: &str) -> Result<DateTime<Utc>, chrono::ParseError> {
    // 尝试 RFC3339（如 2024-01-01T00:00:00Z）
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Ok(dt.with_timezone(&Utc));
    }
    // 尝试 ISO 8601 无时区（如 2024-01-01T00:00:00）→ 按 UTC 解析
    let s2 = if s.ends_with('Z') {
        s.to_string()
    } else {
        format!("{}Z", s)
    };
    DateTime::parse_from_rfc3339(&s2).map(|dt| dt.with_timezone(&Utc))
}

/// 管理员手动解封全球封禁对应的本地封禁
/// 设置 manual_unbanned=true（per-steam_id 标记），防止下次同步重新封禁
pub async fn manual_unban(
    db: &Database,
    ban_cache: &crate::services::access_cache::ActiveBanCache,
    kzt_ban_id: i64,
    operator: &str,
) -> anyhow::Result<()> {
    // 查找本地 global_bans 记录
    let row: Option<(Option<Uuid>,)> =
        sqlx::query_as("SELECT local_ban_id FROM global_bans WHERE kzt_ban_id = $1")
            .bind(kzt_ban_id)
            .fetch_optional(&db.pool)
            .await?;

    let (local_id,) = row.ok_or_else(|| anyhow::anyhow!("全球封禁记录不存在"))?;

    // 解除对应的 ban_records 记录
    if let Some(lid) = local_id {
        sqlx::query(
            "UPDATE ban_records SET status = 'inactive', removed_by = $2, removed_at = now() WHERE id = $1 AND status = 'active'",
        )
        .bind(lid)
        .bind(operator)
        .execute(&db.pool)
        .await?;
    }

    // 仅标记这一条全球封禁记录为 manual_unbanned=true（per-ban 标记）
    // 后续该玩家如果被 KZTimer 发了新的封禁（不同 kzt_ban_id）仍会被同步到 LumiAdmin
    sqlx::query("UPDATE global_bans SET manual_unbanned = true WHERE kzt_ban_id = $1")
        .bind(kzt_ban_id)
        .execute(&db.pool)
        .await?;

    // 刷新封禁缓存
    ban_cache.refresh(db).await?;

    Ok(())
}

/// 标记某条全球封禁记录为「管理员已解封」（供 ban_service 在解封 global_ban 来源封禁时调用）
/// 仅标记 local_ban_id 对应的那一条 global_bans 记录，不影响该玩家后续的新全球封禁
pub async fn mark_ban_unbanned_by_local_ban_id(
    db: &Database,
    local_ban_id: Uuid,
) -> anyhow::Result<()> {
    sqlx::query("UPDATE global_bans SET manual_unbanned = true WHERE local_ban_id = $1")
        .bind(local_ban_id)
        .execute(&db.pool)
        .await?;
    Ok(())
}

/// 清理误封禁数据：将 global_bans.is_expired=true 但 ban_records.status='active' 的记录修正为 inactive
pub async fn cleanup_stale_global_bans(db: &Database) -> anyhow::Result<u64> {
    let result = sqlx::query(
        r#"UPDATE ban_records
           SET status = 'inactive', removed_by = 'global_ban_cleanup', removed_at = now()
           FROM global_bans
           WHERE ban_records.id = global_bans.local_ban_id
             AND global_bans.is_expired = true
             AND ban_records.status = 'active'
             AND ban_records.source = 'global_ban'"#,
    )
    .execute(&db.pool)
    .await?;
    Ok(result.rows_affected())
}

// =====================================================
// 本地表查询（历史记录/已过期封禁）
// =====================================================

#[allow(dead_code)]
#[derive(Debug, Clone, Default, Deserialize)]
pub struct GlobalBanQueryParams {
    pub steam_id64: Option<String>,
    pub ban_type: Option<String>,
    pub is_expired: Option<bool>,
    pub search: Option<String>,
    pub page: Option<i64>,
    pub page_size: Option<i64>,
}

#[allow(dead_code)]
impl GlobalBanQueryParams {
    pub fn page(&self) -> i64 {
        self.page.unwrap_or(1).max(1)
    }
    pub fn page_size(&self) -> i64 {
        self.page_size.unwrap_or(20).clamp(1, 100)
    }
}

#[allow(dead_code)]
fn push_filter_prefix(builder: &mut QueryBuilder<'_, Postgres>, has_where: &mut bool) {
    if *has_where {
        builder.push(" AND ");
    } else {
        builder.push(" WHERE ");
        *has_where = true;
    }
}

#[allow(dead_code)]
fn push_filters(builder: &mut QueryBuilder<'_, Postgres>, params: &GlobalBanQueryParams) {
    let mut has_where = false;
    if let Some(v) = params.steam_id64.as_ref().filter(|v| !v.trim().is_empty()) {
        push_filter_prefix(builder, &mut has_where);
        builder
            .push("steam_id64 = ")
            .push_bind(v.trim().to_string());
    }
    if let Some(v) = params.ban_type.as_ref().filter(|v| !v.trim().is_empty()) {
        push_filter_prefix(builder, &mut has_where);
        builder.push("ban_type = ").push_bind(v.trim().to_string());
    }
    if let Some(v) = params.is_expired {
        push_filter_prefix(builder, &mut has_where);
        builder.push("is_expired = ").push_bind(v);
    }
    if let Some(v) = params
        .search
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        let pattern = format!("%{}%", v.replace('%', "\\%").replace('_', "\\_"));
        push_filter_prefix(builder, &mut has_where);
        builder
            .push("(steam_id64 ILIKE ")
            .push_bind(pattern.clone())
            .push(" ESCAPE '\\' OR player_name ILIKE ")
            .push_bind(pattern)
            .push(" ESCAPE '\\')");
    }
}

/// 同步结果
#[derive(Debug, Default, Serialize)]
pub struct SyncResult {
    pub total_fetched: i64,
    pub new_bans: i64,
    pub repaired_local_bans: i64,
    pub expired: i64,
    pub re_banned: i64,
}

/// 启动定时同步任务
pub fn start_global_ban_sync_loop(
    db: Database,
    ban_cache: std::sync::Arc<crate::services::access_cache::ActiveBanCache>,
    interval_secs: u64,
) {
    observability_service::register_task(
        "global_ban_sync",
        "全球封禁同步",
        "外部依赖",
        Some(interval_secs),
        true,
    );
    observability_service::register_task(
        "global_ban_stale_cleanup",
        "全球封禁误封清理",
        "清理",
        None,
        true,
    );
    tokio::spawn(async move {
        // 如果设置了环境变量 GLOBAL_BAN_SYNC_SINCE，用它覆盖数据库中的 sync_since
        if let Ok(env_since) = std::env::var("GLOBAL_BAN_SYNC_SINCE") {
            let trimmed = env_since.trim();
            if !trimmed.is_empty() {
                match chrono::DateTime::parse_from_rfc3339(trimmed)
                    .map(|dt| dt.with_timezone(&Utc))
                    .or_else(|_| {
                        // 尝试无时区格式（如 2025-01-01T00:00:00）→ 按 UTC 解析
                        chrono::DateTime::parse_from_rfc3339(&format!("{}Z", trimmed))
                            .map(|dt| dt.with_timezone(&Utc))
                    }) {
                    Ok(dt) => {
                        sqlx::query("UPDATE global_ban_config SET sync_since = $1 WHERE id = true")
                            .bind(dt)
                            .execute(&db.pool)
                            .await
                            .ok();
                        tracing::info!(%dt, "已通过 GLOBAL_BAN_SYNC_SINCE 设置全球封禁同步起始时间");
                    }
                    Err(_) => {
                        tracing::warn!(value = trimmed, "GLOBAL_BAN_SYNC_SINCE 格式无效，请使用 RFC3339 如 2025-06-13T00:00:00Z");
                    }
                }
            }
        }

        // 读取并记录当前 sync_since
        let current_since: Option<(DateTime<Utc>,)> =
            sqlx::query_as("SELECT sync_since FROM global_ban_config WHERE id = true")
                .fetch_optional(&db.pool)
                .await
                .ok()
                .flatten();
        if let Some((since,)) = current_since {
            tracing::info!(%since, "全球封禁同步起始时间（只同步该时间之后新增的全球封禁）");
        }

        // 启动时清理可能的误封禁数据
        match observability_service::observe_task(
            "global_ban_stale_cleanup",
            cleanup_stale_global_bans(&db),
            |count| format!("清理 {} 条误封记录", count),
        )
        .await
        {
            Ok(0) => {}
            Ok(n) => {
                tracing::info!(count = n, "清理了因全球封禁过期但本地仍活跃的误封禁记录");
                if let Err(e) = ban_cache.refresh(&db).await {
                    tracing::warn!(%e, "清理后刷新封禁缓存失败");
                }
            }
            Err(e) => tracing::warn!(%e, "清理误封禁数据失败"),
        }

        // 启动时执行一次增量同步
        match observability_service::observe_task("global_ban_sync", sync_global_bans(&db), |r| {
            format!(
                "拉取 {} 条，新增 {}，过期 {}，重封 {}",
                r.total_fetched, r.new_bans, r.expired, r.re_banned
            )
        })
        .await
        {
            Ok(r) => {
                tracing::info!(?r, "全球封禁初始同步完成");
                if r.new_bans > 0 || r.expired > 0 || r.re_banned > 0 {
                    if let Err(e) = ban_cache.refresh(&db).await {
                        tracing::warn!(%e, "同步后刷新封禁缓存失败");
                    }
                }
            }
            Err(e) => tracing::warn!(%e, "全球封禁初始同步失败"),
        }

        let mut interval = tokio::time::interval(std::time::Duration::from_secs(interval_secs));
        loop {
            interval.tick().await;
            match observability_service::observe_task(
                "global_ban_sync",
                sync_global_bans(&db),
                |r| {
                    format!(
                        "拉取 {} 条，新增 {}，过期 {}，重封 {}",
                        r.total_fetched, r.new_bans, r.expired, r.re_banned
                    )
                },
            )
            .await
            {
                Ok(r) => {
                    if r.new_bans > 0 || r.expired > 0 || r.re_banned > 0 {
                        tracing::info!(?r, "全球封禁同步完成");
                        if let Err(e) = ban_cache.refresh(&db).await {
                            tracing::warn!(%e, "同步后刷新封禁缓存失败");
                        }
                    }
                }
                Err(e) => tracing::warn!(%e, "全球封禁同步失败"),
            }
        }
    });
}

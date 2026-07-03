// 全球封禁同步服务
// 从 KZTimer GlobalAPI (https://kztimerglobal.com/api/v2.0/bans) 同步封禁记录，
// 并在 ban_records 中创建本地封禁，使被全球封禁的玩家无法进入服务器。
//
// 同步策略（全量权威同步活跃 KZTimer 封禁）：
//   1. global_bans 表以 kzt_ban_id 为 UNIQUE 键
//   2. 每轮完整拉取 KZTimer 活跃封禁；分页/API 失败时本轮不修改本地数据
//   3. 每条 KZTimer 封禁对应一条 source='global_ban' 的 ban_records 记录
//   4. KZTimer 中消失的封禁会标记为过期，并只自动解除对应的 source='global_ban' 本地封禁
//   5. 管理员在 LumiAdmin 中解封某条全球封禁后，仅该 kzt_ban_id 不会被同步封回
//      同一玩家后续产生新的 KZTimer 封禁 ID 时仍会同步到本地封禁管理
//   6. 服务器准入只看 ban_records；global_bans 仅作为同步/展示元数据
//
// 封禁时间与解封时间：
//   - 同步创建的 ban_records 的 created_at 使用 KZTimer 封禁的 created_on
//   - expires_at 使用 KZTimer 封禁的 expires_on
//   - 这样本地封禁时间与全球 API 封禁时间一致
use crate::{
    db::Database,
    services::{external_api_service, observability_service},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Postgres;
use sqlx::QueryBuilder;
use std::{collections::HashMap, sync::OnceLock, time::Duration};
use uuid::Uuid;

const KZT_GLOBAL_BANS_API_KEY: &str = "kztimer_global_bans";
const KZT_GLOBAL_BANS_API_NAME: &str = "KZTimer GlobalAPI";
const KZT_GLOBAL_BAN_PAGE_LIMIT: i64 = 500;
const DEFAULT_KZT_GLOBAL_BAN_MAX_PAGES: i64 = 1000;
const GLOBAL_BAN_SYNC_MAX_PAGES_ENV: &str = "GLOBAL_BAN_SYNC_MAX_PAGES";

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

#[derive(Debug, Serialize)]
pub struct GlobalBanSyncStatus {
    pub sync_interval_secs: u64,
    pub stored_bans: i64,
    pub active_bans: i64,
    pub local_active_bans: i64,
    pub last_synced_at: Option<DateTime<Utc>>,
    pub task: Option<observability_service::TaskMetric>,
    pub external_api: Option<external_api_service::ExternalApiMetric>,
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
// HTTP 请求（统一外部 API 限流 + 冷却）
// =====================================================

static GLOBAL_BAN_SYNC_LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();

fn global_ban_sync_lock() -> &'static tokio::sync::Mutex<()> {
    GLOBAL_BAN_SYNC_LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

/// 从 KZTimer API 拉取封禁列表。
/// `expired_filter`: None=全部, Some(true)=仅过期, Some(false)=仅活跃
async fn fetch_kzt_bans(
    offset: i64,
    limit: i64,
    expired_filter: Option<bool>,
) -> anyhow::Result<Vec<KZTBan>> {
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

    external_api_service::get_json(
        KZT_GLOBAL_BANS_API_KEY,
        KZT_GLOBAL_BANS_API_NAME,
        &url,
        Duration::from_secs(20),
    )
    .await
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

    let local_rows =
        load_local_global_bans_for_steamids(db, std::slice::from_ref(steamid64)).await?;
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

pub async fn sync_status(
    db: &Database,
    sync_interval_secs: u64,
) -> anyhow::Result<GlobalBanSyncStatus> {
    let (stored_bans, active_bans, last_synced_at): (i64, i64, Option<DateTime<Utc>>) =
        sqlx::query_as(
            r#"SELECT
                 COUNT(*)::BIGINT,
                 COUNT(*) FILTER (WHERE is_expired = false)::BIGINT,
                 MAX(synced_at)
               FROM global_bans"#,
        )
        .fetch_one(&db.pool)
        .await?;

    let (local_active_bans,): (i64,) = sqlx::query_as(
        r#"SELECT COUNT(*)::BIGINT
           FROM ban_records
           WHERE source = 'global_ban' AND status = 'active'"#,
    )
    .fetch_one(&db.pool)
    .await?;

    Ok(GlobalBanSyncStatus {
        sync_interval_secs,
        stored_bans,
        active_bans,
        local_active_bans,
        last_synced_at,
        task: observability_service::task_metric("global_ban_sync"),
        external_api: external_api_service::metric(KZT_GLOBAL_BANS_API_KEY),
    })
}

// =====================================================
// 后台同步逻辑（全量同步 KZTimer 当前活跃封禁）
// =====================================================

/// 核心同步逻辑：
///   - 完整拉取 KZTimer 所有活跃封禁（isExpired=false）
///   - 拉取完整成功后才同步本地数据，避免 API/分页异常造成误解封
///   - 新增/更新每条全球封禁对应的 source='global_ban' 本地封禁
///   - KZTimer 中已解除/过期的封禁，只自动解除对应 source='global_ban' 本地封禁
pub async fn sync_global_bans(db: &Database) -> anyhow::Result<SyncResult> {
    let Ok(_guard) = global_ban_sync_lock().try_lock() else {
        tracing::warn!("全球封禁同步跳过：已有同步任务正在运行");
        anyhow::bail!("全球封禁同步正在运行，请稍后重试");
    };

    sync_global_bans_locked(db).await
}

async fn sync_global_bans_locked(db: &Database) -> anyhow::Result<SyncResult> {
    let all_bans = fetch_all_active_kzt_bans().await?;
    apply_authoritative_global_bans(db, &all_bans).await
}

async fn fetch_all_active_kzt_bans() -> anyhow::Result<Vec<KZTBan>> {
    fetch_all_active_kzt_bans_with(
        KZT_GLOBAL_BAN_PAGE_LIMIT,
        configured_kzt_global_ban_max_pages(),
        |offset, limit| fetch_kzt_bans(offset, limit, Some(false)),
    )
    .await
}

fn configured_kzt_global_ban_max_pages() -> i64 {
    match std::env::var(GLOBAL_BAN_SYNC_MAX_PAGES_ENV) {
        Ok(value) => match value.trim().parse::<i64>() {
            Ok(n) if n > 0 => n,
            Ok(n) => {
                tracing::warn!(
                    key = GLOBAL_BAN_SYNC_MAX_PAGES_ENV,
                    value = n,
                    default = DEFAULT_KZT_GLOBAL_BAN_MAX_PAGES,
                    "全球封禁分页上限必须大于 0，使用默认值"
                );
                DEFAULT_KZT_GLOBAL_BAN_MAX_PAGES
            }
            Err(_) => {
                tracing::warn!(
                    key = GLOBAL_BAN_SYNC_MAX_PAGES_ENV,
                    value = %value,
                    default = DEFAULT_KZT_GLOBAL_BAN_MAX_PAGES,
                    "全球封禁分页上限解析失败，使用默认值"
                );
                DEFAULT_KZT_GLOBAL_BAN_MAX_PAGES
            }
        },
        Err(_) => DEFAULT_KZT_GLOBAL_BAN_MAX_PAGES,
    }
}

async fn fetch_all_active_kzt_bans_with<F, Fut>(
    limit: i64,
    max_pages: i64,
    mut fetch_page: F,
) -> anyhow::Result<Vec<KZTBan>>
where
    F: FnMut(i64, i64) -> Fut,
    Fut: std::future::Future<Output = anyhow::Result<Vec<KZTBan>>>,
{
    let limit = limit.max(1);
    let max_pages = max_pages.max(1);
    let mut all_bans: Vec<KZTBan> = Vec::new();

    for page in 0..max_pages {
        let offset = page * limit;
        let bans = fetch_page(offset, limit).await.map_err(|error| {
            tracing::warn!(
                offset,
                already_fetched = all_bans.len(),
                %error,
                "KZTimer 全球封禁分页拉取失败，本轮不同步"
            );
            error
        })?;
        let len = bans.len();
        all_bans.extend(bans);
        if len < limit as usize {
            return Ok(all_bans);
        }
    }

    anyhow::bail!(
        "KZTimer 全球封禁分页超过 {max_pages} 页（已拉取 {} 条），本轮同步终止以避免不完整数据；可通过 {GLOBAL_BAN_SYNC_MAX_PAGES_ENV} 调整上限",
        all_bans.len()
    )
}

async fn apply_authoritative_global_bans(
    db: &Database,
    all_bans: &[KZTBan],
) -> anyhow::Result<SyncResult> {
    let mut result = SyncResult {
        total_fetched: all_bans.len() as i64,
        ..Default::default()
    };

    for ban in all_bans {
        let (local_ban_id, manual_unbanned) = upsert_global_ban_metadata(db, ban).await?;
        if manual_unbanned {
            continue;
        }

        let outcome = sync_local_ban_for_global(db, ban, local_ban_id).await?;
        if outcome.created {
            result.new_bans += 1;
        }
        if outcome.reactivated {
            result.re_banned += 1;
        }
        if local_ban_id != Some(outcome.local_id) {
            sqlx::query(
                r#"UPDATE global_bans
                   SET local_ban_id = $2, synced_at = now()
                   WHERE kzt_ban_id = $1"#,
            )
            .bind(ban.id)
            .bind(outcome.local_id)
            .execute(&db.pool)
            .await?;
        }
    }

    if all_bans.is_empty() {
        tracing::warn!("KZTimer 全球封禁本轮返回 0 条，跳过自动解除缺失记录以避免误解封");
    } else {
        let active_ids: Vec<i64> = all_bans.iter().map(|ban| ban.id).collect();
        result.expired = expire_missing_global_bans(db, &active_ids).await?;
    }

    Ok(result)
}

async fn upsert_global_ban_metadata(
    db: &Database,
    ban: &KZTBan,
) -> anyhow::Result<(Option<Uuid>, bool)> {
    let (local_ban_id, manual_unbanned): (Option<Uuid>, bool) = sqlx::query_as(
        r#"INSERT INTO global_bans (
             id, kzt_ban_id, steam_id64, player_name, steam_id, ban_type,
             notes, stats, server_id, expires_on, created_on, updated_on,
             is_expired, local_ban_id, manual_unbanned, synced_at
           )
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12,
                   false, NULL, false, now())
           ON CONFLICT (kzt_ban_id) DO UPDATE SET
             steam_id64 = EXCLUDED.steam_id64,
             player_name = EXCLUDED.player_name,
             steam_id = EXCLUDED.steam_id,
             ban_type = EXCLUDED.ban_type,
             notes = EXCLUDED.notes,
             stats = EXCLUDED.stats,
             server_id = EXCLUDED.server_id,
             expires_on = EXCLUDED.expires_on,
             created_on = EXCLUDED.created_on,
             updated_on = EXCLUDED.updated_on,
             is_expired = false,
             synced_at = now()
           RETURNING local_ban_id, manual_unbanned"#,
    )
    .bind(Uuid::new_v4())
    .bind(ban.id)
    .bind(&ban.steamid64)
    .bind(&ban.player_name)
    .bind(&ban.steam_id)
    .bind(&ban.ban_type)
    .bind(&ban.notes)
    .bind(&ban.stats)
    .bind(ban.server_id)
    .bind(&ban.expires_on)
    .bind(&ban.created_on)
    .bind(&ban.updated_on)
    .fetch_one(&db.pool)
    .await?;

    Ok((local_ban_id, manual_unbanned))
}

#[derive(Debug)]
struct LocalBanSyncOutcome {
    local_id: Uuid,
    created: bool,
    reactivated: bool,
}

async fn sync_local_ban_for_global(
    db: &Database,
    ban: &KZTBan,
    local_ban_id: Option<Uuid>,
) -> anyhow::Result<LocalBanSyncOutcome> {
    if let Some(local_id) = local_ban_id {
        let existing: Option<(String,)> = sqlx::query_as(
            "SELECT status FROM ban_records WHERE id = $1 AND source = 'global_ban'",
        )
        .bind(local_id)
        .fetch_optional(&db.pool)
        .await?;
        if let Some((status,)) = existing {
            update_local_ban_from_global(db, local_id, ban).await?;
            return Ok(LocalBanSyncOutcome {
                local_id,
                created: false,
                reactivated: status != "active",
            });
        }
    }

    let local_id = create_local_ban(
        db,
        &ban.steamid64,
        &ban.player_name,
        &ban.ban_type,
        &ban.notes,
        ban.created_on.as_deref(),
        ban.expires_on.as_deref(),
    )
    .await?;
    Ok(LocalBanSyncOutcome {
        local_id,
        created: true,
        reactivated: false,
    })
}

async fn update_local_ban_from_global(
    db: &Database,
    local_id: Uuid,
    ban: &KZTBan,
) -> anyhow::Result<()> {
    let (duration_minutes, reason, created_at, expires_at) = build_ban_meta(
        &ban.ban_type,
        &ban.notes,
        ban.created_on.as_deref(),
        ban.expires_on.as_deref(),
    );

    sqlx::query(
        r#"UPDATE ban_records
           SET player = $2,
               steam_id = $3,
               ban_type = 'steam',
               duration_minutes = $4,
               reason = $5,
               status = 'active',
               operator_name = 'KZTimer Global',
               source = 'global_ban',
               created_at = COALESCE($6, created_at),
               expires_at = $7,
               removed_reason = NULL,
               removed_by = NULL,
               removed_at = NULL
           WHERE id = $1 AND source = 'global_ban'"#,
    )
    .bind(local_id)
    .bind(&ban.player_name)
    .bind(&ban.steamid64)
    .bind(duration_minutes)
    .bind(reason)
    .bind(created_at)
    .bind(expires_at)
    .execute(&db.pool)
    .await?;

    Ok(())
}

async fn expire_missing_global_bans(db: &Database, active_ids: &[i64]) -> anyhow::Result<i64> {
    if active_ids.is_empty() {
        return Ok(0);
    }

    let result = sqlx::query(
        r#"WITH expired AS (
             UPDATE global_bans
             SET is_expired = true, synced_at = now()
             WHERE is_expired = false
               AND NOT (kzt_ban_id::BIGINT = ANY($1))
             RETURNING local_ban_id
           )
           UPDATE ban_records br
           SET status = 'inactive',
               removed_reason = '全球封禁已解除或过期，自动同步解封',
               removed_by = 'global_ban_sync',
               removed_at = now()
           FROM expired e
           WHERE br.id = e.local_ban_id
             AND br.status = 'active'
             AND br.source = 'global_ban'"#,
    )
    .bind(active_ids)
    .execute(&db.pool)
    .await?;

    Ok(result.rows_affected() as i64)
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
) -> anyhow::Result<Uuid> {
    // 解析封禁时间和到期时间（与 KZTimer 全球封禁保持一致）
    let (duration_minutes, reason, created_at, expires_at) =
        build_ban_meta(ban_type, notes, created_on, expires_on);

    let id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO ban_records (
               id, player, steam_id, ban_type, duration_minutes,
               reason, status, operator_name, source, created_at, expires_at
           )
           VALUES ($1, $2, $3, 'steam', $4, $5, 'active', 'KZTimer Global', 'global_ban', $6, $7)"#,
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

    Ok(id)
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
/// 设置 manual_unbanned=true（per-ban 标记），防止同一 KZTimer 封禁下次同步封回
/// 同一玩家后续产生新的 KZTimer 封禁 ID 时仍会同步。
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
            r#"UPDATE ban_records
               SET status = 'inactive', removed_by = $2, removed_at = now()
               WHERE id = $1 AND status = 'active' AND source = 'global_ban'"#,
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

        // 启动时执行一次全量同步
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::Config, db::Database};
    use uuid::Uuid;

    async fn with_test_db(test: impl AsyncFnOnce(Database) -> anyhow::Result<()>) {
        let config = Config::from_env();
        let base_url = config.database_url.clone();
        let schema = format!("test_{}", Uuid::new_v4().simple());
        let scoped_url = crate::test_util::schema_url(&base_url, &schema);
        crate::test_util::create_schema(&base_url, &schema).await;

        let result = async {
            let db = Database::connect_for_test(&scoped_url).await?;
            db.migrate().await?;
            test(db).await
        }
        .await;

        crate::test_util::drop_schema(&base_url, &schema).await;
        result.unwrap();
    }

    fn test_kzt_ban(
        id: i64,
        steamid64: &str,
        created_on: &str,
        expires_on: Option<&str>,
    ) -> KZTBan {
        KZTBan {
            id,
            ban_type: "cheat".to_string(),
            expires_on: expires_on.map(str::to_string),
            steamid64: steamid64.to_string(),
            player_name: Some(format!("Player {id}")),
            steam_id: None,
            notes: Some(format!("global ban {id}")),
            stats: None,
            server_id: None,
            created_on: Some(created_on.to_string()),
            updated_on: None,
        }
    }

    #[tokio::test]
    async fn active_ban_fetch_can_continue_past_legacy_100_page_limit() {
        let limit = 2;
        let active_pages = 101;
        let result =
            fetch_all_active_kzt_bans_with(limit, active_pages + 1, |offset, limit| async move {
                let page = offset / limit;
                if page >= active_pages {
                    return Ok(Vec::new());
                }

                Ok((0..limit)
                    .map(|index| {
                        test_kzt_ban(
                            offset + index + 1,
                            "76561198000000999",
                            "2099-06-30T00:00:00Z",
                            None,
                        )
                    })
                    .collect())
            })
            .await
            .expect("fetch should continue beyond 100 full pages");

        assert_eq!(result.len(), (limit * active_pages) as usize);
    }

    #[tokio::test]
    async fn active_ban_fetch_still_errors_when_page_limit_is_exhausted() {
        let error = fetch_all_active_kzt_bans_with(2, 3, |offset, limit| async move {
            Ok((0..limit)
                .map(|index| {
                    test_kzt_ban(
                        offset + index + 1,
                        "76561198000000998",
                        "2099-06-30T00:00:00Z",
                        None,
                    )
                })
                .collect())
        })
        .await
        .unwrap_err();

        let message = error.to_string();
        assert!(message.contains("分页超过 3 页"));
        assert!(message.contains(GLOBAL_BAN_SYNC_MAX_PAGES_ENV));
    }

    #[tokio::test]
    async fn new_global_ban_can_sync_after_previous_global_ban_was_unbanned() {
        with_test_db(async |db| {
            let steam_id = "76561198000000001";
            let old_local_id = Uuid::new_v4();
            sqlx::query(
                r#"INSERT INTO ban_records (
                       id, player, steam_id, ban_type, duration_minutes, reason,
                       status, operator_name, source, created_at, removed_at, removed_by
                   )
                   VALUES ($1, 'Player A', $2, 'steam', 0, '[全球封禁] old',
                           'inactive', 'KZTimer Global', 'global_ban', now(), now(), 'Admin')"#,
            )
            .bind(old_local_id)
            .bind(steam_id)
            .execute(&db.pool)
            .await?;

            let new_local_id = create_local_ban(
                &db,
                steam_id,
                &Some("Player A".to_string()),
                "cheat",
                &Some("new global ban".to_string()),
                Some("2099-06-30T00:00:00Z"),
                None,
            )
            .await?;

            let row: (String, String) =
                sqlx::query_as("SELECT status, source FROM ban_records WHERE id = $1")
                    .bind(new_local_id)
                    .fetch_one(&db.pool)
                    .await?;
            assert_eq!(row, ("active".to_string(), "global_ban".to_string()));

            Ok(())
        })
        .await;
    }

    #[tokio::test]
    async fn multiple_active_global_bans_create_multiple_local_records_for_same_player() {
        with_test_db(async |db| {
            let steam_id = "76561198000000002";
            let bans = vec![
                test_kzt_ban(
                    1001,
                    steam_id,
                    "2099-06-30T00:00:00Z",
                    Some("2099-07-30T00:00:00Z"),
                ),
                test_kzt_ban(1002, steam_id, "2099-06-30T00:00:03Z", None),
            ];

            let result = apply_authoritative_global_bans(&db, &bans).await?;
            assert_eq!(result.total_fetched, 2);
            assert_eq!(result.new_bans, 2);

            let (local_count,): (i64,) = sqlx::query_as(
                r#"SELECT COUNT(*)::BIGINT
                   FROM ban_records
                   WHERE steam_id = $1 AND source = 'global_ban' AND status = 'active'"#,
            )
            .bind(steam_id)
            .fetch_one(&db.pool)
            .await?;
            assert_eq!(local_count, 2);

            let (linked_count,): (i64,) = sqlx::query_as(
                r#"SELECT COUNT(*)::BIGINT
                   FROM global_bans
                   WHERE steam_id64 = $1 AND is_expired = false AND local_ban_id IS NOT NULL"#,
            )
            .bind(steam_id)
            .fetch_one(&db.pool)
            .await?;
            assert_eq!(linked_count, 2);

            Ok(())
        })
        .await;
    }

    #[tokio::test]
    async fn expiring_global_ban_only_unbans_synced_local_record() {
        with_test_db(async |db| {
            let steam_id = "76561198000000003";
            let global_ban = test_kzt_ban(
                2001,
                steam_id,
                "2099-06-01T00:00:00Z",
                Some("2099-07-01T00:00:00Z"),
            );
            apply_authoritative_global_bans(&db, &[global_ban]).await?;

            let (global_local_id,): (Option<Uuid>,) =
                sqlx::query_as("SELECT local_ban_id FROM global_bans WHERE kzt_ban_id = $1")
                    .bind(2001_i64)
                    .fetch_one(&db.pool)
                    .await?;
            let global_local_id =
                global_local_id.expect("global sync should link a local ban record");

            let manual_id = Uuid::new_v4();
            sqlx::query(
                r#"INSERT INTO ban_records (
                       id, player, steam_id, ban_type, duration_minutes, reason,
                       status, operator_name, source, created_at
                   )
                   VALUES ($1, 'Player Manual', $2, 'steam', 0, 'manual ban',
                           'active', 'Admin', 'manual', now())"#,
            )
            .bind(manual_id)
            .bind(steam_id)
            .execute(&db.pool)
            .await?;

            let other_active =
                test_kzt_ban(2002, "76561198000000004", "2099-06-02T00:00:00Z", None);
            let result = apply_authoritative_global_bans(&db, &[other_active]).await?;
            assert_eq!(result.expired, 1);

            let (global_status, global_removed_by): (String, Option<String>) =
                sqlx::query_as("SELECT status, removed_by FROM ban_records WHERE id = $1")
                    .bind(global_local_id)
                    .fetch_one(&db.pool)
                    .await?;
            assert_eq!(global_status, "inactive");
            assert_eq!(global_removed_by.as_deref(), Some("global_ban_sync"));

            let (manual_status,): (String,) =
                sqlx::query_as("SELECT status FROM ban_records WHERE id = $1")
                    .bind(manual_id)
                    .fetch_one(&db.pool)
                    .await?;
            assert_eq!(manual_status, "active");

            Ok(())
        })
        .await;
    }
}

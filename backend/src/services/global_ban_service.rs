// 全球封禁同步服务
// 从 KZTimer GlobalAPI (https://kztimerglobal.com/api/v2.0/bans) 同步封禁记录，
// 并在 ban_records 中创建本地封禁，使被全球封禁的玩家无法进入服务器。
//
// 去重保证：
//   1. global_bans 表以 kzt_ban_id 为 UNIQUE 键，同一条全球封禁只会有一条本地记录
//   2. 每次同步只拉取 isExpired=false 的活跃封禁，API 不返回的视为已过期
//   3. 已存在的 global_bans 记录只会做 UPDATE，不会重复创建本地封禁
//   4. create_local_ban 检查 source='global_ban' 活跃封禁 + partial unique index 保证
//      每个 steam_id 最多只有一条 source='global_ban' 的活跃封禁
//   5. 管理员手动解封后，设置 manual_unbanned=true，同步不会再重新封禁
//
// 过期处理：
//   - 同步时仅拉取 isExpired=false 的封禁，API 不再返回的封禁标记为 is_expired=true
//   - 过期封禁的 local_ban_id 保留（不置 NULL），指向 status='inactive' 的 ban_records
//   - 保留 local_ban_id 防止同步重复创建：下次同步时若 local_ban_id 不为 NULL
//     且指向的 ban_records 仍为 inactive，说明该封禁曾因全球过期被解封，不会误重封
//   - 如果 LumiAdmin 中有其他来源的活跃封禁，全球封禁过期不影响该玩家在 LumiAdmin 中的封禁状态
use crate::db::Database;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::QueryBuilder;
use sqlx::Postgres;
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
    /// 当前页是否满（用于判断是否有下一页）
    pub has_more: bool,
}

// =====================================================
// 实时查询（前端展示用）：直接从 KZTimer API 拉取
// =====================================================

/// 从 KZTimer API 拉取活跃封禁列表（仅未过期）
async fn fetch_active_kzt_bans(offset: i64, limit: i64) -> anyhow::Result<Vec<KZTBan>> {
    let client = crate::http_client::http_client();
    let url = format!(
        "https://kztimerglobal.com/api/v2.0/bans?isExpired=false&limit={}&offset={}",
        limit, offset
    );
    let resp = client.get(&url).send().await?;
    if !resp.status().is_success() {
        anyhow::bail!("KZTimer API 请求失败: status={}", resp.status());
    }
    let bans: Vec<KZTBan> = resp.json().await?;
    Ok(bans)
}

/// 实时获取 KZTimer 封禁列表并合并本地封禁状态（前端展示用）
pub async fn fetch_live_global_bans(
    db: &Database,
    page: i64,
    page_size: i64,
) -> anyhow::Result<LiveBanListResult> {
    let offset = (page - 1) * page_size;
    let bans = fetch_active_kzt_bans(offset, page_size).await?;
    let has_more = bans.len() as i64 == page_size;

    // 为每条封禁查询本地状态
    let mut items: Vec<LiveBanItem> = Vec::with_capacity(bans.len());
    for ban in &bans {
        let local: Option<(Option<Uuid>, bool)> = sqlx::query_as(
            "SELECT local_ban_id, manual_unbanned FROM global_bans WHERE kzt_ban_id = $1"
        )
        .bind(ban.id)
        .fetch_optional(&db.pool)
        .await
        .ok()
        .flatten();

        let (local_ban_id, manual_unbanned) = match local {
            Some((lid, mu)) => (lid, mu),
            None => (None, false),
        };

        items.push(LiveBanItem {
            ban: ban.clone(),
            local_ban_id,
            manual_unbanned,
        });
    }

    Ok(LiveBanListResult {
        items,
        page,
        page_size,
        has_more,
    })
}

// =====================================================
// 后台同步逻辑
// =====================================================

/// 核心同步逻辑：
///   - 拉取 KZTimer 所有活跃封禁（isExpired=false）
///   - 对每条封禁做 upsert（基于 kzt_ban_id）
///   - 只对新增的且未被手动解封的封禁创建本地 ban_records
///   - 标记本地已不再出现在 API 中的封禁为过期，解除本地封禁
///   - 过期封禁保留 local_ban_id 引用（指向 inactive 的 ban_records），防止重复创建
pub async fn sync_global_bans(db: &Database) -> anyhow::Result<SyncResult> {
    let mut result = SyncResult::default();

    // 1) 拉取 KZTimer 所有活跃封禁（isExpired=false，分页，最多 10000 条）
    let mut all_bans: Vec<KZTBan> = Vec::new();
    for offset in (0..10000).step_by(500) {
        let bans = fetch_active_kzt_bans(offset, 500).await?;
        let len = bans.len();
        all_bans.extend(bans);
        if len < 500 {
            break;
        }
    }
    result.total_fetched = all_bans.len() as i64;

    // 2) 收集 API 返回的所有 kzt_ban_id，用于后续检测过期
    let mut api_ban_ids: std::collections::HashSet<i64> = std::collections::HashSet::new();
    for ban in &all_bans {
        api_ban_ids.insert(ban.id);
    }

    // 3) 逐条 upsert
    for ban in &all_bans {
        let existing: Option<(Uuid, Option<Uuid>, bool, Option<String>)> = sqlx::query_as(
            "SELECT id, local_ban_id, manual_unbanned, ban_status FROM global_bans LEFT JOIN LATERAL (SELECT status AS ban_status FROM ban_records WHERE id = global_bans.local_ban_id) AS sub ON true WHERE kzt_ban_id = $1",
        )
        .bind(ban.id)
        .fetch_optional(&db.pool)
        .await?;

        if let Some((gid, local_id, was_manual_unbanned, ban_record_status)) = existing {
            // 已存在 → 仅更新字段
            sqlx::query(
                r#"UPDATE global_bans SET
                    player_name = $2, ban_type = $3, notes = $4, stats = $5,
                    expires_on = $6, updated_on = $7, is_expired = false, synced_at = now()
                WHERE id = $1"#,
            )
            .bind(gid)
            .bind(&ban.player_name)
            .bind(&ban.ban_type)
            .bind(&ban.notes)
            .bind(&ban.stats)
            .bind(&ban.expires_on)
            .bind(&ban.updated_on)
            .execute(&db.pool)
            .await?;

            // 封禁在 API 中仍为活跃 → 确保本地封禁也是活跃的
            if !was_manual_unbanned {
                if local_id.is_none() {
                    // 从未有本地封禁 → 创建
                    let lid = create_local_ban(
                        db,
                        &ban.steamid64,
                        &ban.player_name,
                        &ban.ban_type,
                        &ban.notes,
                    )
                    .await
                    .ok();
                    if let Some(Some(lid)) = lid {
                        sqlx::query("UPDATE global_bans SET local_ban_id = $1 WHERE id = $2")
                            .bind(lid)
                            .bind(gid)
                            .execute(&db.pool)
                            .await?;
                        result.re_banned += 1;
                    }
                } else if let Some(lid) = local_id {
                    if ban_record_status.as_deref() == Some("inactive") {
                        // 有本地封禁但已 inactive（之前因全球封禁过期被解封）→ 重新激活
                        reactivate_local_ban(db, lid).await.ok();
                        result.re_banned += 1;
                    }
                    // local_id 有值且 ban_records.status='active' → 无需操作
                }
            }
        } else {
            // 新封禁 → 插入 global_bans + 创建本地封禁
            let gid = Uuid::new_v4();
            let local_id =
                create_local_ban(db, &ban.steamid64, &ban.player_name, &ban.ban_type, &ban.notes)
                    .await
                    .ok();
            sqlx::query(
                r#"INSERT INTO global_bans (
                    id, kzt_ban_id, steam_id64, player_name, steam_id, ban_type,
                    notes, stats, server_id, expires_on, created_on, updated_on,
                    is_expired, local_ban_id, manual_unbanned, synced_at
                   )
                   VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, false, $13, false, now())"#,
            )
            .bind(gid)
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
            .bind(local_id)
            .execute(&db.pool)
            .await?;
            result.new_bans += 1;
        }
    }

    // 4) 标记本地已不再出现在 API 中的封禁为过期
    //    重要：local_ban_id 保留（不置 NULL），指向 status='inactive' 的 ban_records，
    //    这样下次同步时即使 API 延迟返回该封禁，也不会误创建新的本地封禁
    let locally_active: Vec<(Uuid, i64, Option<Uuid>)> = sqlx::query_as(
        "SELECT id, kzt_ban_id, local_ban_id FROM global_bans WHERE is_expired = false",
    )
    .fetch_all(&db.pool)
    .await?;

    for (gid, kzt_id, local_id) in locally_active {
        if !api_ban_ids.contains(&kzt_id) {
            sqlx::query("UPDATE global_bans SET is_expired = true, synced_at = now() WHERE id = $1")
                .bind(gid)
                .execute(&db.pool)
                .await?;
            if let Some(lid) = local_id {
                // 仅解除 global_ban 来源的封禁，不影响 LumiAdmin 中其他来源的封禁
                unban_local(db, lid).await.ok();
            }
            result.expired += 1;
        }
    }

    Ok(result)
}

/// 在 ban_records 中创建本地封禁（source = "global_ban"）
/// 返回 None 表示跳过（有其他来源的活跃封禁），返回 Some(id) 表示创建/复用成功
async fn create_local_ban(
    db: &Database,
    steam_id64: &str,
    player_name: &Option<String>,
    ban_type: &str,
    notes: &Option<String>,
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

    // 有其他来源的活跃封禁 → 不创建全球封禁来源的记录
    // LumiAdmin 封禁优先：即使全球封禁过期，玩家在 LumiAdmin 中仍被封禁
    let existing_other: Option<(Uuid,)> = sqlx::query_as(
        "SELECT id FROM ban_records WHERE steam_id = $1 AND source != 'global_ban' AND status = 'active'"
    )
    .bind(steam_id64)
    .fetch_optional(&db.pool)
    .await?;

    if existing_other.is_some() {
        tracing::info!(steam_id = steam_id64, "玩家已有 LumiAdmin 活跃封禁记录，跳过创建全球封禁来源记录");
        return Ok(None);
    }

    let reason = format!(
        "[全球封禁] {}{}",
        ban_type,
        notes.as_deref().map(|n| format!(" - {}", n)).unwrap_or_default()
    );
    let id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO ban_records (
               id, player, steam_id, ban_type, duration_minutes,
               reason, status, operator_name, source
           )
           VALUES ($1, $2, $3, 'steam', 0, $4, 'active', 'KZTimer Global', 'global_ban')
           ON CONFLICT DO NOTHING"#,
    )
    .bind(id)
    .bind(player_name)
    .bind(steam_id64)
    .bind(reason)
    .execute(&db.pool)
    .await?;

    Ok(Some(id))
}

/// 解除本地封禁（全球封禁过期时自动解封）
/// 仅解除 source='global_ban' 的封禁，不影响 LumiAdmin 中其他来源的封禁
async fn unban_local(db: &Database, ban_id: Uuid) -> anyhow::Result<()> {
    // 只解封 global_ban 来源且当前 active 的记录，避免误解封其他来源的封禁
    sqlx::query(
        "UPDATE ban_records SET status = 'inactive', removed_by = 'global_ban_sync', removed_at = now() WHERE id = $1 AND status = 'active' AND source = 'global_ban'",
    )
    .bind(ban_id)
    .execute(&db.pool)
    .await?;
    Ok(())
}

/// 重新激活本地封禁（全球封禁恢复时）
async fn reactivate_local_ban(db: &Database, ban_id: Uuid) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE ban_records SET status = 'active', removed_by = NULL, removed_at = NULL WHERE id = $1 AND status = 'inactive' AND source = 'global_ban'",
    )
    .bind(ban_id)
    .execute(&db.pool)
    .await?;
    Ok(())
}

/// 管理员手动解封全球封禁对应的本地封禁
/// 设置 manual_unbanned=true，防止下次同步重新封禁
pub async fn manual_unban(
    db: &Database,
    ban_cache: &crate::services::access_cache::ActiveBanCache,
    kzt_ban_id: i64,
    operator: &str,
) -> anyhow::Result<()> {
    // 查找本地 global_bans 记录
    let row: Option<(Uuid, Option<Uuid>)> = sqlx::query_as(
        "SELECT id, local_ban_id FROM global_bans WHERE kzt_ban_id = $1",
    )
    .bind(kzt_ban_id)
    .fetch_optional(&db.pool)
    .await?;

    let (gid, local_id) = row.ok_or_else(|| anyhow::anyhow!("全球封禁记录不存在"))?;

    // 解除 ban_records
    if let Some(lid) = local_id {
        sqlx::query(
            "UPDATE ban_records SET status = 'inactive', removed_by = $2, removed_at = now() WHERE id = $1 AND status = 'active'",
        )
        .bind(lid)
        .bind(operator)
        .execute(&db.pool)
        .await?;
    }

    // 标记为手动解封（不置 NULL local_ban_id，保留引用防止重封）
    sqlx::query(
        "UPDATE global_bans SET manual_unbanned = true WHERE id = $1",
    )
    .bind(gid)
    .execute(&db.pool)
    .await?;

    // 刷新封禁缓存
    ban_cache.refresh(db).await?;

    Ok(())
}

/// 清理误封禁数据：将 global_bans.is_expired=true 但 ban_records.status='active' 的记录修正为 inactive
/// 用于修复之前同步 bug 导致的数据错误
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

// 以下查询参数和函数仅用于历史记录查询（当前页面使用实时 API），保留以备将来使用
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
    pub fn page(&self) -> i64 { self.page.unwrap_or(1).max(1) }
    pub fn page_size(&self) -> i64 { self.page_size.unwrap_or(20).clamp(1, 100) }
}

#[allow(dead_code)]
fn push_filter_prefix(builder: &mut QueryBuilder<'_, Postgres>, has_where: &mut bool) {
    if *has_where { builder.push(" AND "); } else { builder.push(" WHERE "); *has_where = true; }
}

#[allow(dead_code)]
fn push_filters(builder: &mut QueryBuilder<'_, Postgres>, params: &GlobalBanQueryParams) {
    let mut has_where = false;
    if let Some(v) = params.steam_id64.as_ref().filter(|v| !v.trim().is_empty()) {
        push_filter_prefix(builder, &mut has_where);
        builder.push("steam_id64 = ").push_bind(v.trim().to_string());
    }
    if let Some(v) = params.ban_type.as_ref().filter(|v| !v.trim().is_empty()) {
        push_filter_prefix(builder, &mut has_where);
        builder.push("ban_type = ").push_bind(v.trim().to_string());
    }
    if let Some(v) = params.is_expired {
        push_filter_prefix(builder, &mut has_where);
        builder.push("is_expired = ").push_bind(v);
    }
    if let Some(v) = params.search.as_ref().map(|s| s.trim()).filter(|s| !s.is_empty()) {
        let pattern = format!("%{}%", v.replace('%', "\\%").replace('_', "\\_"));
        push_filter_prefix(builder, &mut has_where);
        builder
            .push("(steam_id64 ILIKE ").push_bind(pattern.clone())
            .push(" ESCAPE '\\' OR player_name ILIKE ").push_bind(pattern)
            .push(" ESCAPE '\\')");
    }
}

/// 同步结果
#[derive(Debug, Default, Serialize)]
pub struct SyncResult {
    pub total_fetched: i64,
    pub new_bans: i64,
    pub expired: i64,
    pub re_banned: i64,
}

/// 启动定时同步任务
pub fn start_global_ban_sync_loop(
    db: Database,
    ban_cache: std::sync::Arc<crate::services::access_cache::ActiveBanCache>,
    interval_secs: u64,
) {
    tokio::spawn(async move {
        // 启动时先清理之前可能的误封禁数据
        match cleanup_stale_global_bans(&db).await {
            Ok(0) => {},
            Ok(n) => {
                tracing::info!(count = n, "清理了因全球封禁过期但本地仍活跃的误封禁记录");
                if let Err(e) = ban_cache.refresh(&db).await {
                    tracing::warn!(%e, "清理后刷新封禁缓存失败");
                }
            }
            Err(e) => tracing::warn!(%e, "清理误封禁数据失败"),
        }

        match sync_global_bans(&db).await {
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
            match sync_global_bans(&db).await {
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

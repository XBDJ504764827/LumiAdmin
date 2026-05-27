use crate::db::Database;
use crate::models::{PublicBanItem, PublicWhitelistItem};
use crate::routes::ListQuery;
use serde::Serialize;
#[cfg(not(test))]
use std::sync::{Mutex, OnceLock};
#[cfg(not(test))]
use std::time::{Duration, Instant};

#[cfg(not(test))]
const PUBLIC_BAN_STATS_CACHE_TTL: Duration = Duration::from_secs(15);
#[cfg(not(test))]
static PUBLIC_BAN_STATS_CACHE: OnceLock<Mutex<Option<(Instant, BanStats)>>> = OnceLock::new();

#[derive(Debug, Clone, Serialize)]
pub struct BanStats {
    pub active: i64,
    pub permanent: i64,
    pub expired: i64,
}

pub async fn ban_stats(db: &Database) -> anyhow::Result<BanStats> {
    #[cfg(not(test))]
    if let Some(stats) = cached_ban_stats() {
        return Ok(stats);
    }

    let stats = ban_stats_uncached(db).await?;
    #[cfg(not(test))]
    {
        let cache = PUBLIC_BAN_STATS_CACHE.get_or_init(|| Mutex::new(None));
        if let Ok(mut guard) = cache.lock() {
            *guard = Some((Instant::now(), stats.clone()));
        }
    }
    Ok(stats)
}

#[cfg(not(test))]
fn cached_ban_stats() -> Option<BanStats> {
    let cache = PUBLIC_BAN_STATS_CACHE.get_or_init(|| Mutex::new(None));
    let guard = cache.lock().ok()?;
    let (created_at, stats) = guard.as_ref()?;
    if created_at.elapsed() <= PUBLIC_BAN_STATS_CACHE_TTL {
        return Some(stats.clone());
    }
    None
}

async fn ban_stats_uncached(db: &Database) -> anyhow::Result<BanStats> {
    let row: (i64, i64, i64) = sqlx::query_as(
        r#"SELECT
            COUNT(*) FILTER (WHERE status = 'active' AND duration_minutes > 0 AND (expires_at IS NULL OR expires_at > now())),
            COUNT(*) FILTER (WHERE status = 'active' AND duration_minutes = 0),
            COUNT(*) FILTER (WHERE status = 'inactive' OR (status = 'active' AND duration_minutes > 0 AND expires_at IS NOT NULL AND expires_at <= now()))
        FROM ban_records"#
    )
    .fetch_one(&db.pool)
    .await?;
    Ok(BanStats {
        active: row.0,
        permanent: row.1,
        expired: row.2,
    })
}

pub async fn list_public_whitelist(
    db: &Database,
    query: &ListQuery,
) -> anyhow::Result<crate::routes::PaginatedResponse<PublicWhitelistItem>> {
    let mut conditions = vec!["status = 'approved'".to_string()];
    let mut param_idx = 1u32;
    let search_pattern = query.search_pattern();

    if search_pattern.is_some() {
        conditions.push(format!(
            "(steamid64 ILIKE ${param_idx} OR nickname ILIKE ${param_idx})"
        ));
        param_idx += 1;
    }

    let where_clause = format!("WHERE {}", conditions.join(" AND "));

    let count_sql = format!("SELECT COUNT(*) FROM whitelist_requests {where_clause}");
    let data_sql = format!(
        r#"SELECT id, nickname, steamid64, applied_at, approved_at
           FROM whitelist_requests {where_clause}
           ORDER BY approved_at DESC NULLS LAST, applied_at DESC
           LIMIT ${param_idx} OFFSET ${}"#,
        param_idx + 1
    );

    let mut count_query = sqlx::query_scalar::<_, i64>(&count_sql);
    let mut data_query = sqlx::query_as::<_, PublicWhitelistItem>(&data_sql);

    if let Some(ref pattern) = search_pattern {
        count_query = count_query.bind(pattern);
        data_query = data_query.bind(pattern);
    }
    data_query = data_query.bind(query.page_size()).bind(query.offset());

    let total = count_query.fetch_one(&db.pool).await?;
    let items = data_query.fetch_all(&db.pool).await?;

    Ok(crate::routes::PaginatedResponse {
        items,
        total,
        page: query.page(),
        page_size: query.page_size(),
    })
}

pub async fn list_public_bans(
    db: &Database,
    query: &ListQuery,
) -> anyhow::Result<crate::routes::PaginatedResponse<PublicBanItem>> {
    // 显示 active 和 inactive（已过期）状态的封禁
    let mut conditions = vec!["status IN ('active', 'inactive')".to_string()];
    let mut param_idx = 1u32;
    let search_pattern = query.search_pattern();

    if search_pattern.is_some() {
        conditions.push(format!(
            "(player ILIKE ${param_idx} OR steam_id ILIKE ${param_idx})"
        ));
        param_idx += 1;
    }

    let where_clause = format!("WHERE {}", conditions.join(" AND "));

    let count_sql = format!("SELECT COUNT(*) FROM ban_records {where_clause}");
    let data_sql = format!(
        r#"SELECT id, COALESCE(player, steam_id) AS player, steam_id, server_name,
                  duration_minutes, expires_at, reason, status, created_at
           FROM ban_records {where_clause} ORDER BY created_at DESC
           LIMIT ${param_idx} OFFSET ${}"#,
        param_idx + 1
    );

    let mut count_query = sqlx::query_scalar::<_, i64>(&count_sql);
    let mut data_query = sqlx::query_as::<_, PublicBanItem>(&data_sql);

    if let Some(ref pattern) = search_pattern {
        count_query = count_query.bind(pattern);
        data_query = data_query.bind(pattern);
    }
    data_query = data_query.bind(query.page_size()).bind(query.offset());

    let total = count_query.fetch_one(&db.pool).await?;
    let items = data_query.fetch_all(&db.pool).await?;

    Ok(crate::routes::PaginatedResponse {
        items,
        total,
        page: query.page(),
        page_size: query.page_size(),
    })
}

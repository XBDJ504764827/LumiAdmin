use crate::{auth::session::role_label, db::Database, services::community_service};
use serde::Serialize;
#[cfg(not(test))]
use std::sync::{OnceLock, RwLock};
#[cfg(not(test))]
use std::time::{Duration, Instant};

#[cfg(not(test))]
const DASHBOARD_CACHE_TTL: Duration = Duration::from_secs(10);
const SERVER_PERFORMANCE_WINDOW_SECONDS: i64 = 300;
/// 仪表盘缓存使用 RwLock：读多写少（所有管理员共享、每 30s 轮询一次），
/// 读不阻塞读，避免 Mutex 在并发读时造成不必要的串行化。
#[cfg(not(test))]
static DASHBOARD_CACHE: OnceLock<RwLock<Option<(Instant, DashboardMetrics)>>> = OnceLock::new();

#[derive(Clone, Serialize)]
pub struct DashboardAdminPreview {
    pub display_name: String,
    pub role: String,
    pub role_label: String,
    pub status: String,
}

#[derive(Clone, Serialize)]
pub struct WhitelistStats {
    pub pending: i64,
    pub approved: i64,
    pub rejected: i64,
    pub revoked: i64,
}

#[derive(Clone, Serialize)]
pub struct ServerPerformanceStats {
    pub avg_fps: f32,
    pub avg_cpu_usage: f32,
    pub avg_tickrate: f32,
    pub total_players: i64,
    pub total_max_players: i64,
}

#[derive(Clone, Serialize)]
pub struct DashboardMetrics {
    pub total_servers: i64,
    pub online_servers: i64,
    pub offline_servers: i64,
    pub communities: i64,
    pub online_players: i64,
    pub admins: i64,
    pub admin_preview: Vec<DashboardAdminPreview>,
    pub whitelist_stats: WhitelistStats,
    pub server_performance: ServerPerformanceStats,
}

#[derive(Clone, Serialize)]
pub struct ReviewCounts {
    pub whitelist: i64,
    pub ban_appeal: i64,
    pub player_report: i64,
    pub map_feedback: i64,
    pub abnormal_record: i64,
}

pub async fn get_review_counts(
    db: &Database,
    include_reports: bool,
) -> anyhow::Result<ReviewCounts> {
    if include_reports {
        let counts: (i64, i64, i64, i64, i64) = sqlx::query_as(
            r#"SELECT
                (SELECT COUNT(*) FROM whitelist_requests WHERE status = 'pending') AS whitelist,
                (SELECT COUNT(*) FROM ban_appeals WHERE status = 'pending') AS ban_appeal,
                (SELECT COUNT(*) FROM player_reports WHERE status = 'pending') AS player_report,
                (SELECT COUNT(*) FROM map_feedback WHERE status = 'pending') AS map_feedback,
                (SELECT COUNT(*) FROM abnormal_records WHERE status = 'pending') AS abnormal_record"#,
        )
        .fetch_one(&db.pool)
        .await?;

        return Ok(ReviewCounts {
            whitelist: counts.0,
            ban_appeal: counts.1,
            player_report: counts.2,
            map_feedback: counts.3,
            abnormal_record: counts.4,
        });
    }

    let counts: (i64, i64) = sqlx::query_as(
        r#"SELECT
            (SELECT COUNT(*) FROM whitelist_requests WHERE status = 'pending') AS whitelist,
            (SELECT COUNT(*) FROM map_feedback WHERE status = 'pending') AS map_feedback"#,
    )
    .fetch_one(&db.pool)
    .await?;
    Ok(ReviewCounts {
        whitelist: counts.0,
        ban_appeal: 0,
        player_report: 0,
        map_feedback: counts.1,
        abnormal_record: 0,
    })
}

pub async fn get_metrics(db: &Database) -> anyhow::Result<DashboardMetrics> {
    #[cfg(not(test))]
    if let Some(metrics) = cached_metrics() {
        return Ok(metrics);
    }

    let metrics = get_metrics_uncached(db).await?;
    #[cfg(not(test))]
    {
        let cache = DASHBOARD_CACHE.get_or_init(|| RwLock::new(None));
        if let Ok(mut guard) = cache.write() {
            *guard = Some((Instant::now(), metrics.clone()));
        }
    }
    Ok(metrics)
}

#[cfg(not(test))]
fn cached_metrics() -> Option<DashboardMetrics> {
    let cache = DASHBOARD_CACHE.get_or_init(|| RwLock::new(None));
    // 读锁：多个并发请求可同时读取，互不阻塞
    let guard = cache.read().ok()?;
    let (created_at, metrics) = guard.as_ref()?;
    if created_at.elapsed() <= DASHBOARD_CACHE_TTL {
        return Some(metrics.clone());
    }
    None
}

async fn get_metrics_uncached(db: &Database) -> anyhow::Result<DashboardMetrics> {
    // 查询1: 服务器 + 社区组 + 在线玩家统计（合并为 1 个查询）
    let stale_after = community_service::stale_report_interval_sql();
    let stats: (i64, i64, i64, i64) = sqlx::query_as(
        r#"SELECT
            (SELECT COUNT(*) FROM servers) AS total_servers,
            (SELECT COUNT(*) FROM servers WHERE status = 'online' AND (last_reported_at IS NULL OR last_reported_at > now() - $1::INTERVAL)) AS online_servers,
            (SELECT COUNT(*) FROM communities) AS communities,
            (SELECT COALESCE(SUM(cardinality(players)), 0)::BIGINT FROM servers WHERE status = 'online' AND (last_reported_at IS NULL OR last_reported_at > now() - $1::INTERVAL)) AS online_players"#,
    )
    .bind(&stale_after)
    .fetch_one(&db.pool)
    .await?;
    let (total_servers, online_servers, communities, online_players) = stats;
    let offline_servers = total_servers - online_servers;

    // 查询2: 管理员预览
    let admin_rows: Vec<(String, String)> = sqlx::query_as(
        r#"SELECT COALESCE(NULLIF(remark, ''), username) AS display_name, role
           FROM users
           WHERE role IN ('admin', 'developer', 'normal')
           ORDER BY created_at DESC"#,
    )
    .fetch_all(&db.pool)
    .await?;
    let admins = admin_rows.len() as i64;
    let admin_preview = admin_rows
        .into_iter()
        .map(|(display_name, role)| DashboardAdminPreview {
            role_label: role_label(&role).to_string(),
            display_name,
            role,
            status: "可用".to_string(),
        })
        .collect();

    // 查询3: 白名单统计（条件 COUNT 合并为 1 个查询）
    let whitelist_stats: (i64, i64, i64, i64) = sqlx::query_as(
        r#"SELECT
            COUNT(*) FILTER (WHERE status = 'pending'),
            COUNT(*) FILTER (WHERE status = 'approved'),
            COUNT(*) FILTER (WHERE status = 'rejected'),
            COUNT(*) FILTER (WHERE status = 'revoked')
           FROM whitelist_requests"#,
    )
    .fetch_one(&db.pool)
    .await?;

    // 查询4: 服务器性能指标
    let server_performance = get_server_performance_stats(db).await?;

    Ok(DashboardMetrics {
        total_servers,
        online_servers,
        offline_servers,
        communities,
        online_players,
        admins,
        admin_preview,
        whitelist_stats: WhitelistStats {
            pending: whitelist_stats.0,
            approved: whitelist_stats.1,
            rejected: whitelist_stats.2,
            revoked: whitelist_stats.3,
        },
        server_performance,
    })
}

async fn get_server_performance_stats(db: &Database) -> anyhow::Result<ServerPerformanceStats> {
    let performance_window = format!("{SERVER_PERFORMANCE_WINDOW_SECONDS} seconds");
    let result: Option<(f32, f32, f32, i64, i64)> = sqlx::query_as(
        r#"
        SELECT
            COALESCE(AVG(ssh.fps), 0)::REAL,
            COALESCE(AVG(ssh.cpu_usage), 0)::REAL,
            COALESCE(AVG(ssh.tickrate), 0)::REAL,
            COALESCE(SUM(ssh.players_count), 0)::BIGINT,
            COALESCE(SUM(ssh.max_players), 0)::BIGINT
        FROM (
            SELECT DISTINCT ON (ssh.server_id)
                ssh.fps,
                ssh.cpu_usage,
                ssh.tickrate,
                ssh.players_count,
                ssh.max_players,
                ssh.reported_at
            FROM server_status_history ssh
            JOIN servers s ON s.id = ssh.server_id
            WHERE s.status = 'online'
              AND ssh.reported_at > now() - $1::INTERVAL
            ORDER BY ssh.server_id, ssh.reported_at DESC
        ) ssh
        "#,
    )
    .bind(&performance_window)
    .fetch_optional(&db.pool)
    .await?;

    match result {
        Some((avg_fps, avg_cpu_usage, avg_tickrate, total_players, total_max_players)) => {
            Ok(ServerPerformanceStats {
                avg_fps,
                avg_cpu_usage,
                avg_tickrate,
                total_players,
                total_max_players,
            })
        }
        None => Ok(ServerPerformanceStats {
            avg_fps: 0.0,
            avg_cpu_usage: 0.0,
            avg_tickrate: 0.0,
            total_players: 0,
            total_max_players: 0,
        }),
    }
}

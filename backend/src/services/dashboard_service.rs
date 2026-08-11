use crate::{auth::session::role_label, db::Database, services::community_service};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
#[cfg(not(test))]
use std::collections::HashMap;
#[cfg(not(test))]
use std::sync::{Mutex, OnceLock, RwLock};
#[cfg(not(test))]
use std::time::{Duration, Instant};

#[cfg(not(test))]
const DASHBOARD_CACHE_TTL: Duration = Duration::from_secs(10);
#[cfg(not(test))]
const ANALYTICS_CACHE_TTL: Duration = Duration::from_secs(30);
const SERVER_PERFORMANCE_WINDOW_SECONDS: i64 = 300;
/// 仪表盘缓存使用 RwLock：读多写少（所有管理员共享、每 30s 轮询一次），
/// 读不阻塞读，避免 Mutex 在并发读时造成不必要的串行化。
#[cfg(not(test))]
static DASHBOARD_CACHE: OnceLock<RwLock<Option<(Instant, DashboardMetrics)>>> = OnceLock::new();
/// Analytics 图表缓存：键为「端点 + 参数」，如 `whitelist-trend:30`、`server-activity:7d`。
/// 聚合查询按 30s 缓存，避免管理员高频刷新时反复执行大范围聚合。
#[cfg(not(test))]
static ANALYTICS_CACHE: OnceLock<Mutex<HashMap<String, (Instant, serde_json::Value)>>> =
    OnceLock::new();

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
pub struct DashboardOverviewStats {
    pub whitelist_total: i64,
    pub whitelist_weekly_new: i64,
    pub whitelist_today_new: i64,
    pub whitelist_yesterday_new: i64,
    pub players_today_active: i64,
    pub players_yesterday_active: i64,
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
    pub analytics: DashboardOverviewStats,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct WhitelistTrendItem {
    pub date: String,
    pub count: i64,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct WhitelistTrendResponse {
    pub days: i64,
    pub items: Vec<WhitelistTrendItem>,
}

#[derive(Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ServerActivityItem {
    pub time: DateTime<Utc>,
    pub active_players: i64,
    pub sessions: i64,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ServerActivityResponse {
    pub range: String,
    pub unit: String,
    pub items: Vec<ServerActivityItem>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ServerStatusItem {
    pub status: String,
    pub count: i64,
}

#[derive(Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ServerRankingItem {
    pub server_name: String,
    pub active_players: i64,
    pub sessions: i64,
    pub playtime_seconds: i64,
}

#[derive(Clone, Serialize)]
pub struct ReviewCounts {
    pub whitelist: i64,
    pub abnormal_record: i64,
}

pub async fn get_review_counts(db: &Database) -> anyhow::Result<ReviewCounts> {
    let counts: (i64, i64) = sqlx::query_as(
        r#"SELECT
            (SELECT COUNT(*) FROM whitelist_requests WHERE status = 'pending') AS whitelist,
            (SELECT COUNT(*) FROM abnormal_records WHERE status = 'pending') AS abnormal_record"#,
    )
    .fetch_one(&db.pool)
    .await?;

    Ok(ReviewCounts {
        whitelist: counts.0,
        abnormal_record: counts.1,
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

    // 查询5: 顶部趋势统计卡片
    let overview = get_overview_stats(db).await?;

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
        analytics: DashboardOverviewStats {
            whitelist_total: whitelist_stats.1,
            ..overview
        },
    })
}

async fn get_overview_stats(db: &Database) -> anyhow::Result<DashboardOverviewStats> {
    let today_start: DateTime<Utc> = sqlx::query_scalar(
        r#"SELECT date_trunc('day', timezone('Asia/Shanghai', now())) AT TIME ZONE 'Asia/Shanghai'"#,
    )
    .fetch_one(&db.pool)
    .await?;

    let (whitelist_today_new, whitelist_yesterday_new): (i64, i64) = sqlx::query_as(
        r#"SELECT
            COUNT(*) FILTER (WHERE status = 'approved' AND approved_at >= $1 AND approved_at < $1 + interval '1 day'),
            COUNT(*) FILTER (WHERE status = 'approved' AND approved_at >= $1 - interval '1 day' AND approved_at < $1)
           FROM whitelist_requests"#,
    )
    .bind(today_start)
    .fetch_one(&db.pool)
    .await?;

    let whitelist_weekly_new: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM whitelist_requests
           WHERE status = 'approved' AND approved_at >= now() - interval '7 days'"#,
    )
    .fetch_one(&db.pool)
    .await?;

    let (players_today_active, players_yesterday_active): (i64, i64) = sqlx::query_as(
        r#"SELECT
            (SELECT COUNT(DISTINCT steam_id64) FROM player_server_sessions
              WHERE first_seen_at < now() AND (left_at IS NULL OR left_at > $1)),
            (SELECT COUNT(DISTINCT steam_id64) FROM player_server_sessions
              WHERE first_seen_at < $1 AND (left_at IS NULL OR left_at > $1 - interval '1 day'))"#,
    )
    .bind(today_start)
    .fetch_one(&db.pool)
    .await?;

    Ok(DashboardOverviewStats {
        whitelist_total: 0,
        whitelist_weekly_new,
        whitelist_today_new,
        whitelist_yesterday_new,
        players_today_active,
        players_yesterday_active,
    })
}

/// 白名单增长趋势：按「通过时间（Asia/Shanghai）」统计每日新增白名单数量。
/// `days` 取值范围由路由层约束为 7 / 30 / 90。
pub async fn get_whitelist_trend(
    db: &Database,
    days: i64,
) -> anyhow::Result<WhitelistTrendResponse> {
    #[cfg(not(test))]
    if let Some(cached) = cached_analytics(&format!("whitelist-trend:{days}")) {
        return Ok(serde_json::from_value(cached)?);
    }

    let start: DateTime<Utc> = sqlx::query_scalar(
        r#"SELECT (date_trunc('day', timezone('Asia/Shanghai', now())) AT TIME ZONE 'Asia/Shanghai') - make_interval(days => ($1)::int)"#,
    )
    .bind(days - 1)
    .fetch_one(&db.pool)
    .await?;
    let end: DateTime<Utc> = sqlx::query_scalar(
        r#"SELECT (date_trunc('day', timezone('Asia/Shanghai', now())) AT TIME ZONE 'Asia/Shanghai') + interval '1 day'"#,
    )
    .fetch_one(&db.pool)
    .await?;

    let rows: Vec<(String, i64)> = sqlx::query_as(
        r#"SELECT to_char(timezone('Asia/Shanghai', approved_at), 'YYYY-MM-DD') AS date, COUNT(*) AS count
           FROM whitelist_requests
          WHERE status = 'approved'
            AND approved_at >= $1
            AND approved_at < $2
          GROUP BY 1
          ORDER BY 1"#,
    )
    .bind(start)
    .bind(end)
    .fetch_all(&db.pool)
    .await?;

    let items = rows
        .into_iter()
        .map(|(date, count)| WhitelistTrendItem { date, count })
        .collect();
    let response = WhitelistTrendResponse { days, items };

    #[cfg(not(test))]
    store_analytics_cache(&format!("whitelist-trend:{days}"), &response);
    Ok(response)
}

/// 服务器活跃度趋势：
/// - `today`：今日 00:00（Asia/Shanghai）到当前小时，按小时分桶
/// - `1d`：最近 24 小时，按小时分桶
/// - `7d` / `30d` / `90d`：按天分桶
pub async fn get_server_activity(
    db: &Database,
    range: &str,
) -> anyhow::Result<ServerActivityResponse> {
    #[cfg(not(test))]
    if let Some(cached) = cached_analytics(&format!("server-activity:{range}")) {
        return Ok(serde_json::from_value(cached)?);
    }

    let (unit, items) = match range {
        "today" | "1d" => {
            let start: DateTime<Utc> = sqlx::query_scalar(
                r#"SELECT
                    CASE WHEN $1 = 'today'
                      THEN date_trunc('day', timezone('Asia/Shanghai', now())) AT TIME ZONE 'Asia/Shanghai'
                      ELSE date_trunc('hour', timezone('Asia/Shanghai', now())) AT TIME ZONE 'Asia/Shanghai' - interval '23 hours'
                    END"#,
            )
            .bind(range)
            .fetch_one(&db.pool)
            .await?;
            let items = get_activity_hourly(db, start).await?;
            ("hour".to_string(), items)
        }
        _ => {
            let days: i64 = match range {
                "7d" => 7,
                "30d" => 30,
                "90d" => 90,
                _ => 7,
            };
            let items = get_activity_daily(db, days).await?;
            ("day".to_string(), items)
        }
    };

    let response = ServerActivityResponse {
        range: range.to_string(),
        unit,
        items,
    };
    #[cfg(not(test))]
    store_analytics_cache(&format!("server-activity:{range}"), &response);
    Ok(response)
}

async fn get_activity_hourly(
    db: &Database,
    start: DateTime<Utc>,
) -> anyhow::Result<Vec<ServerActivityItem>> {
    let items = sqlx::query_as::<_, ServerActivityItem>(
        r#"WITH hours AS (
             SELECT generate_series(
                      $1,
                      date_trunc('hour', timezone('Asia/Shanghai', now())) AT TIME ZONE 'Asia/Shanghai',
                      interval '1 hour'
                    ) AS hour_start
           )
           SELECT h.hour_start AS time,
                  COUNT(DISTINCT pss.steam_id64) AS active_players,
                  COUNT(pss.id) AS sessions
           FROM hours h
           LEFT JOIN player_server_sessions pss
             ON pss.first_seen_at < h.hour_start + interval '1 hour'
            AND (pss.left_at IS NULL OR pss.left_at > h.hour_start)
           GROUP BY h.hour_start
           ORDER BY h.hour_start"#,
    )
    .bind(start)
    .fetch_all(&db.pool)
    .await?;
    Ok(items)
}

async fn get_activity_daily(db: &Database, days: i64) -> anyhow::Result<Vec<ServerActivityItem>> {
    let items = sqlx::query_as::<_, ServerActivityItem>(
        r#"WITH days AS (
             SELECT generate_series(
                      (date_trunc('day', timezone('Asia/Shanghai', now())) AT TIME ZONE 'Asia/Shanghai') - make_interval(days => ($1)::int),
                      (date_trunc('day', timezone('Asia/Shanghai', now())) AT TIME ZONE 'Asia/Shanghai'),
                      interval '1 day'
                    ) AS day_start
           )
           SELECT d.day_start AS time,
                  COUNT(DISTINCT pss.steam_id64) AS active_players,
                  COUNT(pss.id) AS sessions
           FROM days d
           LEFT JOIN player_server_sessions pss
             ON pss.first_seen_at < d.day_start + interval '1 day'
            AND (pss.left_at IS NULL OR pss.left_at > d.day_start)
           GROUP BY d.day_start
           ORDER BY d.day_start"#,
    )
    .bind(days - 1)
    .fetch_all(&db.pool)
    .await?;
    Ok(items)
}

/// 服务器状态分布：与 /api/dashboard 相同的口径（在线 = status='online' 且上报新鲜），
/// 并区分休眠（hibernating）、离线（offline/上报过期）、未测试（untested）。
pub async fn get_server_status_distribution(
    db: &Database,
) -> anyhow::Result<Vec<ServerStatusItem>> {
    #[cfg(not(test))]
    if let Some(cached) = cached_analytics("server-status") {
        return Ok(serde_json::from_value(cached)?);
    }

    let stale_after = community_service::stale_report_interval_sql();
    let counts: (i64, i64, i64, i64) = sqlx::query_as(
        r#"SELECT
            COUNT(*) FILTER (WHERE status = 'online' AND (last_reported_at IS NULL OR last_reported_at > now() - $1::INTERVAL)),
            COUNT(*) FILTER (WHERE status = 'hibernating'),
            COUNT(*) FILTER (WHERE status = 'untested'),
            COUNT(*) FILTER (WHERE status = 'offline' OR (status = 'online' AND (last_reported_at IS NULL OR last_reported_at <= now() - $1::INTERVAL)))
           FROM servers"#,
    )
    .bind(&stale_after)
    .fetch_one(&db.pool)
    .await?;

    let mut items = vec![
        ServerStatusItem {
            status: "online".to_string(),
            count: counts.0,
        },
        ServerStatusItem {
            status: "hibernating".to_string(),
            count: counts.1,
        },
        ServerStatusItem {
            status: "untested".to_string(),
            count: counts.2,
        },
        ServerStatusItem {
            status: "offline".to_string(),
            count: counts.3,
        },
    ];
    items.retain(|item| item.count > 0);

    #[cfg(not(test))]
    store_analytics_cache("server-status", &items);
    Ok(items)
}

/// 服务器活跃度排行：按窗口内去重活跃玩家数排序，辅助指标为会话数与在线时长。
pub async fn get_server_ranking(
    db: &Database,
    days: i64,
    limit: i64,
) -> anyhow::Result<Vec<ServerRankingItem>> {
    #[cfg(not(test))]
    if let Some(cached) = cached_analytics(&format!("server-ranking:{days}:{limit}")) {
        return Ok(serde_json::from_value(cached)?);
    }

    let items = sqlx::query_as::<_, ServerRankingItem>(
        r#"SELECT s.name AS server_name,
                  COUNT(DISTINCT pss.steam_id64) AS active_players,
                  COUNT(pss.id) AS sessions,
                  COALESCE(SUM(EXTRACT(EPOCH FROM (
                    LEAST(COALESCE(pss.left_at, now()), now()) - GREATEST(pss.first_seen_at, now() - make_interval(days => ($1)::int))
                  ))), 0)::BIGINT AS playtime_seconds
           FROM player_server_sessions pss
           JOIN servers s ON s.id = pss.server_id
           WHERE pss.first_seen_at < now()
             AND (pss.left_at IS NULL OR pss.left_at > now() - make_interval(days => ($1)::int))
           GROUP BY s.id, s.name
           ORDER BY active_players DESC, playtime_seconds DESC
           LIMIT $2"#,
    )
    .bind(days)
    .bind(limit)
    .fetch_all(&db.pool)
    .await?;

    #[cfg(not(test))]
    store_analytics_cache(&format!("server-ranking:{days}:{limit}"), &items);
    Ok(items)
}

#[cfg(not(test))]
fn cached_analytics(key: &str) -> Option<serde_json::Value> {
    let cache = ANALYTICS_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let guard = cache.lock().ok()?;
    let (created_at, value) = guard.get(key)?;
    if created_at.elapsed() <= ANALYTICS_CACHE_TTL {
        return Some(value.clone());
    }
    None
}

#[cfg(not(test))]
fn store_analytics_cache(key: &str, value: &impl Serialize) {
    let cache = ANALYTICS_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let Ok(serialized) = serde_json::to_value(value) else {
        return;
    };
    if let Ok(mut guard) = cache.lock() {
        guard.insert(key.to_string(), (Instant::now(), serialized));
    }
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

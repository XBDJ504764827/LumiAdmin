use crate::db::Database;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Deserialize)]
pub struct ServerStatusInput {
    pub report_token: String,
    pub port: i32,
    pub fps: f32,
    pub cpu_usage: f32,
    pub tickrate: f32,
    #[serde(default)]
    pub in_rate: f32,
    #[serde(default)]
    pub out_rate: f32,
    #[serde(default)]
    pub uptime_seconds: i64,
    #[serde(default)]
    pub players_count: i32,
    #[serde(default)]
    pub max_players: i32,
    #[serde(default)]
    pub current_map: String,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize)]
pub struct ServerStatusItem {
    pub server_id: Uuid,
    pub server_name: String,
    pub fps: f32,
    pub cpu_usage: f32,
    pub tickrate: f32,
    pub in_rate: f32,
    pub out_rate: f32,
    pub uptime_seconds: i64,
    pub players_count: i32,
    pub max_players: i32,
    pub current_map: String,
    pub reported_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ServerStatusReportResult {
    pub server_id: Uuid,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize)]
pub struct ServerPerformanceMetrics {
    pub avg_fps: f32,
    pub avg_cpu_usage: f32,
    pub avg_tickrate: f32,
    pub total_players: i64,
    pub total_max_players: i64,
    pub servers: Vec<ServerStatusItem>,
}

pub async fn report_server_status(
    db: &Database,
    input: ServerStatusInput,
) -> anyhow::Result<ServerStatusReportResult> {
    let report_token = input.report_token.trim();
    anyhow::ensure!(!report_token.is_empty(), "report_token 不能为空");

    let server_id: Uuid = sqlx::query_as::<_, (Uuid,)>(
        r#"
        SELECT id
        FROM servers
        WHERE report_token = $1 AND port = $2
        "#,
    )
    .bind(report_token)
    .bind(input.port)
    .fetch_optional(&db.pool)
    .await?
    .map(|row| row.0)
    .ok_or_else(|| anyhow::anyhow!("服务器 token 或端口不匹配"))?;

    sqlx::query(
        r#"
        INSERT INTO server_status_history (
            id, server_id, fps, cpu_usage, tickrate, in_rate, out_rate,
            uptime_seconds, players_count, max_players, current_map, reported_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, now())
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(server_id)
    .bind(input.fps)
    .bind(input.cpu_usage)
    .bind(input.tickrate)
    .bind(input.in_rate)
    .bind(input.out_rate)
    .bind(input.uptime_seconds)
    .bind(input.players_count)
    .bind(input.max_players)
    .bind(&input.current_map)
    .execute(&db.pool)
    .await?;

    sqlx::query(
        r#"
        UPDATE servers
        SET status = 'online',
            last_reported_at = now(),
            max_players = GREATEST(max_players, $2),
            players = COALESCE((
                SELECT ARRAY_AGG(name ORDER BY name)
                FROM server_online_players
                WHERE server_id = $1
            ), ARRAY[]::TEXT[])
        WHERE id = $1
        "#,
    )
    .bind(server_id)
    .bind(input.max_players)
    .execute(&db.pool)
    .await?;

    Ok(ServerStatusReportResult { server_id })
}

/// 清理过期的服务器状态历史记录
/// 只保留最近 `retention_secs` 秒的数据，删除更早的记录
pub async fn cleanup_old_status_history(
    db: &Database,
    retention_secs: i64,
) -> anyhow::Result<u64> {
    let result = sqlx::query(
        r#"DELETE FROM server_status_history WHERE reported_at < now() - make_interval(secs => $1)"#,
    )
    .bind(retention_secs)
    .execute(&db.pool)
    .await?;

    Ok(result.rows_affected())
}

/// 启动服务器状态历史清理循环
/// `interval_secs` 为清理执行间隔（秒），`retention_secs` 为数据保留时长（秒）
pub fn start_status_history_cleanup_loop(
    db: Database,
    interval_secs: u64,
    retention_secs: i64,
) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(interval_secs));
        // 首次 tick 立即触发，等待一个周期后再开始
        interval.tick().await;
        loop {
            interval.tick().await;
            match cleanup_old_status_history(&db, retention_secs).await {
                Ok(0) => {}
                Ok(count) => {
                    tracing::info!(count, retention_secs, "清理过期服务器状态历史")
                }
                Err(e) => tracing::warn!(%e, "清理服务器状态历史失败"),
            }
        }
    });
}

#[allow(dead_code)]
pub async fn get_latest_status(db: &Database) -> anyhow::Result<Vec<ServerStatusItem>> {
    let rows = sqlx::query_as::<_, ServerStatusRow>(
        r#"
        SELECT DISTINCT ON (ssh.server_id)
            ssh.server_id,
            s.name AS server_name,
            ssh.fps,
            ssh.cpu_usage,
            ssh.tickrate,
            ssh.in_rate,
            ssh.out_rate,
            ssh.uptime_seconds,
            ssh.players_count,
            ssh.max_players,
            ssh.current_map,
            ssh.reported_at
        FROM server_status_history ssh
        JOIN servers s ON s.id = ssh.server_id
        WHERE s.status = 'online'
          AND ssh.reported_at > now() - interval '5 minutes'
        ORDER BY ssh.server_id, ssh.reported_at DESC
        "#,
    )
    .fetch_all(&db.pool)
    .await?;

    Ok(rows.into_iter().map(Into::into).collect())
}

#[allow(dead_code)]
pub async fn get_performance_metrics(db: &Database) -> anyhow::Result<ServerPerformanceMetrics> {
    let rows = get_latest_status(db).await?;

    if rows.is_empty() {
        return Ok(ServerPerformanceMetrics {
            avg_fps: 0.0,
            avg_cpu_usage: 0.0,
            avg_tickrate: 0.0,
            total_players: 0,
            total_max_players: 0,
            servers: Vec::new(),
        });
    }

    let total_fps: f32 = rows.iter().map(|r| r.fps).sum();
    let total_cpu: f32 = rows.iter().map(|r| r.cpu_usage).sum();
    let total_tick: f32 = rows.iter().map(|r| r.tickrate).sum();
    let total_players: i64 = rows.iter().map(|r| r.players_count as i64).sum();
    let total_max_players: i64 = rows.iter().map(|r| r.max_players as i64).sum();
    let count = rows.len() as f32;

    Ok(ServerPerformanceMetrics {
        avg_fps: total_fps / count,
        avg_cpu_usage: total_cpu / count,
        avg_tickrate: total_tick / count,
        total_players,
        total_max_players,
        servers: rows,
    })
}

#[allow(dead_code)]
#[derive(Debug, sqlx::FromRow)]
struct ServerStatusRow {
    server_id: Uuid,
    server_name: String,
    fps: f32,
    cpu_usage: f32,
    tickrate: f32,
    in_rate: f32,
    out_rate: f32,
    uptime_seconds: i64,
    players_count: i32,
    max_players: i32,
    current_map: String,
    reported_at: chrono::DateTime<chrono::Utc>,
}

impl From<ServerStatusRow> for ServerStatusItem {
    fn from(row: ServerStatusRow) -> Self {
        Self {
            server_id: row.server_id,
            server_name: row.server_name,
            fps: row.fps,
            cpu_usage: row.cpu_usage,
            tickrate: row.tickrate,
            in_rate: row.in_rate,
            out_rate: row.out_rate,
            uptime_seconds: row.uptime_seconds,
            players_count: row.players_count,
            max_players: row.max_players,
            current_map: row.current_map,
            reported_at: row.reported_at.to_rfc3339(),
        }
    }
}

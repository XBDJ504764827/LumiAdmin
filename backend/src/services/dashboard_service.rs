use crate::{auth::session::role_label, db::Database};
use serde::Serialize;

#[derive(Serialize)]
pub struct DashboardAdminPreview {
    pub display_name: String,
    pub role: String,
    pub role_label: String,
    pub status: String,
}

#[derive(Serialize)]
pub struct WhitelistStats {
    pub pending: i64,
    pub approved: i64,
    pub rejected: i64,
    pub revoked: i64,
}

#[derive(Serialize)]
pub struct ServerPerformanceStats {
    pub avg_fps: f32,
    pub avg_cpu_usage: f32,
    pub avg_tickrate: f32,
    pub total_players: i64,
    pub total_max_players: i64,
}

#[derive(Serialize)]
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

pub async fn get_metrics(db: &Database) -> anyhow::Result<DashboardMetrics> {
    let total_servers: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM servers").fetch_one(&db.pool).await?;
    let online_servers: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM servers WHERE status = 'online'")
        .fetch_one(&db.pool)
        .await?;
    let offline_servers = total_servers.0 - online_servers.0;
    let communities: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM communities").fetch_one(&db.pool).await?;
    let online_players: (i64,) = sqlx::query_as(
        "SELECT COALESCE(SUM(cardinality(players)), 0)::BIGINT FROM servers WHERE status = 'online'",
    )
    .fetch_one(&db.pool)
    .await?;
    let admin_rows: Vec<(String, String)> = sqlx::query_as(
        r#"SELECT display_name, role
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

    let whitelist_pending: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM whitelist_requests WHERE status = 'pending'")
            .fetch_one(&db.pool)
            .await?;
    let whitelist_approved: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM whitelist_requests WHERE status = 'approved'")
            .fetch_one(&db.pool)
            .await?;
    let whitelist_rejected: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM whitelist_requests WHERE status = 'rejected'")
            .fetch_one(&db.pool)
            .await?;
    let whitelist_revoked: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM whitelist_requests WHERE status = 'revoked'")
            .fetch_one(&db.pool)
            .await?;

    let server_performance = get_server_performance_stats(db).await?;

    Ok(DashboardMetrics {
        total_servers: total_servers.0,
        online_servers: online_servers.0,
        offline_servers,
        communities: communities.0,
        online_players: online_players.0,
        admins,
        admin_preview,
        whitelist_stats: WhitelistStats {
            pending: whitelist_pending.0,
            approved: whitelist_approved.0,
            rejected: whitelist_rejected.0,
            revoked: whitelist_revoked.0,
        },
        server_performance,
    })
}

async fn get_server_performance_stats(db: &Database) -> anyhow::Result<ServerPerformanceStats> {
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
              AND ssh.reported_at > now() - interval '5 minutes'
            ORDER BY ssh.server_id, ssh.reported_at DESC
        ) ssh
        "#,
    )
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

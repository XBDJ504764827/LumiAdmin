use crate::db::Database;
use crate::rcon::StatusResult;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ExternalServer {
    pub id: Uuid,
    pub name: String,
    pub ip: String,
    pub port: i32,
    #[serde(skip_serializing)]
    pub rcon_password: Option<String>,
    pub enabled: bool,
    pub poll_interval: i32,
    pub last_queried_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ExternalServerWithStatus {
    pub id: Uuid,
    pub name: String,
    pub ip: String,
    pub port: i32,
    #[serde(skip_serializing)]
    pub rcon_password: Option<String>,
    pub enabled: bool,
    pub poll_interval: i32,
    pub last_queried_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub server_name: Option<String>,
    pub current_map: Option<String>,
    pub player_count: Option<i32>,
    pub max_players: Option<i32>,
    pub players: Option<Vec<String>>,
    pub status_queried_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateExternalServerInput {
    pub name: String,
    pub ip: String,
    pub port: i32,
    pub rcon_password: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_poll_interval")]
    pub poll_interval: i32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateExternalServerInput {
    pub name: String,
    pub ip: String,
    pub port: i32,
    pub rcon_password: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_poll_interval")]
    pub poll_interval: i32,
}

fn default_true() -> bool { true }
fn default_poll_interval() -> i32 { 30 }

#[derive(Debug, Clone, Serialize)]
pub struct ExternalServerTestResult {
    pub ok: bool,
    pub message: String,
    pub status: Option<StatusResult>,
}

pub async fn list_servers(db: &Database) -> anyhow::Result<Vec<ExternalServerWithStatus>> {
    sqlx::query_as::<_, ExternalServerWithStatus>(
        r#"SELECT es.id, es.name, es.ip, es.port, es.rcon_password, es.enabled,
                  es.poll_interval, es.last_queried_at, es.created_at,
                  st.server_name, st.current_map, st.player_count, st.max_players,
                  st.players, st.queried_at AS status_queried_at
           FROM external_servers es
           LEFT JOIN external_server_status st ON st.server_id = es.id
           ORDER BY es.created_at ASC"#,
    )
    .fetch_all(&db.pool)
    .await
    .map_err(Into::into)
}

pub async fn create_server(db: &Database, input: CreateExternalServerInput) -> anyhow::Result<ExternalServer> {
    let name = input.name.trim();
    let ip = input.ip.trim();
    anyhow::ensure!(!name.is_empty(), "名称不能为空");
    anyhow::ensure!(!ip.is_empty(), "IP 不能为空");

    let poll_interval = input.poll_interval.clamp(5, 3600);

    sqlx::query_as::<_, ExternalServer>(
        r#"INSERT INTO external_servers (id, name, ip, port, rcon_password, enabled, poll_interval)
           VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, $6)
           RETURNING id, name, ip, port, rcon_password, enabled, poll_interval, last_queried_at, created_at"#,
    )
    .bind(name)
    .bind(ip)
    .bind(input.port)
    .bind(input.rcon_password.as_deref().map(|p| p.trim()).filter(|p| !p.is_empty()))
    .bind(input.enabled)
    .bind(poll_interval)
    .fetch_one(&db.pool)
    .await
    .map_err(Into::into)
}

pub async fn update_server(db: &Database, id: Uuid, input: UpdateExternalServerInput) -> anyhow::Result<ExternalServer> {
    let name = input.name.trim();
    let ip = input.ip.trim();
    anyhow::ensure!(!name.is_empty(), "名称不能为空");
    anyhow::ensure!(!ip.is_empty(), "IP 不能为空");

    let password = input.rcon_password.as_deref().map(|p| p.trim()).filter(|p| !p.is_empty());
    let poll_interval = input.poll_interval.clamp(5, 3600);
    sqlx::query_as::<_, ExternalServer>(
        r#"UPDATE external_servers SET name = $2, ip = $3, port = $4, rcon_password = $5, enabled = $6, poll_interval = $7
           WHERE id = $1
           RETURNING id, name, ip, port, rcon_password, enabled, poll_interval, last_queried_at, created_at"#,
    )
    .bind(id).bind(name).bind(ip).bind(input.port).bind(password).bind(input.enabled).bind(poll_interval)
    .fetch_one(&db.pool).await.map_err(Into::into)
}

pub async fn delete_server(db: &Database, id: Uuid) -> anyhow::Result<()> {
    let result = sqlx::query("DELETE FROM external_servers WHERE id = $1")
        .bind(id).execute(&db.pool).await?;
    anyhow::ensure!(result.rows_affected() > 0, "服务器不存在");
    Ok(())
}

pub async fn test_server(db: &Database, id: Uuid) -> anyhow::Result<ExternalServerTestResult> {
    let server: ExternalServer = sqlx::query_as(
        "SELECT id, name, ip, port, rcon_password, enabled, poll_interval, last_queried_at, created_at FROM external_servers WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&db.pool)
    .await?
    .ok_or_else(|| anyhow::anyhow!("服务器不存在"))?;

    let address = format!("{}:{}", server.ip, server.port);

    // 优先尝试 A2S 查询（无需密码）
    match crate::a2s::query_server(&address, 5) {
        Ok(info) => {
            let status = StatusResult {
                server_name: info.server_name,
                current_map: info.current_map,
                player_count: info.player_count,
                max_players: info.max_players,
                players: info.players.iter().map(|p| p.name.clone()).collect(),
            };
            return Ok(ExternalServerTestResult {
                ok: true,
                message: format!("A2S 查询成功: {} 玩家", status.player_count),
                status: Some(status),
            });
        }
        Err(e) => {
            tracing::debug!(server = %server.name, %e, "A2S query failed, trying RCON");
        }
    }

    // A2S 失败时回退到 RCON
    if let Some(ref pw) = server.rcon_password {
        match crate::rcon::RconConnection::connect(&address, pw, 5).await {
            Ok(mut conn) => {
                match conn.execute("status").await {
                    Ok(output) => {
                        let status = crate::rcon::parse_status_output(&output);
                        Ok(ExternalServerTestResult {
                            ok: true,
                            message: "RCON 连接测试成功".to_string(),
                            status: Some(status),
                        })
                    }
                    Err(error) => Ok(ExternalServerTestResult {
                        ok: false,
                        message: format!("执行 status 命令失败: {}", error),
                        status: None,
                    }),
                }
            }
            Err(error) => Ok(ExternalServerTestResult {
                ok: false,
                message: error,
                status: None,
            }),
        }
    } else {
        Ok(ExternalServerTestResult {
            ok: false,
            message: "A2S 查询失败且未配置 RCON 密码".to_string(),
            status: None,
        })
    }
}

pub async fn list_enabled_servers(db: &Database) -> anyhow::Result<Vec<ExternalServer>> {
    sqlx::query_as::<_, ExternalServer>(
        "SELECT id, name, ip, port, rcon_password, enabled, poll_interval, last_queried_at, created_at FROM external_servers WHERE enabled = true",
    )
    .fetch_all(&db.pool).await.map_err(Into::into)
}

pub async fn upsert_status(db: &Database, server_id: Uuid, status: &StatusResult) -> anyhow::Result<()> {
    sqlx::query(
        r#"INSERT INTO external_server_status (server_id, server_name, current_map, player_count, max_players, players, queried_at)
           VALUES ($1, $2, $3, $4, $5, $6, now())
           ON CONFLICT (server_id) DO UPDATE SET
             server_name = $2, current_map = $3, player_count = $4, max_players = $5,
             players = $6, queried_at = now()"#,
    )
    .bind(server_id).bind(&status.server_name).bind(&status.current_map)
    .bind(status.player_count).bind(status.max_players).bind(&status.players)
    .execute(&db.pool).await?;
    Ok(())
}

pub async fn get_status_by_ids(db: &Database, ids: &[Uuid]) -> anyhow::Result<Vec<ExternalServerWithStatus>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    sqlx::query_as::<_, ExternalServerWithStatus>(
        r#"SELECT es.id, es.name, es.ip, es.port, es.rcon_password, es.enabled,
                  es.poll_interval, es.last_queried_at, es.created_at,
                  st.server_name, st.current_map, st.player_count, st.max_players,
                  st.players, st.queried_at AS status_queried_at
           FROM external_servers es
           JOIN external_server_status st ON st.server_id = es.id
           WHERE es.id = ANY($1)
           ORDER BY es.name ASC"#,
    )
    .bind(ids).fetch_all(&db.pool).await.map_err(Into::into)
}

pub async fn get_all_with_status(db: &Database) -> anyhow::Result<Vec<ExternalServerWithStatus>> {
    sqlx::query_as::<_, ExternalServerWithStatus>(
        r#"SELECT es.id, es.name, es.ip, es.port, es.rcon_password, es.enabled,
                  es.poll_interval, es.last_queried_at, es.created_at,
                  st.server_name, st.current_map, st.player_count, st.max_players,
                  st.players, st.queried_at AS status_queried_at
           FROM external_servers es
           JOIN external_server_status st ON st.server_id = es.id
           WHERE es.enabled = true
           ORDER BY es.name ASC"#,
    )
    .fetch_all(&db.pool).await.map_err(Into::into)
}

pub async fn update_last_queried(db: &Database, server_id: Uuid) -> anyhow::Result<()> {
    sqlx::query("UPDATE external_servers SET last_queried_at = now() WHERE id = $1")
        .bind(server_id).execute(&db.pool).await?;
    Ok(())
}

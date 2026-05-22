use crate::db::Database;
use chrono::{DateTime, Utc};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct PlayerApiPlayer {
    pub player: String,
    pub steam_id64: String,
    pub ip_address: String,
    pub server_name: String,
    pub server_port: i32,
    pub current_map: String,
    pub reported_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlayerApiConfigResponse {
    pub max_api_count: i32,
    pub interval_seconds: i32,
    pub items: Vec<PlayerApiWebhookConfig>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct PlayerApiWebhookConfig {
    pub id: Uuid,
    pub public_path: String,
    pub webhook_url: Option<String>,
    pub secret: Option<String>,
    pub server_ids: Vec<Uuid>,
    #[serde(default)]
    pub external_server_ids: Vec<Uuid>,
    pub enabled: bool,
    pub public_access: bool,
    pub last_status: Option<String>,
    pub last_error: Option<String>,
    pub last_dispatched_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PlayerApiConfigInput {
    pub max_api_count: i32,
    pub interval_seconds: i32,
    pub items: Vec<PlayerApiWebhookInput>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PlayerApiWebhookInput {
    #[serde(default)]
    pub public_path: String,
    #[serde(default)]
    pub webhook_url: Option<String>,
    pub secret: Option<String>,
    pub server_ids: Vec<Uuid>,
    #[serde(default)]
    pub external_server_ids: Vec<Uuid>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_true")]
    pub public_access: bool,
}

#[derive(Debug, Clone)]
pub struct NormalizedWebhookInput {
    pub public_path: String,
    pub webhook_url: Option<String>,
    pub secret: Option<String>,
    pub server_ids: Vec<Uuid>,
    pub external_server_ids: Vec<Uuid>,
    pub enabled: bool,
    pub public_access: bool,
}

fn default_true() -> bool { true }

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PlayerApiDispatchRow {
    pub server_id: Uuid,
    pub server_name: String,
    pub server_ip: String,
    pub server_port: i32,
    pub current_map: String,
    pub max_players: i32,
    pub player: String,
    pub steam_id64: String,
    pub ping: i32,
    pub reported_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct WebhookPayload {
    pub generated_at: DateTime<Utc>,
    pub servers: Vec<WebhookServerPayload>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct WebhookServerPayload {
    pub server_id: Uuid,
    pub server_name: String,
    pub server_ip: String,
    pub server_port: i32,
    pub current_map: String,
    pub player_count: usize,
    pub max_players: i32,
    pub map_tier: Option<i32>,
    pub players: Vec<WebhookPlayerPayload>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct WebhookPlayerPayload {
    pub player: String,
    pub steam_id64: String,
    pub ping: i32,
    pub reported_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
struct PlayerApiConfigRow {
    max_api_count: i32,
    interval_seconds: i32,
}

pub async fn list_players(db: &Database) -> anyhow::Result<Vec<PlayerApiPlayer>> {
    sqlx::query_as::<_, PlayerApiPlayer>(
        r#"SELECT p.name AS player,
                  p.steam_id64,
                  p.ip AS ip_address,
                  s.name AS server_name,
                  p.server_port,
                  p.current_map,
                  p.reported_at
           FROM server_online_players p
           JOIN servers s ON s.id = p.server_id
           ORDER BY p.reported_at DESC, p.name ASC"#,
    )
    .fetch_all(&db.pool)
    .await
    .map_err(Into::into)
}

pub async fn get_config(db: &Database) -> anyhow::Result<PlayerApiConfigResponse> {
    let row = sqlx::query_as::<_, PlayerApiConfigRow>(
        r#"SELECT max_api_count, interval_seconds FROM player_api_config WHERE id = true"#,
    )
    .fetch_one(&db.pool)
    .await?;

    let items = sqlx::query_as::<_, PlayerApiWebhookConfig>(
        r#"SELECT id, public_path, webhook_url, secret, server_ids, external_server_ids,
                  enabled, public_access, last_status, last_error, last_dispatched_at
           FROM player_api_webhooks
           ORDER BY created_at ASC"#,
    )
    .fetch_all(&db.pool)
    .await?;

    Ok(PlayerApiConfigResponse {
        max_api_count: row.max_api_count,
        interval_seconds: row.interval_seconds,
        items,
    })
}

pub async fn save_config(db: &Database, input: PlayerApiConfigInput) -> anyhow::Result<PlayerApiConfigResponse> {
    anyhow::ensure!(input.max_api_count >= 0, "最大 API 数量不能为负数");
    anyhow::ensure!(input.interval_seconds > 0, "分发周期必须大于 0 秒");
    anyhow::ensure!(input.items.len() <= input.max_api_count as usize, "Webhook 数量超过限制");

    let normalized_items = input
        .items
        .into_iter()
        .map(normalize_webhook_input)
        .collect::<anyhow::Result<Vec<_>>>()?;

    let mut tx = db.pool.begin().await?;
    sqlx::query(
        r#"UPDATE player_api_config
           SET max_api_count = $1, interval_seconds = $2, updated_at = now()
           WHERE id = true"#,
    )
    .bind(input.max_api_count)
    .bind(input.interval_seconds)
    .execute(&mut *tx)
    .await?;

    // 获取现有配置，用于保留状态信息
    let existing_rows: Vec<(Uuid, String, Option<String>, Option<String>, Option<DateTime<Utc>>)> = sqlx::query_as(
        r#"SELECT id, public_path, last_status, last_error, last_dispatched_at
           FROM player_api_webhooks"#,
    )
    .fetch_all(&mut *tx)
    .await?;

    // 建立 public_path -> (id, last_status, last_error, last_dispatched_at) 的映射
    let existing_map: std::collections::HashMap<String, (Uuid, Option<String>, Option<String>, Option<DateTime<Utc>>)> = existing_rows
        .into_iter()
        .map(|(id, public_path, last_status, last_error, last_dispatched_at)| {
            (public_path, (id, last_status, last_error, last_dispatched_at))
        })
        .collect();

    // 收集需要保留的 ID
    let new_public_paths: Vec<&str> = normalized_items.iter().map(|item| item.public_path.as_str()).collect();

    // 删除不再需要的 webhook
    sqlx::query(r#"DELETE FROM player_api_webhooks WHERE NOT (public_path = ANY($1))"#)
        .bind(&new_public_paths)
        .execute(&mut *tx)
        .await?;

    // 插入或更新 webhook
    for item in normalized_items {
        if let Some((existing_id, _last_status, _last_error, _last_dispatched_at)) = existing_map.get(&item.public_path) {
            // 更新现有记录，保留状态信息
            sqlx::query(
                r#"UPDATE player_api_webhooks
                   SET webhook_url = $2, secret = $3, server_ids = $4, external_server_ids = $5,
                       enabled = $6, public_access = $7, updated_at = now()
                   WHERE id = $1"#,
            )
            .bind(existing_id)
            .bind(&item.webhook_url)
            .bind(&item.secret)
            .bind(&item.server_ids)
            .bind(&item.external_server_ids)
            .bind(item.enabled)
            .bind(item.public_access)
            .execute(&mut *tx)
            .await?;
        } else {
            // 插入新记录
            sqlx::query(
                r#"INSERT INTO player_api_webhooks (id, public_path, webhook_url, secret, server_ids, external_server_ids, enabled, public_access)
                   VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"#,
            )
            .bind(Uuid::new_v4())
            .bind(item.public_path)
            .bind(&item.webhook_url)
            .bind(item.secret)
            .bind(item.server_ids)
            .bind(item.external_server_ids)
            .bind(item.enabled)
            .bind(item.public_access)
            .execute(&mut *tx)
            .await?;
        }
    }

    tx.commit().await?;
    get_config(db).await
}

pub fn normalize_webhook_input(input: PlayerApiWebhookInput) -> anyhow::Result<NormalizedWebhookInput> {
    let public_path = input.public_path.trim().to_string();
    anyhow::ensure!(!public_path.is_empty(), "自定义后缀不能为空");
    let webhook_url = input.webhook_url.and_then(|value| {
        let trimmed = value.trim().to_string();
        if trimmed.is_empty() { None } else { Some(trimmed) }
    });
    let secret = input.secret.and_then(|value| {
        let trimmed = value.trim().to_string();
        if trimmed.is_empty() { None } else { Some(trimmed) }
    });

    Ok(NormalizedWebhookInput {
        public_path,
        webhook_url,
        secret,
        server_ids: input.server_ids,
        external_server_ids: input.external_server_ids,
        enabled: input.enabled,
        public_access: input.public_access,
    })
}

pub fn build_webhook_payload(generated_at: DateTime<Utc>, rows: Vec<PlayerApiDispatchRow>) -> WebhookPayload {
    let mut servers: Vec<WebhookServerPayload> = Vec::new();

    for row in rows {
        let player = WebhookPlayerPayload {
            player: row.player,
            steam_id64: row.steam_id64,
            ping: row.ping,
            reported_at: row.reported_at,
        };

        if let Some(server) = servers.iter_mut().find(|server| server.server_id == row.server_id) {
            server.players.push(player);
            server.player_count = server.players.len();
        } else {
            servers.push(WebhookServerPayload {
                server_id: row.server_id,
                server_name: row.server_name,
                server_ip: row.server_ip.clone(),
                server_port: row.server_port,
                current_map: row.current_map.clone(),
                player_count: 1,
                max_players: row.max_players,
                map_tier: None,
                players: vec![player],
            });
        }
    }

    WebhookPayload { generated_at, servers }
}

/// 按 id 列表的顺序排列 servers。不在列表中的排到末尾并按名称字母排序。
pub fn sort_servers_by_ids(servers: &mut [WebhookServerPayload], order: &[Uuid]) {
    let position: std::collections::HashMap<Uuid, usize> = order
        .iter()
        .enumerate()
        .map(|(i, id)| (*id, i))
        .collect();
    servers.sort_by(|a, b| {
        let pa = position.get(&a.server_id).unwrap_or(&usize::MAX);
        let pb = position.get(&b.server_id).unwrap_or(&usize::MAX);
        pa.cmp(pb).then_with(|| a.server_name.cmp(&b.server_name))
    });
}

/// 批量查询 map_tier 并填充到每个 server 中
async fn fill_map_tiers(db: &Database, servers: &mut [WebhookServerPayload]) {
    let map_names: Vec<String> = servers.iter()
        .map(|s| s.current_map.clone())
        .filter(|m| !m.is_empty())
        .collect();
    let tiers = match crate::services::map_tier_service::get_map_tiers(db, &map_names).await {
        Ok(t) => t,
        Err(_) => return,
    };
    for server in servers.iter_mut() {
        if !server.current_map.is_empty() {
            server.map_tier = tiers.get(&server.current_map).copied();
        }
    }
}

async fn fetch_external_server_rows(db: &Database, ids: &[Uuid]) -> anyhow::Result<Vec<WebhookServerPayload>> {
    let servers = if ids.is_empty() {
        crate::services::external_server_service::get_all_with_status(db).await?
    } else {
        crate::services::external_server_service::get_status_by_ids(db, ids).await?
    };

    Ok(servers.into_iter().map(|s| {
        let server_name = s.server_name
            .as_deref()
            .filter(|n| !n.is_empty())
            .unwrap_or(&s.name)
            .to_string();
        WebhookServerPayload {
            server_id: s.id,
            server_name,
            server_ip: s.ip,
            server_port: s.port,
            current_map: s.current_map.clone().unwrap_or_default(),
            player_count: s.player_count.unwrap_or(0) as usize,
            max_players: s.max_players.unwrap_or(0),
            map_tier: None,
            players: s.players.unwrap_or_default().into_iter().map(|name| WebhookPlayerPayload {
                player: name,
                steam_id64: String::new(),
                ping: 0,
                reported_at: s.status_queried_at.unwrap_or_default(),
            }).collect(),
        }
    }).collect())
}

async fn dispatch_rows_for_item(db: &Database, item: &PlayerApiWebhookConfig) -> anyhow::Result<Vec<PlayerApiDispatchRow>> {
    if item.server_ids.is_empty() {
        return sqlx::query_as::<_, PlayerApiDispatchRow>(
            r#"SELECT s.id AS server_id,
                      s.name AS server_name,
                      s.ip AS server_ip,
                      p.server_port,
                      p.current_map,
                      s.max_players,
                      p.name AS player,
                      p.steam_id64,
                      p.ping,
                      p.reported_at
               FROM server_online_players p
               JOIN servers s ON s.id = p.server_id
               ORDER BY s.name ASC, p.name ASC"#,
        )
        .fetch_all(&db.pool)
        .await
        .map_err(Into::into);
    }

    sqlx::query_as::<_, PlayerApiDispatchRow>(
        r#"SELECT s.id AS server_id,
                  s.name AS server_name,
                  s.ip AS server_ip,
                  p.server_port,
                  p.current_map,
                  s.max_players,
                  p.name AS player,
                  p.steam_id64,
                  p.ping,
                  p.reported_at
           FROM server_online_players p
           JOIN servers s ON s.id = p.server_id
           WHERE s.id = ANY($1)
           ORDER BY s.name ASC, p.name ASC"#,
    )
    .bind(&item.server_ids)
    .fetch_all(&db.pool)
    .await
    .map_err(Into::into)
}

async fn record_dispatch_result(db: &Database, id: Uuid, status: &str, error: Option<String>) -> anyhow::Result<()> {
    sqlx::query(
        r#"UPDATE player_api_webhooks
           SET last_status = $2,
               last_error = $3,
               last_dispatched_at = now(),
               updated_at = now()
           WHERE id = $1"#,
    )
    .bind(id)
    .bind(status)
    .bind(error.as_deref())
    .execute(&db.pool)
    .await?;
    Ok(())
}

pub async fn dispatch_once(db: &Database, client: &Client) -> anyhow::Result<()> {
    let config = get_config(db).await?;
    if config.max_api_count == 0 || config.items.is_empty() {
        return Ok(());
    }

    for item in config.items {
        if !item.enabled {
            continue;
        }
        let webhook_url = match item.webhook_url.as_deref() {
            Some(url) if !url.trim().is_empty() => url,
            _ => continue,
        };
        let rows = dispatch_rows_for_item(db, &item).await?;
        let mut payload = build_webhook_payload(Utc::now(), rows);
        let mut external_servers = fetch_external_server_rows(db, &item.external_server_ids).await?;
        if !item.server_ids.is_empty() {
            sort_servers_by_ids(&mut payload.servers, &item.server_ids);
        } else {
            payload.servers.sort_by(|a, b| a.server_name.cmp(&b.server_name));
        }
        if !item.external_server_ids.is_empty() {
            sort_servers_by_ids(&mut external_servers, &item.external_server_ids);
        } else {
            external_servers.sort_by(|a, b| a.server_name.cmp(&b.server_name));
        }
        payload.servers.extend(external_servers);
        fill_map_tiers(db, &mut payload.servers).await;
        let mut request = client.post(webhook_url).json(&payload);
        if let Some(secret) = item.secret.as_deref() {
            request = request.header("X-Manger-Secret", secret);
        }

        match request.send().await {
            Ok(response) if response.status().is_success() => {
                record_dispatch_result(db, item.id, "success", None).await?;
            }
            Ok(response) => {
                record_dispatch_result(db, item.id, "failed", Some(format!("HTTP {}", response.status()))).await?;
            }
            Err(error) => {
                record_dispatch_result(db, item.id, "failed", Some(error.to_string())).await?;
            }
        }
    }

    Ok(())
}

pub async fn fetch_webhook_payload_by_path(db: &Database, public_path: &str, secret_header: Option<&str>) -> anyhow::Result<WebhookPayload> {
    let item = sqlx::query_as::<_, PlayerApiWebhookConfig>(
        r#"SELECT id, public_path, webhook_url, secret, server_ids, external_server_ids,
                  enabled, public_access, last_status, last_error, last_dispatched_at
           FROM player_api_webhooks
           WHERE public_path = $1"#,
    )
    .bind(public_path)
    .fetch_optional(&db.pool)
    .await?
    .ok_or_else(|| anyhow::anyhow!("not found"))?;

    if !item.enabled {
        anyhow::bail!("disabled");
    }

    // 如果不是公开访问，必须提供正确的 secret（常量时间比较，防止时序攻击）
    if !item.public_access {
        let expected = item.secret.as_deref().unwrap_or("");
        let provided = secret_header.unwrap_or("");
        if expected.is_empty() || !constant_time_eq(expected.as_bytes(), provided.as_bytes()) {
            anyhow::bail!("unauthorized");
        }
    }

    let rows = dispatch_rows_for_item(db, &item).await?;
    let mut payload = build_webhook_payload(Utc::now(), rows);

    let mut external_servers = fetch_external_server_rows(db, &item.external_server_ids).await?;
    if !item.server_ids.is_empty() {
        sort_servers_by_ids(&mut payload.servers, &item.server_ids);
    } else {
        payload.servers.sort_by(|a, b| a.server_name.cmp(&b.server_name));
    }
    if !item.external_server_ids.is_empty() {
        sort_servers_by_ids(&mut external_servers, &item.external_server_ids);
    } else {
        external_servers.sort_by(|a, b| a.server_name.cmp(&b.server_name));
    }
    payload.servers.extend(external_servers);
    fill_map_tiers(db, &mut payload.servers).await;

    Ok(payload)
}

pub async fn dispatch_interval_seconds(db: &Database) -> anyhow::Result<u64> {
    let row = sqlx::query_as::<_, (i32,)>(
        r#"SELECT interval_seconds FROM player_api_config WHERE id = true"#,
    )
    .fetch_one(&db.pool)
    .await?;
    Ok(row.0.max(1) as u64)
}

pub fn start_dispatch_loop(db: Database) {
    let _handle = tokio::spawn(async move {
        let client = Client::new();
        loop {
            let seconds = dispatch_interval_seconds(&db).await.unwrap_or(30);
            tokio::time::sleep(std::time::Duration::from_secs(seconds)).await;
            if let Err(error) = dispatch_once(&db, &client).await {
                tracing::warn!(%error, "player api webhook dispatch failed");
            }
        }
    });
}

/// 常量时间比较，防止通过响应时间差异推断 secret 内容
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut result = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        result |= x ^ y;
    }
    result == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    #[test]
    fn build_webhook_payload_groups_players_by_server() {
        let server_id = Uuid::new_v4();
        let server_id2 = Uuid::new_v4();
        let generated_at = Utc.with_ymd_and_hms(2026, 4, 27, 13, 0, 0).unwrap();
        let reported_at = Utc.with_ymd_and_hms(2026, 4, 27, 12, 59, 0).unwrap();

        let payload = build_webhook_payload(
            generated_at,
            vec![
                PlayerApiDispatchRow {
                    server_id,
                    server_name: "一号服".to_string(),
                    server_ip: "203.0.113.100".to_string(),
                    server_port: 27015,
                    current_map: "de_dust2".to_string(),
                    max_players: 32,
                    player: "Alice".to_string(),
                    steam_id64: "76561198000000001".to_string(),
                    ping: 28,
                    reported_at,
                },
                PlayerApiDispatchRow {
                    server_id,
                    server_name: "一号服".to_string(),
                    server_ip: "203.0.113.100".to_string(),
                    server_port: 27015,
                    current_map: "de_dust2".to_string(),
                    max_players: 32,
                    player: "Bob".to_string(),
                    steam_id64: "76561198000000002".to_string(),
                    ping: 35,
                    reported_at,
                },
                PlayerApiDispatchRow {
                    server_id: server_id2,
                    server_name: "二号服".to_string(),
                    server_ip: "203.0.113.200".to_string(),
                    server_port: 27016,
                    current_map: "de_inferno".to_string(),
                    max_players: 16,
                    player: "Charlie".to_string(),
                    steam_id64: "76561198000000003".to_string(),
                    ping: 42,
                    reported_at,
                },
            ],
        );

        assert_eq!(payload.generated_at, generated_at);
        assert_eq!(payload.servers.len(), 2);

        let server1 = &payload.servers[0];
        assert_eq!(server1.server_id, server_id);
        assert_eq!(server1.server_name, "一号服");
        assert_eq!(server1.server_ip, "203.0.113.100");
        assert_eq!(server1.server_port, 27015);
        assert_eq!(server1.current_map, "de_dust2");
        assert_eq!(server1.player_count, 2);
        assert_eq!(server1.players.len(), 2);
        assert_eq!(server1.players[0].player, "Alice");
        assert_eq!(server1.players[1].steam_id64, "76561198000000002");

        let server2 = &payload.servers[1];
        assert_eq!(server2.server_id, server_id2);
        assert_eq!(server2.server_name, "二号服");
        assert_eq!(server2.server_ip, "203.0.113.200");
        assert_eq!(server2.current_map, "de_inferno");
        assert_eq!(server2.player_count, 1);
        assert_eq!(server2.players.len(), 1);
        assert_eq!(server2.players[0].player, "Charlie");
    }

    #[test]
    fn normalize_webhook_input_trims_secret_and_server_ids() {
        let server_id = Uuid::new_v4();
        let normalized = normalize_webhook_input(PlayerApiWebhookInput {
            public_path: "my-hook".to_string(),
            webhook_url: Some(" https://api.example.com/players ".to_string()),
            secret: Some("  token  ".to_string()),
            server_ids: vec![server_id],
            external_server_ids: vec![],
            enabled: true,
            public_access: true,
        })
        .unwrap();

        assert_eq!(normalized.public_path, "my-hook");
        assert_eq!(normalized.webhook_url.as_deref(), Some("https://api.example.com/players"));
        assert_eq!(normalized.secret.as_deref(), Some("token"));
        assert_eq!(normalized.server_ids, vec![server_id]);
    }
}

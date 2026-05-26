use std::collections::BTreeMap;

use crate::db::Database;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const OFFLINE_AFTER_SECONDS: i64 = 60;

#[derive(Serialize)]
pub struct ServerItem {
    pub id: Uuid,
    pub name: String,
    pub ip: String,
    pub port: i32,
    pub report_token: Option<String>,
    pub note: Option<String>,
    pub status: String,
    pub players: Vec<String>,
    pub online_player_count: usize,
    pub max_players: i32,
    pub last_tested_at: Option<String>,
    pub last_reported_at: Option<String>,
    pub access_restriction_enabled: bool,
    pub min_rating: i32,
    pub min_steam_level: i32,
    pub whitelist_mode_enabled: bool,
    pub use_custom_access: bool,
}

#[derive(Serialize)]
pub struct CommunityGroup {
    pub id: Uuid,
    pub name: String,
    pub whitelist_mode_enabled: bool,
    pub min_rating: i32,
    pub min_steam_level: i32,
    pub servers: Vec<ServerItem>,
}

#[derive(Deserialize)]
pub struct CreateCommunityInput {
    pub name: String,
}

#[derive(Deserialize)]
pub struct ServerInput {
    pub name: String,
    pub ip: String,
    pub port: u16,
    #[serde(default)]
    pub rcon_password: String,
    pub report_token: Option<String>,
    pub note: Option<String>,
    #[serde(default)]
    pub access_restriction_enabled: bool,
    #[serde(default)]
    pub min_rating: i32,
    #[serde(default)]
    pub min_steam_level: i32,
    #[serde(default)]
    pub whitelist_mode_enabled: bool,
    #[serde(default)]
    pub max_players: i32,
    #[serde(default)]
    pub use_custom_access: bool,
}

#[derive(Serialize)]
pub struct ServerReportToken {
    pub report_token: String,
}

#[derive(Deserialize)]
pub struct UpdateCommunityAccessInput {
    pub whitelist_mode_enabled: bool,
    pub min_rating: i32,
    pub min_steam_level: i32,
}

#[derive(Deserialize)]
pub struct OnlinePlayersReportInput {
    pub report_token: String,
    pub port: u16,
    #[serde(default)]
    pub current_map: String,
    pub players: Vec<OnlinePlayerInput>,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct OnlinePlayerInput {
    pub name: String,
    pub steam_id64: Option<String>,
    pub steam_id: Option<String>,
    pub ip: Option<String>,
    pub ping: i32,
    pub server_port: Option<u16>,
}

struct NormalizedOnlinePlayer {
    name: String,
    steam_id64: String,
    ip: String,
    ping: i32,
    server_port: u16,
}

#[derive(Serialize)]
pub struct OnlinePlayersReportResult {
    pub server_id: Uuid,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct OnlinePlayerItem {
    pub name: String,
    pub steam_id64: String,
    pub ip: String,
    pub ping: i32,
    pub server_port: i32,
}

#[derive(Serialize)]
pub struct OnlinePlayersResponse {
    pub players: Vec<String>,
    pub details: Vec<OnlinePlayerItem>,
}

#[derive(Serialize)]
pub struct RconTestResult {
    pub ok: bool,
    pub message: String,
    pub players: Vec<String>,
}

#[derive(sqlx::FromRow)]
struct CommunityRow {
    community_id: Uuid,
    community_name: String,
    community_whitelist_mode_enabled: Option<bool>,
    community_min_rating: Option<i32>,
    community_min_steam_level: Option<i32>,
    server_id: Option<Uuid>,
    server_name: Option<String>,
    ip: Option<String>,
    port: Option<i32>,
    report_token: Option<String>,
    note: Option<String>,
    status: Option<String>,
    online_players: Option<Vec<String>>,
    max_players: Option<i32>,
    last_tested_at: Option<chrono::DateTime<chrono::Utc>>,
    last_reported_at: Option<chrono::DateTime<chrono::Utc>>,
    access_restriction_enabled: Option<bool>,
    min_rating: Option<i32>,
    min_steam_level: Option<i32>,
    whitelist_mode_enabled: Option<bool>,
    use_custom_access: Option<bool>,
}

#[derive(sqlx::FromRow)]
struct ServerDetailRow {
    id: Uuid,
    name: String,
    ip: String,
    port: i32,
    report_token: Option<String>,
    note: Option<String>,
    status: String,
    players: Option<Vec<String>>,
    max_players: i32,
    last_tested_at: Option<chrono::DateTime<chrono::Utc>>,
    last_reported_at: Option<chrono::DateTime<chrono::Utc>>,
    access_restriction_enabled: bool,
    min_rating: i32,
    min_steam_level: i32,
    whitelist_mode_enabled: bool,
    use_custom_access: bool,
}

pub async fn list_groups(db: &Database) -> anyhow::Result<Vec<CommunityGroup>> {
    let rows = sqlx::query_as::<_, CommunityRow>(
        r#"
        SELECT
            c.id AS community_id,
            c.name AS community_name,
            c.whitelist_mode_enabled AS community_whitelist_mode_enabled,
            c.min_rating AS community_min_rating,
            c.min_steam_level AS community_min_steam_level,
            s.id AS server_id,
            s.name AS server_name,
            s.ip,
            s.port,
            s.report_token,
            s.note,
            s.status,
            COALESCE(p.players, ARRAY[]::TEXT[]) AS online_players,
            s.max_players,
            s.last_tested_at,
            s.last_reported_at,
            s.access_restriction_enabled,
            s.min_rating,
            s.min_steam_level,
            s.whitelist_mode_enabled,
            s.use_custom_access
        FROM communities c
        LEFT JOIN servers s ON s.community_id = c.id
        LEFT JOIN LATERAL (
            SELECT ARRAY_AGG(name ORDER BY name) AS players, COUNT(*) AS player_count
            FROM server_online_players
            WHERE server_id = s.id
        ) p ON true
        ORDER BY c.created_at DESC, s.created_at ASC
        "#,
    )
    .fetch_all(&db.pool)
    .await?;

    let mut groups: BTreeMap<Uuid, CommunityGroup> = BTreeMap::new();

    for row in rows {
        let group = groups
            .entry(row.community_id)
            .or_insert_with(|| CommunityGroup {
                id: row.community_id,
                name: row.community_name,
                whitelist_mode_enabled: row.community_whitelist_mode_enabled.unwrap_or(false),
                min_rating: row.community_min_rating.unwrap_or(0),
                min_steam_level: row.community_min_steam_level.unwrap_or(0),
                servers: Vec::new(),
            });

        if let (Some(id), Some(name), Some(ip), Some(port), Some(status)) =
            (row.server_id, row.server_name, row.ip, row.port, row.status)
        {
            let stale = is_report_stale(row.last_reported_at);
            let players = if stale {
                Vec::new()
            } else {
                row.online_players.unwrap_or_default()
            };
            let online_player_count = players.len();
            group.servers.push(ServerItem {
                id,
                name,
                ip,
                port,
                report_token: row.report_token,
                note: row.note,
                status: if stale { "offline".to_string() } else { status },
                players,
                online_player_count,
                max_players: row.max_players.unwrap_or(0),
                last_tested_at: row.last_tested_at.map(|value| value.to_rfc3339()),
                last_reported_at: row.last_reported_at.map(|value| value.to_rfc3339()),
                access_restriction_enabled: row.access_restriction_enabled.unwrap_or(false),
                min_rating: row.min_rating.unwrap_or(0),
                min_steam_level: row.min_steam_level.unwrap_or(0),
                whitelist_mode_enabled: row.whitelist_mode_enabled.unwrap_or(false),
                use_custom_access: row.use_custom_access.unwrap_or(false),
            });
        }
    }

    Ok(groups.into_values().collect())
}

pub async fn create_group(
    db: &Database,
    input: CreateCommunityInput,
) -> anyhow::Result<CommunityGroup> {
    let name = input.name.trim();
    anyhow::ensure!(!name.is_empty(), "社区名称不能为空");

    let id = Uuid::new_v4();
    sqlx::query(r#"INSERT INTO communities (id, name) VALUES ($1, $2)"#)
        .bind(id)
        .bind(name)
        .execute(&db.pool)
        .await?;

    Ok(CommunityGroup {
        id,
        name: name.to_string(),
        whitelist_mode_enabled: false,
        min_rating: 0,
        min_steam_level: 0,
        servers: Vec::new(),
    })
}

pub async fn create_server(
    db: &Database,
    community_id: Uuid,
    input: ServerInput,
) -> anyhow::Result<ServerItem> {
    let tested = test_rcon_connection(&input).await?;
    anyhow::ensure!(tested.ok, "RCON 测试未通过，无法保存服务器");

    let id = Uuid::new_v4();
    let name = input.name.trim();
    let ip = input.ip.trim();
    let password = input.rcon_password.trim();
    let report_token = super::normalize_optional_text(input.report_token.as_deref())
        .unwrap_or_else(generate_report_token);
    let note = super::normalize_optional_text(input.note.as_deref());

    anyhow::ensure!(!name.is_empty(), "服务器名称不能为空");
    anyhow::ensure!(!ip.is_empty(), "服务器 IP 不能为空");
    anyhow::ensure!(!password.is_empty(), "RCON 密码不能为空");
    anyhow::ensure!(input.min_rating >= 0, "最低进入 rating 不能为负数");
    anyhow::ensure!(input.min_steam_level >= 0, "最低 Steam 等级不能为负数");
    anyhow::ensure!(input.max_players >= 0, "最大玩家数不能为负数");

    sqlx::query(
        r#"
        INSERT INTO servers (
            id, community_id, name, ip, port, rcon_password, report_token, note, status, players, last_tested_at,
            access_restriction_enabled, min_rating, min_steam_level, whitelist_mode_enabled, max_players, use_custom_access
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'online', $9, now(), $10, $11, $12, $13, $14, $15)
        "#,
    )
    .bind(id)
    .bind(community_id)
    .bind(name)
    .bind(ip)
    .bind(i32::from(input.port))
    .bind(password)
    .bind(report_token.clone())
    .bind(note.clone())
    .bind(tested.players.clone())
    .bind(input.access_restriction_enabled)
    .bind(input.min_rating)
    .bind(input.min_steam_level)
    .bind(input.whitelist_mode_enabled)
    .bind(input.max_players)
    .bind(input.use_custom_access)
    .execute(&db.pool)
    .await?;

    let online_player_count = tested.players.len();
    Ok(ServerItem {
        id,
        name: name.to_string(),
        ip: ip.to_string(),
        port: i32::from(input.port),
        report_token: Some(report_token),
        note,
        status: "online".to_string(),
        players: tested.players,
        online_player_count,
        max_players: input.max_players,
        last_tested_at: Some(chrono::Utc::now().to_rfc3339()),
        last_reported_at: None,
        access_restriction_enabled: input.access_restriction_enabled,
        min_rating: input.min_rating,
        min_steam_level: input.min_steam_level,
        whitelist_mode_enabled: input.whitelist_mode_enabled,
        use_custom_access: input.use_custom_access,
    })
}

pub async fn update_server(
    db: &Database,
    server_id: Uuid,
    input: ServerInput,
) -> anyhow::Result<ServerItem> {
    let name = input.name.trim();
    let ip = input.ip.trim();
    let password = input.rcon_password.trim();
    let report_token = super::normalize_optional_text(input.report_token.as_deref());
    let note = super::normalize_optional_text(input.note.as_deref());

    anyhow::ensure!(!name.is_empty(), "服务器名称不能为空");
    anyhow::ensure!(!ip.is_empty(), "服务器 IP 不能为空");
    anyhow::ensure!(input.min_rating >= 0, "最低进入 rating 不能为负数");
    anyhow::ensure!(input.min_steam_level >= 0, "最低 Steam 等级不能为负数");
    anyhow::ensure!(input.max_players >= 0, "最大玩家数不能为负数");

    let changing_password = !password.is_empty();
    let players_for_status = if changing_password {
        let tested = test_rcon_connection(&input).await?;
        anyhow::ensure!(tested.ok, "RCON 测试未通过，无法保存服务器");
        tested.players
    } else {
        vec![]
    };

    let row = if changing_password {
        sqlx::query_as::<_, ServerDetailRow>(
            r#"
            UPDATE servers
            SET name = $2, ip = $3, port = $4, rcon_password = $5,
                report_token = COALESCE($6, report_token), note = $7,
                status = 'online', players = $8, last_tested_at = now(),
                access_restriction_enabled = $9, min_rating = $10, min_steam_level = $11, whitelist_mode_enabled = $12, max_players = $13,
                use_custom_access = $14
            WHERE id = $1
            RETURNING id, name, ip, port, report_token, note, status, players, max_players, last_tested_at, last_reported_at,
                      access_restriction_enabled, min_rating, min_steam_level, whitelist_mode_enabled, use_custom_access
            "#,
        )
        .bind(server_id)
        .bind(name)
        .bind(ip)
        .bind(i32::from(input.port))
        .bind(password)
        .bind(report_token.clone())
        .bind(note.clone())
        .bind(players_for_status)
        .bind(input.access_restriction_enabled)
        .bind(input.min_rating)
        .bind(input.min_steam_level)
        .bind(input.whitelist_mode_enabled)
        .bind(input.max_players)
        .bind(input.use_custom_access)
        .fetch_one(&db.pool)
        .await?
    } else {
        sqlx::query_as::<_, ServerDetailRow>(
            r#"
            UPDATE servers
            SET name = $2, ip = $3, port = $4,
                report_token = COALESCE($5, report_token), note = $6,
                access_restriction_enabled = $7, min_rating = $8, min_steam_level = $9, whitelist_mode_enabled = $10, max_players = $11,
                use_custom_access = $12
            WHERE id = $1
            RETURNING id, name, ip, port, report_token, note, status, players, max_players, last_tested_at, last_reported_at,
                      access_restriction_enabled, min_rating, min_steam_level, whitelist_mode_enabled, use_custom_access
            "#,
        )
        .bind(server_id)
        .bind(name)
        .bind(ip)
        .bind(i32::from(input.port))
        .bind(report_token.clone())
        .bind(note.clone())
        .bind(input.access_restriction_enabled)
        .bind(input.min_rating)
        .bind(input.min_steam_level)
        .bind(input.whitelist_mode_enabled)
        .bind(input.max_players)
        .bind(input.use_custom_access)
        .fetch_one(&db.pool)
        .await?
    };

    let players = row.players.unwrap_or_default();
    let online_player_count = players.len();
    Ok(ServerItem {
        id: row.id,
        name: row.name,
        ip: row.ip,
        port: row.port,
        report_token: row.report_token,
        note: row.note,
        status: row.status,
        players,
        online_player_count,
        max_players: row.max_players,
        last_tested_at: row.last_tested_at.map(|value| value.to_rfc3339()),
        last_reported_at: row.last_reported_at.map(|value| value.to_rfc3339()),
        access_restriction_enabled: row.access_restriction_enabled,
        min_rating: row.min_rating,
        min_steam_level: row.min_steam_level,
        whitelist_mode_enabled: row.whitelist_mode_enabled,
        use_custom_access: row.use_custom_access,
    })
}

pub async fn delete_group(db: &Database, group_id: Uuid) -> anyhow::Result<()> {
    let result = sqlx::query(r#"DELETE FROM communities WHERE id = $1"#)
        .bind(group_id)
        .execute(&db.pool)
        .await?;

    anyhow::ensure!(result.rows_affected() == 1, "社区组不存在");
    Ok(())
}

pub async fn delete_server(db: &Database, server_id: Uuid) -> anyhow::Result<()> {
    sqlx::query(r#"DELETE FROM servers WHERE id = $1"#)
        .bind(server_id)
        .execute(&db.pool)
        .await?;
    Ok(())
}

pub async fn update_community_access(
    db: &Database,
    community_id: Uuid,
    input: UpdateCommunityAccessInput,
) -> anyhow::Result<CommunityGroup> {
    anyhow::ensure!(input.min_rating >= 0, "最低进入 rating 不能为负数");
    anyhow::ensure!(input.min_steam_level >= 0, "最低 Steam 等级不能为负数");

    sqlx::query(
        r#"UPDATE communities SET whitelist_mode_enabled = $2, min_rating = $3, min_steam_level = $4 WHERE id = $1"#,
    )
    .bind(community_id)
    .bind(input.whitelist_mode_enabled)
    .bind(input.min_rating)
    .bind(input.min_steam_level)
    .execute(&db.pool)
    .await?;

    let groups = list_groups(db).await?;
    groups
        .into_iter()
        .find(|g| g.id == community_id)
        .ok_or_else(|| anyhow::anyhow!("社区不存在"))
}

pub async fn get_report_token(db: &Database, server_id: Uuid) -> anyhow::Result<ServerReportToken> {
    let token: (String,) = sqlx::query_as(r#"SELECT report_token FROM servers WHERE id = $1"#)
        .bind(server_id)
        .fetch_one(&db.pool)
        .await?;

    Ok(ServerReportToken {
        report_token: token.0,
    })
}

pub async fn reset_report_token(
    db: &Database,
    server_id: Uuid,
) -> anyhow::Result<ServerReportToken> {
    let report_token = generate_report_token();
    let row: (String,) = sqlx::query_as(
        r#"
        UPDATE servers
        SET report_token = $2
        WHERE id = $1
        RETURNING report_token
        "#,
    )
    .bind(server_id)
    .bind(report_token)
    .fetch_one(&db.pool)
    .await?;

    Ok(ServerReportToken {
        report_token: row.0,
    })
}

pub async fn report_online_players(
    db: &Database,
    input: OnlinePlayersReportInput,
) -> anyhow::Result<OnlinePlayersReportResult> {
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
    .bind(i32::from(input.port))
    .fetch_optional(&db.pool)
    .await?
    .map(|row| row.0)
    .ok_or_else(|| anyhow::anyhow!("服务器 token 或端口不匹配"))?;

    let mut normalized_players = Vec::with_capacity(input.players.len());
    for player in input.players {
        normalized_players.push(normalize_online_player(player, input.port)?);
    }

    let mut tx = db.pool.begin().await?;
    let player_names = normalized_players
        .iter()
        .map(|player| player.name.clone())
        .collect::<Vec<_>>();

    sqlx::query(
        r#"
        UPDATE servers
        SET status = 'online', players = $2, last_reported_at = now()
        WHERE id = $1
        "#,
    )
    .bind(server_id)
    .bind(&player_names)
    .execute(&mut *tx)
    .await?;

    sqlx::query(r#"DELETE FROM server_online_players WHERE server_id = $1"#)
        .bind(server_id)
        .execute(&mut *tx)
        .await?;

    if !normalized_players.is_empty() {
        let names: Vec<&str> = normalized_players.iter().map(|p| p.name.as_str()).collect();
        let steam_ids: Vec<&str> = normalized_players
            .iter()
            .map(|p| p.steam_id64.as_str())
            .collect();
        let ips: Vec<&str> = normalized_players.iter().map(|p| p.ip.as_str()).collect();
        let pings: Vec<i32> = normalized_players.iter().map(|p| p.ping).collect();
        let ports: Vec<i32> = normalized_players
            .iter()
            .map(|p| i32::from(p.server_port))
            .collect();
        let current_map = &input.current_map;

        sqlx::query(
            r#"INSERT INTO server_online_players (server_id, name, steam_id64, ip, ping, server_port, current_map)
               SELECT $1, u.name, u.steam_id64, u.ip, u.ping, u.server_port, $3
               FROM UNNEST($2::TEXT[], $4::TEXT[], $5::TEXT[], $6::INTEGER[], $7::INTEGER[]) AS u(name, steam_id64, ip, ping, server_port)"#,
        )
        .bind(server_id)
        .bind(&names)
        .bind(current_map)
        .bind(&steam_ids)
        .bind(&ips)
        .bind(&pings)
        .bind(&ports)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;

    Ok(OnlinePlayersReportResult { server_id })
}

pub async fn list_online_players(
    db: &Database,
    server_id: Uuid,
) -> anyhow::Result<OnlinePlayersResponse> {
    let details = sqlx::query_as::<_, OnlinePlayerItem>(
        r#"
        SELECT name, steam_id64, ip, ping, server_port
        FROM server_online_players
        WHERE server_id = $1
        ORDER BY name ASC
        "#,
    )
    .bind(server_id)
    .fetch_all(&db.pool)
    .await?;

    let players = details.iter().map(|player| player.name.clone()).collect();

    Ok(OnlinePlayersResponse { players, details })
}

pub async fn test_server_input(input: ServerInput) -> anyhow::Result<RconTestResult> {
    test_rcon_connection(&input).await
}

async fn test_rcon_connection(input: &ServerInput) -> anyhow::Result<RconTestResult> {
    let name = input.name.trim();
    let ip = input.ip.trim();
    let password = input.rcon_password.trim();

    anyhow::ensure!(!name.is_empty(), "服务器名称不能为空");
    anyhow::ensure!(!ip.is_empty(), "服务器 IP 不能为空");
    anyhow::ensure!(!password.is_empty(), "RCON 密码不能为空");

    let address = format!("{}:{}", ip, input.port);

    match crate::rcon::RconConnection::connect(&address, password, 3).await {
        Ok(mut conn) => {
            let players = conn
                .execute("listplayers")
                .await
                .map(|response| parse_players_from_response(&response))
                .unwrap_or_default();

            Ok(RconTestResult {
                ok: true,
                message: "RCON 连接测试成功".to_string(),
                players,
            })
        }
        Err(error) => Ok(RconTestResult {
            ok: false,
            message: error,
            players: Vec::new(),
        }),
    }
}

fn parse_players_from_response(response: &str) -> Vec<String> {
    response
        .split(',')
        .map(str::trim)
        .filter(|player| !player.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn normalize_online_player(
    player: OnlinePlayerInput,
    report_port: u16,
) -> anyhow::Result<NormalizedOnlinePlayer> {
    let name = player.name.trim();
    anyhow::ensure!(!name.is_empty(), "玩家名称不能为空");

    let steam_id64 = match player
        .steam_id64
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(value) => value.to_string(),
        None => {
            let steam_id = player
                .steam_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| anyhow::anyhow!("玩家 SteamID64 不能为空"))?;
            super::steam_service::steam2_to_steamid64(steam_id)?
        }
    };

    let server_port = player.server_port.unwrap_or(report_port);
    anyhow::ensure!(
        server_port == report_port,
        "玩家所在服务器端口与上报端口不一致"
    );

    Ok(NormalizedOnlinePlayer {
        name: name.to_string(),
        steam_id64,
        ip: player
            .ip
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("unknown")
            .to_string(),
        ping: player.ping,
        server_port,
    })
}

fn generate_report_token() -> String {
    Uuid::new_v4().simple().to_string()
}

fn is_report_stale(last_reported_at: Option<chrono::DateTime<chrono::Utc>>) -> bool {
    match last_reported_at {
        Some(value) => {
            chrono::Utc::now()
                .signed_duration_since(value)
                .num_seconds()
                > OFFLINE_AFTER_SECONDS
        }
        None => true,
    }
}

async fn mark_stale_servers_offline(db: &Database) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        UPDATE servers
        SET status = 'offline', players = ARRAY[]::TEXT[]
        WHERE last_reported_at IS NOT NULL
          AND last_reported_at < now() - interval '60 seconds'
        "#,
    )
    .execute(&db.pool)
    .await?;

    sqlx::query(
        r#"
        DELETE FROM server_online_players
        WHERE server_id IN (
            SELECT id FROM servers
            WHERE last_reported_at IS NOT NULL
              AND last_reported_at < now() - interval '60 seconds'
        )
        "#,
    )
    .execute(&db.pool)
    .await?;

    Ok(())
}

/// 启动定时清理过期服务器的后台任务（每 30 秒执行一次）
pub fn start_stale_cleanup_loop(db: Database) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
        loop {
            interval.tick().await;
            if let Err(error) = mark_stale_servers_offline(&db).await {
                tracing::warn!(%error, "清理过期服务器状态失败");
            }
        }
    });
}

/// 验证 RCON 命令安全性，阻止破坏性命令
fn validate_rcon_command(command: &str) -> anyhow::Result<()> {
    // 移除前导分号和空格后检查
    let cleaned = command.trim().trim_start_matches(';').trim();
    let cmd = cleaned.to_lowercase();
    anyhow::ensure!(!cmd.is_empty(), "命令不能为空");

    // 按分号和空格分割，检查每个 token
    let keywords: Vec<&str> = cmd
        .split(|c: char| c == ';' || c.is_whitespace())
        .filter(|s| !s.is_empty())
        .collect();

    const BLOCKED_COMMANDS: &[&str] = &[
        "quit",
        "exit",
        "rcon_password",
        "sv_password",
        "servercfgfile",
        "writeid",
        "writeip",
        "banid",
        "removeid",
        "removeip",
        "exec",
        "alias",
        "sm_rcon",
        "changelevel",
        "map",
        "kickid",
        "banip",
        "_restart",
        "restart",
    ];

    for keyword in &keywords {
        anyhow::ensure!(
            !BLOCKED_COMMANDS.contains(keyword),
            "命令 \"{}\" 被禁止执行",
            keyword
        );
    }

    Ok(())
}

/// Execute a RCON command on a specific server
pub async fn execute_rcon_command(
    db: &Database,
    server_id: Uuid,
    command: &str,
) -> anyhow::Result<String> {
    validate_rcon_command(command)?;

    #[derive(sqlx::FromRow)]
    struct ServerRconInfo {
        ip: String,
        port: i32,
        rcon_password: String,
    }

    let server: ServerRconInfo =
        sqlx::query_as(r#"SELECT ip, port, rcon_password FROM servers WHERE id = $1"#)
            .bind(server_id)
            .fetch_optional(&db.pool)
            .await?
            .ok_or_else(|| anyhow::anyhow!("服务器不存在"))?;

    let address = format!("{}:{}", server.ip, server.port);
    let mut conn = crate::rcon::RconConnection::connect(&address, &server.rcon_password, 3)
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    let response = conn
        .execute(command)
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::{test_server_input, ServerInput};
    use std::io::ErrorKind;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    const AUTH_PACKET_TYPE: i32 = 3;
    const AUTH_RESPONSE_PACKET_TYPE: i32 = 2;
    const EXEC_COMMAND_PACKET_TYPE: i32 = 2;
    const RESPONSE_VALUE_PACKET_TYPE: i32 = 0;

    #[tokio::test]
    async fn test_server_input_rejects_wrong_rcon_password() {
        let (port, server) = spawn_fake_rcon_server("correct-password", "玩家甲,玩家乙").await;

        let result = test_server_input(ServerInput {
            name: "一号服".to_string(),
            ip: "127.0.0.1".to_string(),
            port,
            rcon_password: "wrong-password".to_string(),
            report_token: None,
            note: None,
            access_restriction_enabled: false,
            min_rating: 0,
            min_steam_level: 0,
            whitelist_mode_enabled: false,
            use_custom_access: false,
            max_players: 0,
        })
        .await
        .unwrap();

        server.await.unwrap().unwrap();

        assert!(!result.ok, "unexpected success: {}", result.message);
        assert!(
            result.message.contains("密码"),
            "unexpected message: {}",
            result.message
        );
        assert!(
            result.players.is_empty(),
            "unexpected players: {:?}",
            result.players
        );
    }

    #[tokio::test]
    async fn test_server_input_accepts_correct_rcon_password() {
        let (port, server) = spawn_fake_rcon_server("correct-password", "玩家甲,玩家乙").await;

        let result = test_server_input(ServerInput {
            name: "一号服".to_string(),
            ip: "127.0.0.1".to_string(),
            port,
            rcon_password: "correct-password".to_string(),
            report_token: None,
            note: None,
            access_restriction_enabled: false,
            min_rating: 0,
            min_steam_level: 0,
            whitelist_mode_enabled: false,
            use_custom_access: false,
            max_players: 0,
        })
        .await
        .unwrap();

        server.await.unwrap().unwrap();

        assert!(result.ok, "unexpected failure: {}", result.message);
        assert_eq!(result.players, vec!["玩家甲", "玩家乙"]);
    }

    async fn spawn_fake_rcon_server(
        expected_password: &str,
        list_players_response: &str,
    ) -> (u16, tokio::task::JoinHandle<anyhow::Result<()>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let expected_password = expected_password.to_string();
        let list_players_response = list_players_response.to_string();

        let handle = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await?;
            let (request_id, packet_type, body) = match read_packet(&mut stream).await {
                Ok(packet) => packet,
                Err(error) if error.kind() == ErrorKind::UnexpectedEof => return Ok(()),
                Err(error) => return Err(error.into()),
            };

            let auth_success = packet_type == AUTH_PACKET_TYPE && body == expected_password;
            write_packet(&mut stream, request_id, RESPONSE_VALUE_PACKET_TYPE, "").await?;
            write_packet(
                &mut stream,
                if auth_success { request_id } else { -1 },
                AUTH_RESPONSE_PACKET_TYPE,
                "",
            )
            .await?;

            if !auth_success {
                return Ok(());
            }

            let (request_id, packet_type, body) = read_packet(&mut stream).await?;
            if packet_type == EXEC_COMMAND_PACKET_TYPE && body == "listplayers" {
                write_packet(
                    &mut stream,
                    request_id,
                    RESPONSE_VALUE_PACKET_TYPE,
                    &list_players_response,
                )
                .await?;
            }

            Ok(())
        });

        (port, handle)
    }

    async fn read_packet(
        stream: &mut tokio::net::TcpStream,
    ) -> std::io::Result<(i32, i32, String)> {
        let mut size_bytes = [0_u8; 4];
        stream.read_exact(&mut size_bytes).await?;
        let size = i32::from_le_bytes(size_bytes);
        let mut payload = vec![0_u8; size as usize];
        stream.read_exact(&mut payload).await?;

        let mut request_id_bytes = [0_u8; 4];
        request_id_bytes.copy_from_slice(&payload[0..4]);

        let mut packet_type_bytes = [0_u8; 4];
        packet_type_bytes.copy_from_slice(&payload[4..8]);

        Ok((
            i32::from_le_bytes(request_id_bytes),
            i32::from_le_bytes(packet_type_bytes),
            String::from_utf8_lossy(&payload[8..payload.len() - 2]).into_owned(),
        ))
    }

    async fn write_packet(
        stream: &mut tokio::net::TcpStream,
        request_id: i32,
        packet_type: i32,
        body: &str,
    ) -> std::io::Result<()> {
        let size = body.len() + 10;
        let mut packet = Vec::with_capacity(size + 4);
        packet.extend_from_slice(&(size as i32).to_le_bytes());
        packet.extend_from_slice(&request_id.to_le_bytes());
        packet.extend_from_slice(&packet_type.to_le_bytes());
        packet.extend_from_slice(body.as_bytes());
        packet.extend_from_slice(&[0, 0]);
        stream.write_all(&packet).await
    }
}

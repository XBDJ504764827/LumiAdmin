// 社区 RCON 命令执行服务
// 从 community_service.rs 拆分出来，负责 RCON 连接测试和命令执行。

use crate::db::Database;
use crate::services::community_service::ServerInput;
use uuid::Uuid;

/// RCON 连接测试结果
#[derive(serde::Serialize, serde::Deserialize)]
pub struct RconTestResult {
    pub ok: bool,
    pub message: String,
    pub players: Vec<String>,
}

/// 测试服务器 RCON 连接
pub async fn test_server_input(input: ServerInput) -> anyhow::Result<RconTestResult> {
    test_rcon_connection(&input).await
}

pub(crate) async fn test_rcon_connection(input: &ServerInput) -> anyhow::Result<RconTestResult> {
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

/// 验证 RCON 命令安全性，阻止破坏性命令
fn validate_rcon_command(command: &str) -> anyhow::Result<()> {
    let cleaned = command.trim().trim_start_matches(';').trim();
    let cmd = cleaned.to_lowercase();
    anyhow::ensure!(!cmd.is_empty(), "命令不能为空");

    let keywords: Vec<&str> = cmd
        .split(|c: char| c == ';' || c.is_whitespace())
        .filter(|s| !s.is_empty())
        .collect();

    const BLOCKED_COMMANDS: &[&str] = &[
        "quit", "exit", "rcon_password", "sv_password", "servercfgfile",
        "writeid", "writeip", "banid", "removeid", "removeip",
        "exec", "alias", "sm_rcon", "changelevel", "map",
        "kickid", "banip", "_restart", "restart",
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

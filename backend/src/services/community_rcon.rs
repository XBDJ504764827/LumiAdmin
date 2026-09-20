// 社区 RCON 命令执行服务
// 从 community_service.rs 拆分出来，负责 RCON 连接测试和命令执行。

use crate::config::Config;
use crate::db::Database;
use crate::services::community_service::ServerInput;
use uuid::Uuid;

#[derive(Clone, Copy)]
pub struct RconTimeouts {
    pub connect_secs: u64,
    pub io_secs: u64,
}

impl RconTimeouts {
    pub fn from_config(config: &Config) -> Self {
        Self {
            connect_secs: config.rcon_connect_timeout_secs,
            io_secs: config.rcon_io_timeout_secs,
        }
    }
}

impl Default for RconTimeouts {
    fn default() -> Self {
        Self {
            connect_secs: 10,
            io_secs: 10,
        }
    }
}

/// RCON 连接测试结果
#[derive(serde::Serialize, serde::Deserialize)]
pub struct RconTestResult {
    pub ok: bool,
    pub message: String,
    pub players: Vec<String>,
}

/// 测试服务器 RCON 连接
#[cfg(test)]
pub async fn test_server_input(input: ServerInput) -> anyhow::Result<RconTestResult> {
    test_rcon_connection(&input, RconTimeouts::default()).await
}

pub async fn test_server_input_with_timeouts(
    input: ServerInput,
    timeouts: RconTimeouts,
) -> anyhow::Result<RconTestResult> {
    test_rcon_connection(&input, timeouts).await
}

pub(crate) async fn test_rcon_connection(
    input: &ServerInput,
    timeouts: RconTimeouts,
) -> anyhow::Result<RconTestResult> {
    let name = input.name.trim();
    let ip = input.ip.trim();
    let password = input.rcon_password.trim();

    anyhow::ensure!(!name.is_empty(), "服务器名称不能为空");
    anyhow::ensure!(!ip.is_empty(), "服务器 IP 不能为空");
    anyhow::ensure!(!password.is_empty(), "RCON 密码不能为空");

    let address = format!("{}:{}", ip, input.port);

    match crate::rcon::RconConnection::connect_with_timeouts(
        &address,
        password,
        timeouts.connect_secs,
        timeouts.io_secs,
    )
    .await
    {
        Ok(mut conn) => {
            let players = conn
                .execute_with_timeout("listplayers", timeouts.io_secs)
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

/// RCON 命令白名单。
///
/// RCON 具备服务器控制台的全部权限，因此不能依赖黑名单拦截危险命令。
/// developer 由路由层显式允许全部命令；其他管理员只能执行只读诊断命令。
const STAFF_RCON_COMMANDS: &[&str] = &["status", "stats", "version", "listplayers", "sm_version"];

fn validate_rcon_command(command: &str, allow_all: bool) -> anyhow::Result<()> {
    let cleaned = command.trim().trim_start_matches(';').trim();
    anyhow::ensure!(!cleaned.is_empty(), "命令不能为空");
    anyhow::ensure!(cleaned.len() <= 512, "RCON 命令长度不能超过 512 个字符");
    anyhow::ensure!(
        !cleaned.contains(';') && !cleaned.contains('\n') && !cleaned.contains('\r'),
        "不允许执行多条 RCON 命令"
    );

    if allow_all {
        return Ok(());
    }

    let tokens: Vec<&str> = cleaned.split_whitespace().collect();
    let command_name = tokens.first().copied().unwrap_or_default().to_lowercase();
    let allowed = STAFF_RCON_COMMANDS.contains(&command_name.as_str()) && tokens.len() == 1
        || tokens.len() >= 2
            && tokens[0].eq_ignore_ascii_case("sm")
            && ((tokens[1].eq_ignore_ascii_case("version") && tokens.len() == 2)
                || (tokens[1].eq_ignore_ascii_case("plugins")
                    && (tokens.len() == 3
                        && (tokens[2].eq_ignore_ascii_case("list")
                            || tokens[2].eq_ignore_ascii_case("info"))
                        || tokens.len() == 4 && tokens[2].eq_ignore_ascii_case("info"))));
    anyhow::ensure!(allowed, "当前管理员只能执行 RCON 白名单命令");
    Ok(())
}

/// Execute a RCON command on a specific server
pub async fn execute_rcon_command(
    db: &Database,
    server_id: Uuid,
    command: &str,
    timeouts: RconTimeouts,
    allow_all: bool,
) -> anyhow::Result<String> {
    validate_rcon_command(command, allow_all)?;

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
    let mut conn = crate::rcon::RconConnection::connect_with_timeouts(
        &address,
        &server.rcon_password,
        timeouts.connect_secs,
        timeouts.io_secs,
    )
    .await
    .map_err(|e| anyhow::anyhow!("{}", e))?;

    let response = conn
        .execute_with_timeout(command, timeouts.io_secs)
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    // 防止恶意/异常服务器响应占用过多内存并污染日志/UI。
    Ok(response.chars().take(16 * 1024).collect())
}

/// 日志中只保留命令名和参数数量，避免把密码、配置或敏感输出写入审计日志。
pub fn audit_command(command: &str) -> String {
    let mut parts = command.split_whitespace();
    let name = parts.next().unwrap_or("unknown");
    let arg_count = parts.count();
    format!("{name}（参数 {arg_count} 个）")
}

#[cfg(test)]
mod tests {
    use super::validate_rcon_command;

    #[test]
    fn staff_commands_are_strictly_whitelisted() {
        assert!(validate_rcon_command("status", false).is_ok());
        assert!(validate_rcon_command("sm version", false).is_ok());
        assert!(validate_rcon_command("sm plugins list", false).is_ok());
        assert!(validate_rcon_command("sm plugins info 1", false).is_ok());
        assert!(validate_rcon_command("sm plugins info; quit", false).is_err());
        assert!(validate_rcon_command("sm_rcon sv_cheats 1", false).is_err());
        assert!(validate_rcon_command("exec autoexec.cfg", false).is_err());
    }

    #[test]
    fn developer_may_execute_one_arbitrary_command() {
        assert!(validate_rcon_command("sv_cheats 1", true).is_ok());
    }
}

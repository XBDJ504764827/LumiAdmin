use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::timeout;

const AUTH_REQUEST_ID: i32 = 1;
const AUTH_PACKET_TYPE: i32 = 3;
const AUTH_RESPONSE_PACKET_TYPE: i32 = 2;
const EXEC_COMMAND_PACKET_TYPE: i32 = 2;
const RESPONSE_VALUE_PACKET_TYPE: i32 = 0;

pub struct RconConnection {
    stream: TcpStream,
}

impl RconConnection {
    pub async fn connect(address: &str, password: &str, timeout_secs: u64) -> Result<Self, String> {
        Self::connect_with_timeouts(address, password, timeout_secs, timeout_secs).await
    }

    pub async fn connect_with_timeouts(
        address: &str,
        password: &str,
        connect_timeout_secs: u64,
        io_timeout_secs: u64,
    ) -> Result<Self, String> {
        let connect_result = timeout(
            Duration::from_secs(connect_timeout_secs),
            TcpStream::connect(address),
        )
        .await;
        let mut stream = match connect_result {
            Ok(Ok(stream)) => stream,
            Ok(Err(error)) => return Err(format!("无法连接到服务器 {}: {}", address, error)),
            Err(_) => {
                return Err(format!(
                    "连接服务器 {} 超时（TCP 连接在 {} 秒内未建立，请检查运行后端的机器到该地址的出站网络、防火墙/安全组，以及游戏服是否开放 TCP RCON 端口）",
                    address, connect_timeout_secs
                ))
            }
        };

        timeout(
            Duration::from_secs(io_timeout_secs),
            send_rcon_packet(&mut stream, AUTH_REQUEST_ID, AUTH_PACKET_TYPE, password),
        )
        .await
        .map_err(|_| format!("发送 RCON 认证请求超时（{} 秒）", io_timeout_secs))?
        .map_err(|error| format!("发送 RCON 认证请求失败: {}", error))?;

        loop {
            let (request_id, packet_type, _) = timeout(
                Duration::from_secs(io_timeout_secs),
                read_rcon_packet(&mut stream),
            )
            .await
            .map_err(|_| {
                format!(
                    "读取 RCON 认证响应超时（TCP 已连接，但服务器 {} 秒内没有返回认证响应）",
                    io_timeout_secs
                )
            })?
            .map_err(|error| format!("读取 RCON 认证响应失败: {}", error))?;

            if packet_type != AUTH_RESPONSE_PACKET_TYPE {
                continue;
            }
            if request_id == AUTH_REQUEST_ID {
                return Ok(Self { stream });
            }
            if request_id == -1 {
                return Err("RCON 密码错误，认证失败".to_string());
            }
            return Err("服务器返回了无效的 RCON 认证响应".to_string());
        }
    }

    pub async fn execute(&mut self, command: &str) -> Result<String, String> {
        self.execute_with_timeout(command, 10).await
    }

    pub async fn execute_with_timeout(
        &mut self,
        command: &str,
        io_timeout_secs: u64,
    ) -> Result<String, String> {
        timeout(
            Duration::from_secs(io_timeout_secs),
            send_rcon_packet(
                &mut self.stream,
                AUTH_REQUEST_ID,
                EXEC_COMMAND_PACKET_TYPE,
                command,
            ),
        )
        .await
        .map_err(|_| format!("发送 RCON 命令超时（{} 秒）", io_timeout_secs))?
        .map_err(|error| format!("发送 RCON 命令失败: {}", error))?;

        let (request_id, packet_type, body) = timeout(
            Duration::from_secs(io_timeout_secs),
            read_rcon_packet(&mut self.stream),
        )
        .await
        .map_err(|_| format!("读取 RCON 命令响应超时（{} 秒）", io_timeout_secs))?
        .map_err(|error| format!("读取 RCON 命令响应失败: {}", error))?;

        if request_id != AUTH_REQUEST_ID || packet_type != RESPONSE_VALUE_PACKET_TYPE {
            return Err("服务器返回了无效的 RCON 命令响应".to_string());
        }
        Ok(body)
    }
}

async fn send_rcon_packet(
    stream: &mut TcpStream,
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

async fn read_rcon_packet(stream: &mut TcpStream) -> std::io::Result<(i32, i32, String)> {
    let mut size_bytes = [0_u8; 4];
    stream.read_exact(&mut size_bytes).await?;
    let size = i32::from_le_bytes(size_bytes);
    if size < 10 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "RCON 响应长度无效",
        ));
    }
    let mut payload = vec![0_u8; size as usize];
    stream.read_exact(&mut payload).await?;
    let mut rid = [0_u8; 4];
    rid.copy_from_slice(&payload[0..4]);
    let mut pt = [0_u8; 4];
    pt.copy_from_slice(&payload[4..8]);
    Ok((
        i32::from_le_bytes(rid),
        i32::from_le_bytes(pt),
        String::from_utf8_lossy(&payload[8..payload.len() - 2]).into_owned(),
    ))
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct StatusResult {
    pub server_name: String,
    pub current_map: String,
    pub player_count: i32,
    pub max_players: i32,
    pub players: Vec<String>,
}

pub fn parse_status_output(output: &str) -> StatusResult {
    let mut result = StatusResult::default();
    for line in output.lines() {
        let line = line.trim();
        // 将 key 和 value 按 "key  : value" 或 "key: value" 格式分割
        // 字段名和冒号之间可能有多个空格
        if let Some(colon_pos) = line.find(':') {
            let key = line[..colon_pos].trim();
            let value = line[colon_pos + 1..].trim();
            match key {
                "hostname" => {
                    result.server_name = value.to_string();
                }
                "map" => {
                    // map 行可能有额外信息: "kz_slumpfrageous"
                    // 取第一个空白分隔前的部分
                    result.current_map = value.split_whitespace().next().unwrap_or("").to_string();
                }
                "players" => {
                    // 格式可能有多种:
                    // "5/24" — 标准
                    // "2 humans, 0 bots (16/0 max)" — CS:GO 新版
                    // "5 (24) bots" — 另一种格式
                    // 先尝试提取所有数字，找 (N/M max) 模式
                    if let Some(max_info) = extract_max_players(value) {
                        result.player_count = max_info.0;
                        result.max_players = max_info.1;
                    } else if let Some(slash) = value.find('/') {
                        let before = value[..slash].trim();
                        let after = value[slash + 1..].split_whitespace().next().unwrap_or("0");
                        result.player_count = before.parse().unwrap_or(0);
                        result.max_players = after.parse().unwrap_or(0);
                    } else if let Some(n) = value.split_whitespace().next() {
                        result.player_count = n.parse().unwrap_or(0);
                    }
                }
                _ => {}
            }
        }
    }
    for line in output.lines() {
        let line = line.trim();
        if line.starts_with('#') && line.contains('"') {
            if let Some(name) = extract_quoted_name(line) {
                result.players.push(name);
            }
        }
    }
    result
}

/// 从 "2 humans, 0 bots (16/0 max)" 格式中提取 (current_humans, max_players)
fn extract_max_players(value: &str) -> Option<(i32, i32)> {
    use std::sync::LazyLock;
    static MAX_RE: LazyLock<regex::Regex> =
        LazyLock::new(|| regex::Regex::new(r"\((\d+)/(\d+)\s*max\)").unwrap());
    static HUMAN_RE: LazyLock<regex::Regex> =
        LazyLock::new(|| regex::Regex::new(r"^(\d+)\s+human").unwrap());

    let caps = MAX_RE.captures(value)?;
    let max_players: i32 = caps.get(1)?.as_str().parse().ok()?;

    let human_caps = HUMAN_RE.captures(value);
    let player_count = human_caps
        .and_then(|c| c.get(1)?.as_str().parse::<i32>().ok())
        .unwrap_or(0);

    Some((player_count, max_players))
}

fn extract_quoted_name(line: &str) -> Option<String> {
    let mut in_quotes = false;
    let mut name = String::new();
    let mut found_first = false;
    for ch in line.chars() {
        if ch == '"' {
            if in_quotes {
                if found_first {
                    return Some(name);
                }
                in_quotes = false;
            } else {
                if found_first {
                    return None;
                }
                found_first = true;
                in_quotes = true;
            }
        } else if in_quotes && found_first {
            name.push(ch);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;

    #[test]
    fn parse_csgo_kz_status() {
        let output = r#"hostname: CNGOKZ 2服
version : 1.38.8.1/13881 1575/8853 secure  [G:1:15912410]
udp/ip  : 103.219.30.7:10002  (public ip: 103.219.30.7)
os      :  Linux
type    :  community dedicated
map     : kz_slumpfrageous
players : 2 humans, 0 bots (16/0 max) (not hibernating)

# userid name uniqueid connected ping loss state rate adr
# 91 2 ".mONESY" STEAM_1:1:712722834 10:31 64 0 active 196608 222.172.181.86:26983
# 92 3 "灵活的小舌头" STEAM_1:1:215367673 08:09 43 0 active 196608 112.255.145.196:9050
#end"#;

        let result = parse_status_output(output);
        assert_eq!(result.server_name, "CNGOKZ 2服");
        assert_eq!(result.current_map, "kz_slumpfrageous");
        assert_eq!(result.player_count, 2);
        assert_eq!(result.max_players, 16);
        assert_eq!(result.players, vec![".mONESY", "灵活的小舌头"]);
    }

    #[test]
    fn parse_standard_status() {
        let output = r#"hostname: My CS Server
version: 1.38.3.9 secure
map: de_dust2
players: 5/24
# userid name uniqueid connected ping loss state
# 1 "Alice" STEAM_0:1:12345 05:23 35 0 active
# 2 "Bob" STEAM_0:0:67890 12:45 42 0 active"#;

        let result = parse_status_output(output);
        assert_eq!(result.server_name, "My CS Server");
        assert_eq!(result.current_map, "de_dust2");
        assert_eq!(result.player_count, 5);
        assert_eq!(result.max_players, 24);
        assert_eq!(result.players, vec!["Alice", "Bob"]);
    }

    #[tokio::test]
    async fn connect_times_out_when_auth_response_never_arrives() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (_stream, _) = listener.accept().await.unwrap();
            tokio::time::sleep(Duration::from_secs(2)).await;
        });

        let result =
            RconConnection::connect_with_timeouts(&address.to_string(), "secret", 1, 1).await;

        server.abort();
        match result {
            Ok(_) => panic!("expected auth response timeout"),
            Err(error) => assert!(error.contains("认证响应超时")),
        }
    }
}

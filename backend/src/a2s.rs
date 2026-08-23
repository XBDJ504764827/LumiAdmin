use std::net::SocketAddr;
use std::time::Duration;
use tokio::net::UdpSocket;
use tokio::time::timeout;

const A2S_INFO_REQUEST: &[u8] = b"\xFF\xFF\xFF\xFF\x54Source Engine Query\x00";
const A2S_PLAYER_REQUEST: &[u8] = b"\xFF\xFF\xFF\xFF\x55\xFF\xFF\xFF\xFF";

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct ServerInfo {
    pub server_name: String,
    pub current_map: String,
    pub player_count: i32,
    pub max_players: i32,
    pub bot_count: i32,
    pub players: Vec<PlayerInfo>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PlayerInfo {
    pub name: String,
    pub score: i32,
    pub duration: f32,
}

pub async fn query_server(address: &str, timeout_secs: u64) -> Result<ServerInfo, String> {
    // 使用 Tokio UDP，避免同步 socket 在异步任务中阻塞运行时线程。
    let socket = UdpSocket::bind("0.0.0.0:0")
        .await
        .map_err(|e| format!("绑定 UDP 端口失败: {e}"))?;
    let target: SocketAddr = tokio::net::lookup_host(address)
        .await
        .map_err(|e| format!("解析 {address} 失败: {e}"))?
        .next()
        .ok_or_else(|| format!("无法解析服务器地址: {address}"))?;
    socket
        .connect(target)
        .await
        .map_err(|e| format!("连接 {address} 失败: {e}"))?;
    let timeout_duration = Duration::from_secs(timeout_secs.max(1));

    // A2S_INFO（支持 challenge 重发）
    socket
        .send(A2S_INFO_REQUEST)
        .await
        .map_err(|e| format!("发送 A2S_INFO 失败: {e}"))?;
    let mut buf = [0u8; 4096];
    let n = timeout(timeout_duration, socket.recv(&mut buf))
        .await
        .map_err(|_| format!("接收 A2S_INFO 响应超时（{}s）", timeout_secs))?
        .map_err(|e| format!("接收 A2S_INFO 响应失败: {e}"))?;

    let info_len = if n >= 9
        && buf[0] == 0xFF
        && buf[1] == 0xFF
        && buf[2] == 0xFF
        && buf[3] == 0xFF
        && buf[4] == 0x41
    {
        let mut challenge_req = A2S_INFO_REQUEST.to_vec();
        challenge_req.extend_from_slice(&buf[5..9]);
        socket
            .send(&challenge_req)
            .await
            .map_err(|e| format!("发送 A2S_INFO challenge 失败: {e}"))?;
        timeout(timeout_duration, socket.recv(&mut buf))
            .await
            .map_err(|_| format!("接收 A2S_INFO 响应超时（{}s）", timeout_secs))?
            .map_err(|e| format!("接收 A2S_INFO 响应失败: {e}"))?
    } else {
        n
    };

    let info = parse_a2s_info(&buf[..info_len])?;
    let players = query_players(&socket, timeout_duration)
        .await
        .unwrap_or_default();

    Ok(ServerInfo {
        server_name: info.0,
        current_map: info.1,
        player_count: info.2,
        max_players: info.3,
        bot_count: info.4,
        players,
    })
}

async fn query_players(
    socket: &UdpSocket,
    timeout_duration: Duration,
) -> Result<Vec<PlayerInfo>, String> {
    socket
        .send(A2S_PLAYER_REQUEST)
        .await
        .map_err(|e| format!("发送 A2S_PLAYER 失败: {e}"))?;
    let mut buf = [0u8; 4096];
    let n = timeout(timeout_duration, socket.recv(&mut buf))
        .await
        .map_err(|_| "接收 A2S_PLAYER 响应超时".to_string())?
        .map_err(|e| format!("接收 A2S_PLAYER 响应失败: {e}"))?;

    if n >= 9
        && buf[0] == 0xFF
        && buf[1] == 0xFF
        && buf[2] == 0xFF
        && buf[3] == 0xFF
        && buf[4] == 0x41
    {
        let mut challenge_req = vec![0xFF, 0xFF, 0xFF, 0xFF, 0x55];
        challenge_req.extend_from_slice(&buf[5..9]);
        socket
            .send(&challenge_req)
            .await
            .map_err(|e| format!("发送 A2S_PLAYER challenge 失败: {e}"))?;
        let n2 = timeout(timeout_duration, socket.recv(&mut buf))
            .await
            .map_err(|_| "接收 A2S_PLAYER 响应超时".to_string())?
            .map_err(|e| format!("接收 A2S_PLAYER 响应失败: {e}"))?;
        parse_a2s_player(&buf[..n2])
    } else {
        parse_a2s_player(&buf[..n])
    }
}

/// 解析 A2S_INFO 响应，返回 (name, map, players, max_players, bots)
fn parse_a2s_info(data: &[u8]) -> Result<(String, String, i32, i32, i32), String> {
    if data.len() < 5 || data[4] != 0x49 {
        return Err(format!(
            "无效的 A2S_INFO 响应头: {:02X}",
            data.get(4).unwrap_or(&0)
        ));
    }

    let mut pos = 5;
    pos += 1; // Protocol
    if pos > data.len() {
        return Err("A2S_INFO 响应缺少协议字段".to_string());
    }
    let server_name = read_cstring(data, &mut pos);
    let current_map = read_cstring(data, &mut pos);
    let _folder = read_cstring(data, &mut pos);
    let _game = read_cstring(data, &mut pos);
    pos += 2; // AppID
    let player_count = *data.get(pos).unwrap_or(&0) as i32;
    pos += 1;
    let max_players = *data.get(pos).unwrap_or(&0) as i32;
    pos += 1;
    let bot_count = *data.get(pos).unwrap_or(&0) as i32;

    Ok((
        server_name,
        current_map,
        player_count,
        max_players,
        bot_count,
    ))
}

fn parse_a2s_player(data: &[u8]) -> Result<Vec<PlayerInfo>, String> {
    if data.len() < 5 || data[4] != 0x44 {
        return Err(format!(
            "无效的 A2S_PLAYER 响应头: {:02X}",
            data.get(4).unwrap_or(&0)
        ));
    }

    let mut pos = 5;
    let count = *data.get(pos).unwrap_or(&0) as usize;
    pos += 1;

    let mut players = Vec::with_capacity(count);
    for _ in 0..count {
        if pos >= data.len() {
            break;
        }
        pos += 1; // Index (1 byte)
        let name = read_cstring(data, &mut pos);
        let score = if pos + 4 <= data.len() {
            i32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]])
        } else {
            0
        };
        pos += 4;
        let duration = if pos + 4 <= data.len() {
            f32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]])
        } else {
            0.0
        };
        pos += 4;
        players.push(PlayerInfo {
            name,
            score,
            duration,
        });
    }
    Ok(players)
}

fn read_cstring(data: &[u8], pos: &mut usize) -> String {
    let start = *pos;
    while *pos < data.len() && data[*pos] != 0 {
        *pos += 1;
    }
    let s = String::from_utf8_lossy(&data[start..*pos]).into_owned();
    *pos += 1; // skip null terminator
    s
}

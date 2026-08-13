//! 休眠服务器 RCON 兜底轮询。
//!
//! CS:GO 服务器空服后进入休眠（`sv_hibernate_when_empty 1` 默认开启），游戏主循环停摆，
//! SourceMod 定时器不再触发，插件无法继续上报在线玩家/服务器状态，后台数据停留在玩家退出前。
//! 休眠状态下引擎仍响应 RCON 命令，因此本服务在插件上报过期后，通过 RCON 执行
//! `status` / `stats` 获取服务器真实状态并回写数据库，保证空服期间数据持续刷新。
//!
//! 玩家进入后插件恢复上报（`last_reported_at` 恢复新鲜），本服务自动停止对该服务器的轮询，
//! 全程无需修改插件，服务器也无需保持唤醒状态。

use crate::config::Config;
use crate::db::Database;
use crate::rcon::{
    parse_stats_output, parse_status_output, RconConnection, StatsResult, StatusResult,
};
use crate::services::{community_service::SessionEndReason, observability_service};
use chrono::{DateTime, Utc};
use futures::StreamExt;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use uuid::Uuid;

/// RCON 连接失败后的冷却时间：避免对已经宕机/关机的服务器反复 TCP 超时浪费资源
const FAIL_COOLDOWN_SECS: u64 = 300;

/// 休眠兜底轮询配置
#[derive(Debug, Clone)]
pub struct HibernationPollConfig {
    /// 扫描数据库间隔（秒）
    pub scan_interval_secs: u64,
    /// 每台休眠服务器的轮询间隔（秒）
    pub poll_interval_secs: u64,
    /// `last_reported_at` 超过多久未刷新即视为进入休眠（秒）
    pub stale_after_secs: i64,
    pub connect_timeout_secs: u64,
    pub io_timeout_secs: u64,
}

impl HibernationPollConfig {
    pub fn from_config(config: &Config) -> Self {
        Self {
            scan_interval_secs: config.hibernation_poll_scan_interval_secs,
            poll_interval_secs: config.hibernation_poll_interval_secs,
            stale_after_secs: config.hibernation_poll_after_secs,
            connect_timeout_secs: config.rcon_connect_timeout_secs,
            io_timeout_secs: config.rcon_io_timeout_secs,
        }
    }
}

/// 启动休眠兜底轮询循环。
pub fn start_hibernation_poll_loop(db: Database, config: HibernationPollConfig) {
    observability_service::register_task(
        "hibernation_rcon_poll",
        "休眠服务器 RCON 兜底轮询",
        "集成",
        Some(config.scan_interval_secs),
        true,
    );
    tokio::spawn(async move {
        let mut poller = HibernationPoller::new(db, config.clone());
        let mut interval = tokio::time::interval(Duration::from_secs(config.scan_interval_secs));
        // 首个周期先等待，给插件正常上报留出机会
        interval.tick().await;
        loop {
            interval.tick().await;
            match observability_service::observe_task(
                "hibernation_rcon_poll",
                poller.poll_once(),
                |count| format!("轮询 {} 台休眠服务器", count),
            )
            .await
            {
                Ok(_) => {}
                Err(error) => tracing::warn!(%error, "休眠服务器轮询周期执行失败"),
            }
        }
    });
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct PollCandidate {
    id: Uuid,
    name: String,
    ip: String,
    port: i32,
    rcon_password: String,
}

pub struct HibernationPoller {
    db: Database,
    config: HibernationPollConfig,
    /// 每台服务器上次成功轮询时间（用于按 poll_interval 节流）
    last_success: HashMap<Uuid, Instant>,
    /// 每台服务器上次失败时间（失败冷却，避免对故障服务器反复超时）
    last_fail: HashMap<Uuid, Instant>,
}

impl HibernationPoller {
    pub fn new(db: Database, config: HibernationPollConfig) -> Self {
        Self {
            db,
            config,
            last_success: HashMap::new(),
            last_fail: HashMap::new(),
        }
    }

    /// 执行一轮轮询：扫描上报过期的服务器并通过 RCON 刷新其状态。
    /// 返回成功轮询的服务器数量。
    pub async fn poll_once(&mut self) -> anyhow::Result<usize> {
        let candidates: Vec<PollCandidate> = sqlx::query_as(
            r#"
            SELECT id, name, ip, port, rcon_password
            FROM servers
            WHERE last_reported_at IS NOT NULL
              AND last_reported_at < now() - $1::INTERVAL
              AND ip IS NOT NULL AND btrim(ip) <> ''
              AND port IS NOT NULL AND port > 0
              AND rcon_password IS NOT NULL AND btrim(rcon_password) <> ''
            "#,
        )
        .bind(format!("{} seconds", self.config.stale_after_secs))
        .fetch_all(&self.db.pool)
        .await?;

        let now = Instant::now();
        let poll_interval = Duration::from_secs(self.config.poll_interval_secs);
        let fail_cooldown = Duration::from_secs(FAIL_COOLDOWN_SECS);

        let to_query: Vec<PollCandidate> = candidates
            .into_iter()
            .filter(|candidate| {
                let since_success = self
                    .last_success
                    .get(&candidate.id)
                    .is_none_or(|t| now.duration_since(*t) >= poll_interval);
                let out_of_cooldown = self
                    .last_fail
                    .get(&candidate.id)
                    .is_none_or(|t| now.duration_since(*t) >= fail_cooldown);
                since_success && out_of_cooldown
            })
            .collect();

        if to_query.is_empty() {
            return Ok(0);
        }

        let futures = to_query.into_iter().map(|server| {
            let db = self.db.clone();
            let config = self.config.clone();
            async move {
                let result = poll_server(&db, &config, &server).await;
                (server.id, result)
            }
        });

        let results: Vec<(Uuid, anyhow::Result<()>)> = futures::stream::iter(futures)
            .buffer_unordered(5)
            .collect()
            .await;

        let mut success = 0usize;
        for (server_id, result) in results {
            match result {
                Ok(()) => {
                    self.last_success.insert(server_id, now);
                    success += 1;
                }
                Err(error) => {
                    self.last_fail.insert(server_id, now);
                    tracing::warn!(server_id = %server_id, %error, "休眠服务器 RCON 兜底轮询失败（服务器可能已离线）");
                }
            }
        }

        Ok(success)
    }
}

/// 通过 RCON 采集单台服务器状态并回写数据库。
async fn poll_server(
    db: &Database,
    config: &HibernationPollConfig,
    server: &PollCandidate,
) -> anyhow::Result<()> {
    let address = format!("{}:{}", server.ip, server.port);
    let mut conn = RconConnection::connect_with_timeouts(
        &address,
        &server.rcon_password,
        config.connect_timeout_secs,
        config.io_timeout_secs,
    )
    .await
    .map_err(|e| anyhow::anyhow!("{}", e))?;

    let status_output = conn
        .execute_with_timeout("status", config.io_timeout_secs)
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    let status = parse_status_output(&status_output);

    let stats_output = conn
        .execute_with_timeout("stats", config.io_timeout_secs)
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    let stats = parse_stats_output(&stats_output);

    tracing::debug!(
        server = %server.name,
        players = status.player_count,
        map = %status.current_map,
        hibernating = status.hibernating,
        "休眠服务器 RCON 兜底轮询成功"
    );

    apply_poll_result(db, server, &status, &stats).await?;
    Ok(())
}

/// 将 RCON 采集结果写入数据库：
/// - 状态历史（标记 source = 'rcon'）
/// - 服务器汇总（status/last_reported_at/max_players/players）
/// - 空服时清空在线玩家详情，并关闭残留的活跃会话（避免会话悬挂）
async fn apply_poll_result(
    db: &Database,
    server: &PollCandidate,
    status: &StatusResult,
    stats: &StatsResult,
) -> anyhow::Result<()> {
    let mut tx = db.pool.begin().await?;
    let (now,): (DateTime<Utc>,) = sqlx::query_as("SELECT now()").fetch_one(&mut *tx).await?;

    sqlx::query(
        r#"
        INSERT INTO server_status_history (
            id, server_id, fps, cpu_usage, tickrate, in_rate, out_rate,
            uptime_seconds, players_count, max_players, current_map, reported_at, source
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, 'rcon')
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(server.id)
    .bind(stats.fps)
    .bind(stats.cpu_usage)
    // 休眠时引擎不输出 tickrate，取 FPS 作为近似（128 tick 服 FPS 通常等于 tickrate）
    .bind(stats.fps)
    .bind(stats.in_rate)
    .bind(stats.out_rate)
    .bind(stats.uptime_seconds)
    .bind(status.player_count)
    .bind(status.max_players)
    .bind(&status.current_map)
    .bind(now)
    .execute(&mut *tx)
    .await?;

    // 探测到空服/休眠时如实回写休眠状态，避免兜底轮询把状态刷新回『在线』
    // （插件恢复上报或新玩家进入后，report_online_players 会重新置回 online）。
    let new_status = if status.hibernating || status.player_count == 0 {
        "hibernating"
    } else {
        "online"
    };

    sqlx::query(
        r#"
        UPDATE servers
        SET status = $5,
            last_reported_at = $2,
            max_players = GREATEST(max_players, $3),
            players = $4
        WHERE id = $1
        "#,
    )
    .bind(server.id)
    .bind(now)
    .bind(status.max_players)
    .bind(&status.players)
    .bind(new_status)
    .execute(&mut *tx)
    .await?;

    if status.player_count == 0 {
        // 空服：清空在线玩家详情（RCON 无法提供 steamid/ip 等详情，插件恢复上报后会自动重建）
        sqlx::query(r#"DELETE FROM server_online_players WHERE server_id = $1"#)
            .bind(server.id)
            .execute(&mut *tx)
            .await?;

        // 关闭残留的活跃会话：最后一名玩家退出时插件若来不及上报断开（服务器立即休眠），
        // 会话会悬挂在此；确认空服后统一关闭。
        sqlx::query(
            r#"
            UPDATE player_server_sessions
            SET left_at = $2,
                end_reason = $3,
                end_detail = $4,
                updated_at = $2
            WHERE server_id = $1 AND left_at IS NULL
            "#,
        )
        .bind(server.id)
        .bind(now)
        .bind(SessionEndReason::ServerEmpty.as_str())
        .bind("服务器空服进入休眠，由后端 RCON 兜底轮询关闭会话。")
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::test_util;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    fn schema_url(base_url: &str, schema: &str) -> String {
        test_util::schema_url(base_url, schema)
    }

    const HIBERNATING_STATUS: &str = "hostname: CNGOKZ 测试服\n\
version : 1.38.8.1/13881 1575/8853 secure  [G:1:15912410]\n\
udp/ip  : 127.0.0.1:27015  (public ip: 127.0.0.1)\n\
os      :  Linux\n\
type    :  community dedicated\n\
map     : kz_slumpfrageous\n\
players : 0 humans, 0 bots (16/0 max) (hibernating)\n\
\n\
#end\n";

    const ACTIVE_STATUS: &str = "hostname: CNGOKZ 测试服\n\
version : 1.38.8.1/13881 1575/8853 secure  [G:1:15912410]\n\
udp/ip  : 127.0.0.1:27015  (public ip: 127.0.0.1)\n\
os      :  Linux\n\
type    :  community dedicated\n\
map     : kz_slumpfrageous\n\
players : 2 humans, 0 bots (16/0 max) (not hibernating)\n\
\n\
# userid name uniqueid connected ping loss state rate adr\n\
# 91 2 \".mONESY\" STEAM_1:1:712722834 10:31 64 0 active 196608 222.172.181.86:26983\n\
# 92 3 \"灵活的小舌头\" STEAM_1:1:215367673 08:09 43 0 active 196608 112.255.145.196:9050\n\
#end\n";

    const ACTIVE_STATS: &str = "CPU   In    Out   Uptime  Map changes  FPS   Players  Connects\n\
1.20  2.5   1.1   12:34  0            128   2        12\n";

    const EMPTY_STATS: &str = "CPU   In    Out   Uptime  Map changes  FPS   Players  Connects\n\
0.00  0.0   0.0   12:34  0            0     0        0\n";

    /// 启动一个模拟 CS:GO RCON 服务器的 TCP listener，返回 "127.0.0.1:port" 地址。
    /// 认证成功后对 `status` / `stats` 命令返回固定输出。
    fn spawn_fake_rcon_server(status_output: &'static str, stats_output: &'static str) -> String {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind failed");
        let address = listener.local_addr().unwrap().to_string();
        listener.set_nonblocking(true).unwrap();
        let listener = tokio::net::TcpListener::from_std(listener).unwrap();

        tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    break;
                };
                tokio::spawn(async move {
                    // 认证请求
                    let Some((_id, packet_type, _)) = read_packet(&mut stream).await else {
                        return;
                    };
                    if packet_type != 3 {
                        return;
                    }
                    let _ = write_packet(&mut stream, 1, 2, "").await;
                    loop {
                        let Some((_id, _packet_type, body)) = read_packet(&mut stream).await else {
                            return;
                        };
                        match body.as_str() {
                            "status" => {
                                if write_packet(&mut stream, 1, 0, status_output)
                                    .await
                                    .is_err()
                                {
                                    return;
                                }
                            }
                            "stats" => {
                                if write_packet(&mut stream, 1, 0, stats_output).await.is_err() {
                                    return;
                                }
                            }
                            _ => return,
                        }
                    }
                });
            }
        });
        address
    }

    async fn read_packet(stream: &mut tokio::net::TcpStream) -> Option<(i32, i32, String)> {
        let mut size_bytes = [0_u8; 4];
        stream.read_exact(&mut size_bytes).await.ok()?;
        let size = i32::from_le_bytes(size_bytes);
        if size < 10 {
            return None;
        }
        let mut payload = vec![0_u8; size as usize];
        stream.read_exact(&mut payload).await.ok()?;
        let id = i32::from_le_bytes(payload[0..4].try_into().ok()?);
        let packet_type = i32::from_le_bytes(payload[4..8].try_into().ok()?);
        Some((
            id,
            packet_type,
            String::from_utf8_lossy(&payload[8..payload.len() - 2]).into_owned(),
        ))
    }

    async fn write_packet(
        stream: &mut tokio::net::TcpStream,
        id: i32,
        packet_type: i32,
        body: &str,
    ) -> std::io::Result<()> {
        let size = body.len() + 10;
        let mut packet = Vec::with_capacity(size + 4);
        packet.extend_from_slice(&(size as i32).to_le_bytes());
        packet.extend_from_slice(&id.to_le_bytes());
        packet.extend_from_slice(&packet_type.to_le_bytes());
        packet.extend_from_slice(body.as_bytes());
        packet.extend_from_slice(&[0, 0]);
        stream.write_all(&packet).await
    }

    fn test_config() -> HibernationPollConfig {
        HibernationPollConfig {
            scan_interval_secs: 1,
            poll_interval_secs: 1,
            stale_after_secs: 90,
            connect_timeout_secs: 3,
            io_timeout_secs: 3,
        }
    }

    async fn insert_hibernating_server(db: &Database, address: &str, stale_seconds: i64) -> Uuid {
        let community_id = Uuid::new_v4();
        let server_id = Uuid::new_v4();
        sqlx::query(r#"INSERT INTO communities (id, name) VALUES ($1, $2)"#)
            .bind(community_id)
            .bind("测试社区")
            .execute(&db.pool)
            .await
            .unwrap();

        let (ip, port) = address.rsplit_once(':').unwrap();
        sqlx::query(
            r#"
            INSERT INTO servers (id, community_id, name, ip, port, rcon_password, status, players, last_reported_at, report_token)
            VALUES ($1, $2, $3, $4, $5, $6, 'online', $7, now() - $8 * INTERVAL '1 second', $9)
            "#,
        )
        .bind(server_id)
        .bind(community_id)
        .bind("测试服")
        .bind(ip)
        .bind(port.parse::<i32>().unwrap())
        .bind("secret")
        .bind(vec!["旧玩家".to_string()])
        .bind(stale_seconds)
        .bind(Uuid::new_v4().to_string())
        .execute(&db.pool)
        .await
        .unwrap();
        server_id
    }

    async fn insert_active_session(db: &Database, server_id: Uuid) {
        sqlx::query(
            r#"
            INSERT INTO player_server_sessions (
                server_id, server_name, server_port, community_id, community_name,
                steam_id64, player_name, ip, first_seen_at, last_seen_at, last_map
            )
            SELECT id, name, port, community_id, '测试社区', '76561198000000000', '悬挂玩家', '1.2.3.4',
                   now() - INTERVAL '1 hour', now() - INTERVAL '1 minute', 'kz_slumpfrageous'
            FROM servers WHERE id = $1
            "#,
        )
        .bind(server_id)
        .execute(&db.pool)
        .await
        .unwrap();
        sqlx::query(
            r#"
            INSERT INTO server_online_players (server_id, name, steam_id64, ip, ping, server_port, current_map)
            VALUES ($1, '悬挂玩家', '76561198000000000', '1.2.3.4', 40, 27015, 'kz_slumpfrageous')
            "#,
        )
        .bind(server_id)
        .execute(&db.pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn poll_refreshes_hibernating_server() {
        let config = Config::from_env();
        let base_url = config.database_url.clone();
        let schema = format!("test_{}", Uuid::new_v4().simple());
        let scoped_url = schema_url(&base_url, &schema);
        test_util::create_schema(&base_url, &schema).await;

        let result = async {
            let db = Database::connect_for_test(&scoped_url).await?;
            db.migrate().await?;

            let address = spawn_fake_rcon_server(HIBERNATING_STATUS, EMPTY_STATS);
            let server_id = insert_hibernating_server(&db, &address, 200).await;
            insert_active_session(&db, server_id).await;

            let mut poller = HibernationPoller::new(db.clone(), test_config());
            let polled = poller.poll_once().await?;
            assert_eq!(polled, 1, "应当轮询到 1 台休眠服务器");

            // 服务器汇总已刷新：探测到空服休眠，状态应如实标记为休眠
            let (status, last_reported_at, players, max_players): (String, DateTime<Utc>, Vec<String>, i32) =
                sqlx::query_as("SELECT status, last_reported_at, players, max_players FROM servers WHERE id = $1")
                    .bind(server_id)
                    .fetch_one(&db.pool)
                    .await?;
            assert_eq!(status, "hibernating");
            assert!(Utc::now().signed_duration_since(last_reported_at).num_seconds() < 10);
            assert!(players.is_empty(), "空服玩家列表应为空");
            assert_eq!(max_players, 16);

            // 状态历史已写入，标记来源为 rcon
            let (fps, cpu, players_count, current_map, source): (f32, f32, i32, String, String) =
                sqlx::query_as(
                    r#"SELECT fps, cpu_usage, players_count, current_map, source
                       FROM server_status_history WHERE server_id = $1 ORDER BY reported_at DESC LIMIT 1"#,
                )
                .bind(server_id)
                .fetch_one(&db.pool)
                .await?;
            assert_eq!(source, "rcon");
            assert_eq!(players_count, 0);
            assert_eq!(current_map, "kz_slumpfrageous");
            assert_eq!(cpu, 0.0);
            assert_eq!(fps, 0.0);

            // 在线玩家详情已清空
            let online_count: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM server_online_players WHERE server_id = $1",
            )
            .bind(server_id)
            .fetch_one(&db.pool)
            .await?;
            assert_eq!(online_count, 0);

            // 残留活跃会话已关闭
            let (left_at, end_reason): (Option<DateTime<Utc>>, String) = sqlx::query_as(
                r#"SELECT left_at, end_reason FROM player_server_sessions WHERE server_id = $1"#,
            )
            .bind(server_id)
            .fetch_one(&db.pool)
            .await?;
            assert!(left_at.is_some(), "悬挂会话应被关闭");
            assert_eq!(end_reason, "server_empty");

            Ok::<(), anyhow::Error>(())
        }
        .await;

        test_util::drop_schema(&base_url, &schema).await;
        result.unwrap();
    }

    #[tokio::test]
    async fn poll_keeps_active_server_online() {
        let config = Config::from_env();
        let base_url = config.database_url.clone();
        let schema = format!("test_{}", Uuid::new_v4().simple());
        let scoped_url = schema_url(&base_url, &schema);
        test_util::create_schema(&base_url, &schema).await;

        let result = async {
            let db = Database::connect_for_test(&scoped_url).await?;
            db.migrate().await?;

            // 有玩家在线的服务器（RCON 可通、非休眠标记）：兜底轮询后仍应保持在线
            let address = spawn_fake_rcon_server(ACTIVE_STATUS, ACTIVE_STATS);
            let server_id = insert_hibernating_server(&db, &address, 200).await;

            let mut poller = HibernationPoller::new(db.clone(), test_config());
            let polled = poller.poll_once().await?;
            assert_eq!(polled, 1, "应当轮询到 1 台上报过期的服务器");

            let (status, players): (String, Vec<String>) = sqlx::query_as(
                "SELECT status, players FROM servers WHERE id = $1",
            )
            .bind(server_id)
            .fetch_one(&db.pool)
            .await?;
            assert_eq!(status, "online", "有玩家在线时状态应保持在线");
            assert!(players.contains(&".mONESY".to_string()));
            assert!(players.contains(&"灵活的小舌头".to_string()));

            Ok::<(), anyhow::Error>(())
        }
        .await;

        test_util::drop_schema(&base_url, &schema).await;
        result.unwrap();
    }

    #[tokio::test]
    async fn poll_skips_fresh_servers() {
        let config = Config::from_env();
        let base_url = config.database_url.clone();
        let schema = format!("test_{}", Uuid::new_v4().simple());
        let scoped_url = schema_url(&base_url, &schema);
        test_util::create_schema(&base_url, &schema).await;

        let result = async {
            let db = Database::connect_for_test(&scoped_url).await?;
            db.migrate().await?;

            // 服务器 10 秒前刚上报过（插件正常运行中），不应触发兜底轮询
            insert_hibernating_server(&db, "127.0.0.1:1", 10).await;

            let mut poller = HibernationPoller::new(db, test_config());
            let polled = poller.poll_once().await?;
            assert_eq!(polled, 0, "上报新鲜的服务器不应被轮询");
            Ok::<(), anyhow::Error>(())
        }
        .await;

        test_util::drop_schema(&base_url, &schema).await;
        result.unwrap();
    }

    #[tokio::test]
    async fn poll_skips_servers_without_rcon_credentials() {
        let config = Config::from_env();
        let base_url = config.database_url.clone();
        let schema = format!("test_{}", Uuid::new_v4().simple());
        let scoped_url = schema_url(&base_url, &schema);
        test_util::create_schema(&base_url, &schema).await;

        let result = async {
            let db = Database::connect_for_test(&scoped_url).await?;
            db.migrate().await?;

            let community_id = Uuid::new_v4();
            sqlx::query(r#"INSERT INTO communities (id, name) VALUES ($1, $2)"#)
                .bind(community_id)
                .bind("测试社区")
                .execute(&db.pool)
                .await?;
            // 未配置 RCON 密码的过期服务器：应被过滤，不做兜底
            sqlx::query(
                r#"
                INSERT INTO servers (id, community_id, name, ip, port, rcon_password, status, last_reported_at, report_token)
                VALUES ($1, $2, '无RCON服', '127.0.0.1', 27015, '', 'online', now() - INTERVAL '200 seconds', $3)
                "#,
            )
            .bind(Uuid::new_v4())
            .bind(community_id)
            .bind(Uuid::new_v4().to_string())
            .execute(&db.pool)
            .await?;

            let mut poller = HibernationPoller::new(db, test_config());
            let polled = poller.poll_once().await?;
            assert_eq!(polled, 0);
            Ok::<(), anyhow::Error>(())
        }
        .await;

        test_util::drop_schema(&base_url, &schema).await;
        result.unwrap();
    }

    #[tokio::test]
    async fn poll_failure_does_not_touch_db() {
        let config = Config::from_env();
        let base_url = config.database_url.clone();
        let schema = format!("test_{}", Uuid::new_v4().simple());
        let scoped_url = schema_url(&base_url, &schema);
        test_util::create_schema(&base_url, &schema).await;

        let result = async {
            let db = Database::connect_for_test(&scoped_url).await?;
            db.migrate().await?;

            // 无人监听的端口：RCON 连接必然失败，模拟服务器真正离线
            let server_id = insert_hibernating_server(&db, "127.0.0.1:1", 200).await;

            let mut poller = HibernationPoller::new(db.clone(), test_config());
            let polled = poller.poll_once().await?;
            assert_eq!(polled, 0, "RCON 失败不应计入成功");

            // 数据库不应被改动：last_reported_at 仍过期，状态历史无 rcon 记录
            let (last_reported_at,): (DateTime<Utc>,) =
                sqlx::query_as("SELECT last_reported_at FROM servers WHERE id = $1")
                    .bind(server_id)
                    .fetch_one(&db.pool)
                    .await?;
            assert!(Utc::now().signed_duration_since(last_reported_at).num_seconds() > 100);

            let rcon_rows: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM server_status_history WHERE server_id = $1 AND source = 'rcon'",
            )
            .bind(server_id)
            .fetch_one(&db.pool)
            .await?;
            assert_eq!(rcon_rows, 0);

            // 失败冷却生效：紧接的一轮不会重试（连接超时 3 秒，若重试会明显变慢）
            let started = Instant::now();
            let polled_again = poller.poll_once().await?;
            assert_eq!(polled_again, 0);
            assert!(
                started.elapsed() < Duration::from_secs(2),
                "失败冷却期间不应重试连接"
            );

            Ok::<(), anyhow::Error>(())
        }
        .await;

        test_util::drop_schema(&base_url, &schema).await;
        result.unwrap();
    }
}

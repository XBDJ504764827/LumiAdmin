use crate::db::Database;
use crate::rcon::StatusResult;
use crate::services::external_server_service::{self, ExternalServer};
use chrono::Utc;
use futures::StreamExt;
use std::time::Duration;

const CACHE_TTL_SECS: u64 = 60;

/// 启动轮询循环。base_interval_secs 为基础扫描间隔（建议 5 秒），
/// 每个服务器的实际查询频率由其自身的 poll_interval 决定。
/// 服务器列表缓存 60 秒，避免高频查库。
pub fn start_rcon_poll_loop(db: Database, base_interval_secs: u64) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(base_interval_secs));
        let mut cached_servers: Vec<ExternalServer> = Vec::new();
        let mut last_cache_refresh =
            std::time::Instant::now() - Duration::from_secs(CACHE_TTL_SECS + 1); // 首次立即加载

        loop {
            interval.tick().await;

            // 缓存过期或为空时刷新
            if last_cache_refresh.elapsed().as_secs() >= CACHE_TTL_SECS {
                match external_server_service::list_enabled_servers(&db).await {
                    Ok(servers) => {
                        cached_servers = servers;
                        last_cache_refresh = std::time::Instant::now();
                    }
                    Err(error) => {
                        tracing::warn!(%error, "刷新服务器缓存失败");
                    }
                }
            }

            if cached_servers.is_empty() {
                continue;
            }

            if let Err(error) = poll_once(&db, &cached_servers).await {
                tracing::warn!(%error, "RCON poll cycle failed");
            }
        }
    });
}

async fn poll_once(db: &Database, servers: &[ExternalServer]) -> anyhow::Result<()> {
    let now = Utc::now();

    // 先筛选出需要查询的服务器
    let to_query: Vec<ExternalServer> = servers
        .iter()
        .filter(|server| {
            let elapsed = server
                .last_queried_at
                .map(|t| (now - t).num_seconds())
                .unwrap_or(i64::MAX);
            elapsed >= server.poll_interval as i64
        })
        .cloned()
        .collect();

    if to_query.is_empty() {
        return Ok(());
    }

    let futures = to_query.into_iter().map(|server| {
        let db = db.clone();
        async move {
            let address = format!("{}:{}", server.ip, server.port);

            let status = match crate::a2s::query_server(&address, 5) {
                Ok(info) => {
                    tracing::debug!(server = %server.name, players = info.player_count, map = %info.current_map, "A2S poll success");
                    StatusResult {
                        server_name: info.server_name,
                        current_map: info.current_map,
                        player_count: info.player_count,
                        max_players: info.max_players,
                        players: info.players.iter().map(|p| p.name.clone()).collect(),
                    }
                }
                Err(a2s_err) => {
                    tracing::debug!(server = %server.name, %a2s_err, "A2S query failed");

                    if let Some(ref pw) = server.rcon_password {
                        match crate::rcon::RconConnection::connect(&address, pw, 5).await {
                            Ok(mut conn) => {
                                match conn.execute("status").await {
                                    Ok(output) => {
                                        tracing::debug!(server = %server.name, "RCON poll success");
                                        crate::rcon::parse_status_output(&output)
                                    }
                                    Err(error) => {
                                        tracing::warn!(server = %server.name, %error, "RCON status command failed");
                                        return;
                                    }
                                }
                            }
                            Err(error) => {
                                tracing::warn!(server = %server.name, %error, "RCON connect failed");
                                return;
                            }
                        }
                    } else {
                        tracing::warn!(server = %server.name, %a2s_err, "A2S failed, no RCON password configured");
                        return;
                    }
                }
            };

            let _ = external_server_service::upsert_status(&db, server.id, &status).await;
            let _ = external_server_service::update_last_queried(&db, server.id).await;
        }
    });

    futures::stream::iter(futures)
        .buffer_unordered(5)
        .collect::<Vec<()>>()
        .await;

    Ok(())
}

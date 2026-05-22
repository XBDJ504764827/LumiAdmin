use crate::db::Database;
use crate::rcon::StatusResult;
use crate::services::external_server_service;
use chrono::Utc;
use futures::StreamExt;
use std::time::Duration;

/// 启动轮询循环。base_interval_secs 为基础扫描间隔（建议 5 秒），
/// 每个服务器的实际查询频率由其自身的 poll_interval 决定。
pub fn start_rcon_poll_loop(db: Database, base_interval_secs: u64) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(base_interval_secs));
        loop {
            interval.tick().await;
            if let Err(error) = poll_once(&db).await {
                tracing::warn!(%error, "RCON poll cycle failed");
            }
        }
    });
}

async fn poll_once(db: &Database) -> anyhow::Result<()> {
    let servers = external_server_service::list_enabled_servers(db).await?;
    if servers.is_empty() {
        return Ok(());
    }

    let now = Utc::now();
    let futures = servers.into_iter().filter_map(|server| {
        let elapsed = server.last_queried_at
            .map(|t| (now - t).num_seconds())
            .unwrap_or(i64::MAX);
        if elapsed < server.poll_interval as i64 {
            return None;
        }

        let db = db.clone();
        Some(async move {
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
        })
    });

    futures::stream::iter(futures)
        .buffer_unordered(5)
        .collect::<Vec<()>>()
        .await;

    Ok(())
}

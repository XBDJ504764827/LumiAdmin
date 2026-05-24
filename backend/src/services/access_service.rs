use crate::{
    config::Config,
    db::Database,
    http_client,
    services::{access_snapshot_service, player_access_rule_service, plugin_ban_service, server_config_cache},
};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::warn;

#[derive(Debug, Clone)]
pub struct AccessCheckInput {
    pub report_token: String,
    pub port: i32,
    pub steam_id64: String,
    pub ip_address: Option<String>,
    pub player: Option<String>,
    pub server_port: Option<i32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AccessCheckResult {
    pub allowed: bool,
    pub message: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct PlayerAccessCacheRow {
    rating: i32,
    steam_level: i32,
    expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
struct PlayerAccessProfile {
    rating: i32,
    steam_level: i32,
}

#[derive(Debug, Clone)]
struct ActiveBanInfo {
    id: uuid::Uuid,
    reason: String,
    expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize)]
struct GokzPlayerResponse {
    rating: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct SteamLevelEnvelope {
    response: SteamLevelResponse,
}

#[derive(Debug, Deserialize)]
struct SteamLevelResponse {
    player_level: Option<i32>,
}

pub async fn check_access(
    db: &Database,
    config: &Config,
    snapshot_store: &access_snapshot_service::SnapshotStore,
    server_cache: &Arc<server_config_cache::ServerConfigCache>,
    input: AccessCheckInput,
) -> anyhow::Result<AccessCheckResult> {
    let steam_id64 = normalize_steamid64(&input.steam_id64)?;
    match check_access_live(db, config, input.clone(), &steam_id64, server_cache).await {
        Ok(result) => Ok(result),
        Err(error) => {
            warn!(%error, "live access check failed, trying snapshot fallback");
            let Some(snapshot) = snapshot_store.read_snapshot().await? else {
                return Ok(reject("访问控制服务暂时不可用，请稍后再试。"));
            };
            let decision = access_snapshot_service::evaluate_access_snapshot(
                &snapshot,
                &access_snapshot_service::SnapshotAccessInput {
                    report_token: input.report_token,
                    port: input.port,
                    steam_id64,
                    ip_address: input.ip_address,
                    now: Utc::now(),
                },
            );
            Ok(access_result_from_snapshot_decision(decision))
        }
    }
}

async fn check_access_live(
    db: &Database,
    config: &Config,
    input: AccessCheckInput,
    steam_id64: &str,
    server_cache: &Arc<server_config_cache::ServerConfigCache>,
) -> anyhow::Result<AccessCheckResult> {
    // 使用缓存获取服务器配置
    let server = server_cache
        .get_by_token_port(db, &input.report_token, input.port)
        .await?
        .ok_or_else(|| anyhow::anyhow!("服务器 Token 或端口无效"))?;

    // 1. 检查封禁状态
    if let Some(ban) = active_ban(db, steam_id64, input.ip_address.as_deref()).await? {
        let server_auth = plugin_ban_service::ServerAuth {
            id: server.id,
            name: server.name.clone(),
            port: server.port,
        };
        plugin_ban_service::complete_missing_ban_details(
            db,
            ban.id,
            input.player.as_deref(),
            input.ip_address.as_deref(),
            &server_auth,
            input.server_port.unwrap_or(server.port),
        )
        .await?;
        let message = match ban.expires_at {
            Some(expires) => format!(
                "你已被封禁，原因：{}，到期时间：{}",
                ban.reason,
                expires.format("%Y-%m-%d %H:%M")
            ),
            None => format!("你已被永久封禁，原因：{}", ban.reason),
        };
        return Ok(reject(&message));
    }

    // 2. 检查玩家进服权限规则（优先级最高）
    let (access_allowed, access_reason) = player_access_rule_service::check_player_access(
        db,
        steam_id64,
        server.id,
        server.community_id,
    )
    .await?;
    if !access_allowed {
        return Ok(reject(access_reason.as_deref().unwrap_or("您被禁止进入该服务器")));
    }

    // 3. 检查服务器访问模式
    let effective_restriction = server.effective_access_restriction_enabled();
    let effective_whitelist = server.effective_whitelist_mode_enabled();

    // 都没开 → 无限制放行
    if !effective_whitelist && !effective_restriction {
        return Ok(allow("允许进入服务器。"));
    }

    let whitelist_approved = has_approved_whitelist(db, steam_id64).await?;

    // 仅白名单模式：必须通过白名单才能进
    if effective_whitelist && !effective_restriction {
        return if whitelist_approved {
            Ok(allow("已通过白名单审核，允许进入服务器。"))
        } else {
            Ok(reject("你尚未通过白名单审核，无法进入服务器。"))
        };
    }

    // 仅进入限制：检查 rating/steam level
    if !effective_whitelist && effective_restriction {
        return match load_player_profile(db, config, steam_id64).await? {
            Some(profile) => evaluate_restriction(&server, &profile),
            None => Ok(reject("无法验证您的进入资格，请稍后再试。")),
        };
    }

    // 两者都开：满足限制即可进，不满足则看白名单
    match load_player_profile(db, config, steam_id64).await? {
        Some(profile) if profile.rating >= server.effective_min_rating()
            && profile.steam_level >= server.effective_min_steam_level() => {
            Ok(allow("已满足服务器进入限制，允许进入服务器。"))
        }
        _ => {
            // 不满足限制，检查白名单
            if whitelist_approved {
                Ok(allow("已通过白名单审核，允许进入服务器。"))
            } else {
                Ok(reject("你的 GOKZ rating 未达到服务器最低要求。"))
            }
        }
    }
}

fn evaluate_restriction(
    server: &server_config_cache::CachedServerConfig,
    profile: &PlayerAccessProfile,
) -> anyhow::Result<AccessCheckResult> {
    let min_rating = server.effective_min_rating();
    let min_steam_level = server.effective_min_steam_level();
    if profile.rating < min_rating {
        return Ok(reject("你的 GOKZ rating 未达到服务器最低要求。"));
    }
    if profile.steam_level < min_steam_level {
        return Ok(reject("你的 Steam 等级未达到服务器最低要求。"));
    }
    Ok(allow("已满足服务器进入限制，允许进入服务器。"))
}

async fn active_ban(
    db: &Database,
    steam_id64: &str,
    ip_address: Option<&str>,
) -> anyhow::Result<Option<ActiveBanInfo>> {
    let row: Option<(uuid::Uuid, String, Option<DateTime<Utc>>)> = sqlx::query_as(
        r#"SELECT id, reason, expires_at
           FROM ban_records
           WHERE status = 'active'
             AND (expires_at IS NULL OR expires_at > now())
             AND (($1::TEXT IS NOT NULL AND steam_id = $1) OR ($2::TEXT IS NOT NULL AND ip_address = $2))
           ORDER BY created_at DESC
           LIMIT 1"#,
    )
    .bind(steam_id64)
    .bind(ip_address)
    .fetch_optional(&db.pool)
    .await?;
    Ok(row.map(|(id, reason, expires_at)| ActiveBanInfo {
        id,
        reason,
        expires_at,
    }))
}

async fn has_approved_whitelist(db: &Database, steam_id64: &str) -> anyhow::Result<bool> {
    let count: (i64,) = sqlx::query_as(
        r#"SELECT COUNT(*)
           FROM whitelist_requests
           WHERE steamid64 = $1 AND status = 'approved'"#,
    )
    .bind(steam_id64)
    .fetch_one(&db.pool)
    .await?;
    Ok(count.0 > 0)
}

async fn load_player_profile(
    db: &Database,
    config: &Config,
    steam_id64: &str,
) -> anyhow::Result<Option<PlayerAccessProfile>> {
    if let Some(cached) = read_cache(db, steam_id64).await? {
        if cached.expires_at > Utc::now() {
            return Ok(Some(PlayerAccessProfile {
                rating: cached.rating,
                steam_level: cached.steam_level,
            }));
        }
    }

    let Some(profile) = fetch_player_profile(config, steam_id64).await? else {
        return Ok(None);
    };
    write_cache(db, steam_id64, &profile).await?;
    Ok(Some(profile))
}

async fn read_cache(
    db: &Database,
    steam_id64: &str,
) -> anyhow::Result<Option<PlayerAccessCacheRow>> {
    Ok(sqlx::query_as::<_, PlayerAccessCacheRow>(
        r#"SELECT rating, steam_level, expires_at
           FROM player_access_cache
           WHERE steamid64 = $1"#,
    )
    .bind(steam_id64)
    .fetch_optional(&db.pool)
    .await?)
}

async fn write_cache(
    db: &Database,
    steam_id64: &str,
    profile: &PlayerAccessProfile,
) -> anyhow::Result<()> {
    sqlx::query(
        r#"INSERT INTO player_access_cache (steamid64, rating, steam_level, expires_at, updated_at)
           VALUES ($1, $2, $3, $4, now())
           ON CONFLICT (steamid64) DO UPDATE
           SET rating = EXCLUDED.rating,
               steam_level = EXCLUDED.steam_level,
               expires_at = EXCLUDED.expires_at,
               updated_at = now()"#,
    )
    .bind(steam_id64)
    .bind(profile.rating)
    .bind(profile.steam_level)
    .bind(Utc::now() + Duration::hours(24))
    .execute(&db.pool)
    .await?;
    Ok(())
}

async fn fetch_player_profile(
    config: &Config,
    steam_id64: &str,
) -> anyhow::Result<Option<PlayerAccessProfile>> {
    let has_level_key = config.steamchina_level_key.is_some() || config.steam_web_key.is_some();
    if !has_level_key {
        warn!(steam_id64, "缺少 Steam API Key，进入限制将放行");
        return Ok(None);
    }

    let gokz_url = format!("https://api.gokz.top/api/v1/players/{steam_id64}");

    let steam_level = fetch_steam_level(config, steam_id64).await;

    let rating = match http_client::http_client().get(&gokz_url).send().await {
        Ok(response) if response.status().is_success() => {
            match response.json::<GokzPlayerResponse>().await {
                Ok(body) => body.rating.map(|rating| rating.trunc() as i32),
                Err(error) => {
                    warn!(steam_id64, error = %error, "GOKZ 玩家资料解析失败，进入限制将放行");
                    None
                }
            }
        }
        Ok(response) => {
            warn!(steam_id64, status = %response.status(), "GOKZ 玩家资料请求失败，进入限制将放行");
            None
        }
        Err(error) => {
            warn!(steam_id64, error = %error, "GOKZ 玩家资料请求异常，进入限制将放行");
            None
        }
    };
    let steam_level = match steam_level {
        Some(level) => Some(level),
        None => {
            warn!(steam_id64, "Steam 等级查询全部失败，进入限制将放行");
            None
        }
    };

    match (rating, steam_level) {
        (Some(rating), Some(steam_level)) => Ok(Some(PlayerAccessProfile {
            rating,
            steam_level,
        })),
        _ => {
            warn!(
                steam_id64,
                rating_found = rating.is_some(),
                steam_level_found = steam_level.is_some(),
                "玩家准入资料不完整，进入限制将降级到快照"
            );
            Ok(None)
        }
    }
}

fn normalize_steamid64(value: &str) -> anyhow::Result<String> {
    let steam_id64 = value.trim();
    anyhow::ensure!(
        steam_id64.len() == 17 && steam_id64.chars().all(|ch| ch.is_ascii_digit()),
        "SteamID64 不合法"
    );
    Ok(steam_id64.to_string())
}

fn access_result_from_snapshot_decision(
    decision: access_snapshot_service::SnapshotAccessDecision,
) -> AccessCheckResult {
    AccessCheckResult {
        allowed: decision.allowed,
        message: decision.message,
    }
}

fn allow(message: &str) -> AccessCheckResult {
    AccessCheckResult {
        allowed: true,
        message: message.to_string(),
    }
}

fn reject(message: &str) -> AccessCheckResult {
    AccessCheckResult {
        allowed: false,
        message: message.to_string(),
    }
}

/// 查询 Steam 等级：优先 steamchina，失败用 steampowered
async fn fetch_steam_level(config: &Config, steam_id64: &str) -> Option<i32> {
    // 主：steamchina
    if let Some(ref china_key) = config.steamchina_level_key {
        let url = format!(
            "https://api.steamchina.com/IPlayerService/GetSteamLevel/v0001/?key={china_key}&steamid={steam_id64}"
        );
        match http_client::http_client().get(&url).send().await {
            Ok(response) if response.status().is_success() => {
                match response.json::<SteamLevelEnvelope>().await {
                    Ok(body) => {
                        if body.response.player_level.is_some() {
                            return body.response.player_level;
                        }
                    }
                    Err(error) => {
                        warn!(steam_id64, error = %error, "steamchina 等级解析失败，尝试备用");
                    }
                }
            }
            Ok(response) => {
                warn!(steam_id64, status = %response.status(), "steamchina 等级请求失败，尝试备用");
            }
            Err(error) => {
                warn!(steam_id64, error = %error, "steamchina 等级请求异常，尝试备用");
            }
        }
    }

    // 备：steampowered
    if let Some(steam_web_key) = config.steam_web_key.as_deref() {
        let url = format!(
            "https://api.steampowered.com/IPlayerService/GetSteamLevel/v1/?key={steam_web_key}&steamid={steam_id64}"
        );
        match http_client::http_client().get(&url).send().await {
            Ok(response) if response.status().is_success() => {
                match response.json::<SteamLevelEnvelope>().await {
                    Ok(body) => {
                        return body.response.player_level;
                    }
                    Err(error) => {
                        warn!(steam_id64, error = %error, "steampowered 等级解析失败");
                    }
                }
            }
            Ok(response) => {
                warn!(steam_id64, status = %response.status(), "steampowered 等级请求失败");
            }
            Err(error) => {
                warn!(steam_id64, error = %error, "steampowered 等级请求异常");
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server(min_rating: i32, min_steam_level: i32) -> server_config_cache::CachedServerConfig {
        server_config_cache::CachedServerConfig {
            id: uuid::Uuid::nil(),
            community_id: uuid::Uuid::nil(),
            name: "测试服".to_string(),
            port: 27015,
            report_token: "test_token".to_string(),
            access_restriction_enabled: true,
            min_rating,
            min_steam_level,
            whitelist_mode_enabled: false,
            use_custom_access: true,
            community_whitelist_mode_enabled: false,
            community_min_rating: 0,
            community_min_steam_level: 0,
        }
    }

    #[test]
    fn evaluate_restriction_allows_when_rating_and_level_match() {
        let result = evaluate_restriction(
            &server(1200, 10),
            &PlayerAccessProfile {
                rating: 1200,
                steam_level: 10,
            },
        )
        .unwrap();
        assert!(result.allowed);
        assert_eq!(result.message, "已满足服务器进入限制，允许进入服务器。");
    }

    #[test]
    fn evaluate_restriction_rejects_low_rating_first() {
        let result = evaluate_restriction(
            &server(1200, 10),
            &PlayerAccessProfile {
                rating: 1199,
                steam_level: 99,
            },
        )
        .unwrap();
        assert!(!result.allowed);
        assert_eq!(result.message, "你的 GOKZ rating 未达到服务器最低要求。");
    }

    #[test]
    fn evaluate_restriction_rejects_low_steam_level() {
        let result = evaluate_restriction(
            &server(1200, 10),
            &PlayerAccessProfile {
                rating: 1200,
                steam_level: 9,
            },
        )
        .unwrap();
        assert!(!result.allowed);
        assert_eq!(result.message, "你的 Steam 等级未达到服务器最低要求。");
    }

    #[test]
    fn gokz_player_response_accepts_decimal_rating() {
        let response: GokzPlayerResponse = serde_json::from_str(r#"{"rating":8.352655}"#).unwrap();
        assert_eq!(response.rating.map(|rating| rating.trunc() as i32), Some(8));
    }

    #[test]
    fn snapshot_decision_maps_to_access_check_result() {
        let result = access_result_from_snapshot_decision(
            crate::services::access_snapshot_service::SnapshotAccessDecision {
                allowed: false,
                message: "你的白名单状态无法确认，请稍后再试。".to_string(),
            },
        );

        assert!(!result.allowed);
        assert_eq!(result.message, "你的白名单状态无法确认，请稍后再试。");
    }
}

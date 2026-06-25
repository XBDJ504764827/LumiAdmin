use crate::{
    config::Config,
    db::Database,
    http_client,
    services::{
        access_cache::{ActiveBanCache, WhitelistCache},
        access_snapshot_service, player_access_rule_service, player_risk_service,
        plugin_ban_service, server_config_cache,
    },
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
    /// 进服方式
    pub access_method: Option<String>,
    /// 失败原因代码（进服失败时用于结构化筛选/统计）
    pub failure_code: Option<String>,
    /// 玩家 GOKZ rating
    pub rating: Option<i32>,
    /// 玩家 Steam 等级
    pub steam_level: Option<i32>,
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
    #[allow(dead_code)]
    reason: String,
    #[allow(dead_code)]
    expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize)]
struct GokzPlayerResponse {
    #[allow(dead_code)]
    #[serde(default, alias = "name", alias = "player_name")]
    steam_name: Option<String>,
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
    ban_cache: &ActiveBanCache,
    wl_cache: &WhitelistCache,
    input: AccessCheckInput,
) -> anyhow::Result<AccessCheckResult> {
    let steam_id64 = normalize_steamid64(&input.steam_id64)?;
    match check_access_live(
        db,
        config,
        input.clone(),
        &steam_id64,
        server_cache,
        ban_cache,
        wl_cache,
    )
    .await
    {
        Ok(result) => Ok(result),
        Err(error) => {
            warn!(%error, "live access check failed, trying snapshot fallback");
            let Some(snapshot) = snapshot_store.read_snapshot().await? else {
                return Ok(reject_with_method(
                    "访问控制服务暂时不可用，请稍后再试。",
                    "snapshot_fallback",
                    "snapshot_unavailable",
                ));
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
    ban_cache: &ActiveBanCache,
    wl_cache: &WhitelistCache,
) -> anyhow::Result<AccessCheckResult> {
    // 使用缓存获取服务器配置
    let server = server_cache
        .get_by_token_port(db, &input.report_token, input.port)
        .await?
        .ok_or_else(|| anyhow::anyhow!("服务器 Token 或端口无效"))?;

    // 1. 检查封禁状态（优先使用缓存）
    let ban_info = {
        let cached = ban_cache.get_by_steam_id(steam_id64).await;
        let cached = match cached {
            Some(ban) => Some(ban),
            None => match input.ip_address.as_deref() {
                Some(ip) => ban_cache.get_by_ip(ip).await,
                None => None,
            },
        };
        match cached {
            Some(ban) => Some(ActiveBanInfo {
                id: ban.id,
                reason: ban.reason,
                expires_at: ban.expires_at,
            }),
            None => active_ban(db, steam_id64, input.ip_address.as_deref()).await?,
        }
    };
    if let Some(ban) = ban_info {
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
        return Ok(reject_with_method(
            "你已被该服务器封禁。\n如有异议可前往以下地址进行申诉。\n申诉地址:https://zzzxbdjbans.cngokz.com/public/ban-appeal",
            "banned",
            "banned",
        ));
    }

    if let Some(reason) = active_global_ban(db, steam_id64).await? {
        return Ok(reject_with_method(
            &format!(
                "你已被全球封禁，无法进入服务器。\n原因：{reason}\n如有异议请先处理全球封禁记录。"
            ),
            "banned",
            "global_banned",
        ));
    }

    if let Some(ip_risk) =
        player_risk_service::evaluate_ip_ban_for_access(db, steam_id64, input.ip_address.as_deref())
            .await?
    {
        return Ok(reject_with_method(
            &format!(
                "当前 IP 存在高风险关联，无法进入服务器。\n原因：{}",
                ip_risk.message
            ),
            "banned",
            "linked_ip_banned",
        ));
    }

    // 2. 检查玩家进服权限规则（优先级最高）
    let (access_allowed, access_reason, has_custom_rule) =
        player_access_rule_service::check_player_access(
            db,
            steam_id64,
            server.id,
            server.community_id,
        )
        .await?;
    if !access_allowed {
        return Ok(reject_with_method(
            access_reason.as_deref().unwrap_or("您被禁止进入该服务器"),
            "custom_rule_rejected",
            "custom_rule_rejected",
        ));
    }
    // 如果自定义规则明确放行，记录为 custom_rule（直接跳过后续白名单/限制检查）
    if has_custom_rule {
        return Ok(allow_with_data(
            "已通过自定义权限规则，允许进入服务器。",
            "custom_rule",
            None,
            None,
        ));
    }

    // 3. 检查服务器访问模式
    let effective_restriction = server.effective_access_restriction_enabled();
    let effective_whitelist = server.effective_whitelist_mode_enabled();

    // 都没开 → 无限制放行
    if !effective_whitelist && !effective_restriction {
        return Ok(allow_with_data(
            "允许进入服务器。",
            "unrestricted",
            None,
            None,
        ));
    }

    let whitelist_approved = wl_cache.contains(steam_id64).await;

    // 仅白名单模式：必须通过白名单才能进
    if effective_whitelist && !effective_restriction {
        return if whitelist_approved {
            Ok(allow_with_data(
                "已通过白名单审核，允许进入服务器。",
                "whitelist",
                None,
                None,
            ))
        } else {
            Ok(reject_with_method("当前服务器开启了白名单模式。\n您可以通过申请白名单获取进入服务器资格。\n申请地址:https://zzzxbdjbans.cngokz.com/public/apply", "whitelist_rejected", "not_whitelisted"))
        };
    }

    // 仅进入限制：检查 rating/steam level
    if !effective_whitelist && effective_restriction {
        return match load_player_profile(db, config, steam_id64).await? {
            Some(profile) => evaluate_restriction(&server, &profile),
            None => Ok(reject_with_method(
                "无法验证您的进入资格，请稍后再试。",
                "restriction_rejected",
                "profile_fetch_failed",
            )),
        };
    }

    // 两者都开：满足限制即可进，不满足则看白名单
    match load_player_profile(db, config, steam_id64).await? {
        Some(profile)
            if profile.rating >= server.effective_min_rating()
                && profile.steam_level >= server.effective_min_steam_level() =>
        {
            Ok(allow_with_data(
                "已满足服务器进入限制，允许进入服务器。",
                "restriction",
                Some(profile.rating),
                Some(profile.steam_level),
            ))
        }
        _ => {
            // 不满足限制，检查白名单
            if whitelist_approved {
                Ok(allow_with_data(
                    "已通过白名单审核，允许进入服务器。",
                    "whitelist",
                    None,
                    None,
                ))
            } else {
                Ok(reject_with_method("你的GOKZ rating未达到进入服务器最低要求。\n您可以通过申请白名单获取进入服务器资格。\n申请地址:https://zzzxbdjbans.cngokz.com/public/apply", "restriction_rejected", "low_rating"))
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
        return Ok(reject_with_method("你的GOKZ rating未达到进入服务器最低要求。\n您可以通过申请白名单获取进入服务器资格。\n申请地址:https://zzzxbdjbans.cngokz.com/public/apply", "restriction_rejected", "low_rating"));
    }
    if profile.steam_level < min_steam_level {
        return Ok(reject_with_method("你的steam等级未达到进入服务器最低要求。\n您可以通过申请白名单获取进入服务器资格。\n申请地址:https://zzzxbdjbans.cngokz.com/public/apply", "restriction_rejected", "low_steam_level"));
    }
    Ok(allow_with_data(
        "已满足服务器进入限制，允许进入服务器。",
        "restriction",
        Some(profile.rating),
        Some(profile.steam_level),
    ))
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

async fn active_global_ban(db: &Database, steam_id64: &str) -> anyhow::Result<Option<String>> {
    let row: Option<(String, Option<String>)> = sqlx::query_as(
        r#"SELECT ban_type, notes
           FROM global_bans
           WHERE steam_id64 = $1
             AND is_expired = false
             AND manual_unbanned = false
           ORDER BY created_on DESC NULLS LAST, synced_at DESC
           LIMIT 1"#,
    )
    .bind(steam_id64)
    .fetch_optional(&db.pool)
    .await?;
    Ok(row.map(|(ban_type, notes)| match notes {
        Some(notes) if !notes.trim().is_empty() => format!("{ban_type} / {notes}"),
        _ => ban_type,
    }))
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

    let gokz_url = format!("https://api.gokz.top/v1/leaderboards/players/{steam_id64}");

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
        access_method: Some("snapshot_fallback".to_string()),
        failure_code: None,
        rating: None,
        steam_level: None,
    }
}

pub(crate) fn allow_with_data(
    message: &str,
    access_method: &str,
    rating: Option<i32>,
    steam_level: Option<i32>,
) -> AccessCheckResult {
    AccessCheckResult {
        allowed: true,
        message: message.to_string(),
        access_method: Some(access_method.to_string()),
        failure_code: None,
        rating,
        steam_level,
    }
}

fn reject_with_method(message: &str, access_method: &str, failure_code: &str) -> AccessCheckResult {
    AccessCheckResult {
        allowed: false,
        message: message.to_string(),
        access_method: Some(access_method.to_string()),
        failure_code: Some(failure_code.to_string()),
        rating: None,
        steam_level: None,
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
        assert!(result
            .message
            .contains("你的GOKZ rating未达到进入服务器最低要求"));
        assert!(result
            .message
            .contains("申请地址:https://zzzxbdjbans.cngokz.com/public/apply"));
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
        assert!(result
            .message
            .contains("你的steam等级未达到进入服务器最低要求"));
        assert!(result
            .message
            .contains("申请地址:https://zzzxbdjbans.cngokz.com/public/apply"));
    }

    #[test]
    fn gokz_player_response_accepts_decimal_rating() {
        let response: GokzPlayerResponse =
            serde_json::from_str(r#"{"steam_name":"PlayerOne","rating":8.352655}"#).unwrap();
        assert_eq!(response.steam_name.as_deref(), Some("PlayerOne"));
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

use crate::{
    config::Config,
    db::Database,
    http_client,
    services::{
        access_cache::{ActiveBanCache, WhitelistCache},
        access_snapshot_service,
        gokz_cache::GokzCacheManager,
        player_risk_service, plugin_ban_service, server_config_cache,
    },
};
use chrono::{DateTime, Duration, Utc};
use futures::future::join_all;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration as StdDuration;
use tokio::time::timeout;
use tracing::warn;

const GOKZ_RATING_SCOPES: [&str; 4] = ["KZT", "SKZ", "VNL", "OVR"];
pub(crate) const ACCESS_RATING_SOURCE: &str = "scoped_max";
/// 中高风险账号（存在封禁类风险信号）且没有白名单时的进服提示。
pub(crate) const RISK_BLOCK_MESSAGE: &str = "您的账号可能有些问题，本次进入服务器被阻止\n您可以进行申请白名单后再尝试进入\n如有疑问加入Q群275164688寻求帮助";
static GOKZ_NEGATIVE_CACHE: OnceLock<Mutex<HashMap<String, std::time::Instant>>> = OnceLock::new();

#[derive(Debug, Clone)]
pub struct AccessCheckInput {
    pub report_token: String,
    pub port: i32,
    pub steam_id64: String,
    pub ip_address: Option<String>,
    pub player: Option<String>,
    pub server_port: Option<i32>,
    /// 游戏插件上报的 CS 优先账户状态；`None` 表示插件暂未确认
    pub is_cs_prime: Option<bool>,
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
    /// 仅用于后台进服日志的完整审计原因，不返回给游戏插件。
    #[serde(skip_serializing)]
    pub audit_message: Option<String>,
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

#[allow(clippy::too_many_arguments)]
pub async fn check_access(
    db: &Database,
    config: &Config,
    snapshot_store: &access_snapshot_service::SnapshotStore,
    server_cache: &Arc<server_config_cache::ServerConfigCache>,
    ban_cache: &ActiveBanCache,
    wl_cache: &WhitelistCache,
    gokz_cache: &GokzCacheManager,
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
        gokz_cache,
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
                    is_cs_prime: input.is_cs_prime,
                    now: Utc::now(),
                },
            );
            Ok(access_result_from_snapshot_decision(decision))
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn check_access_live(
    db: &Database,
    config: &Config,
    input: AccessCheckInput,
    steam_id64: &str,
    server_cache: &Arc<server_config_cache::ServerConfigCache>,
    ban_cache: &ActiveBanCache,
    wl_cache: &WhitelistCache,
    gokz_cache: &GokzCacheManager,
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
            "你已被该服务器封禁。\n如有异议可前往社区论坛进行申诉。",
            "banned",
            "banned",
        ));
    }

    // 2. 检查服务器访问模式（开启的模式之间为 OR：满足任意一种即可进入）
    let effective_restriction = server.effective_access_restriction_enabled();
    let effective_whitelist = server.effective_whitelist_mode_enabled();
    let effective_cs_prime = server.effective_cs_prime_enabled();
    let risk_block_enabled = server.risk_block_enabled;

    // 白名单状态：白名单模式需要它判定准入；中高风险拦截把它作为唯一的豁免条件，
    // 因此只在真正需要时才查询缓存。
    let whitelist_approved = if effective_whitelist || risk_block_enabled {
        wl_cache.contains(steam_id64).await
    } else {
        false
    };

    // 2.1 中高风险账号拦截：账号存在封禁类风险信号（自身有效封禁、同 IP 关联账号
    // 有效封禁）时为中/高风险，需持有白名单才可进入。该开关独立于上方进服模式，
    // 因此必须放在「无限制放行」与「CS 优先账户放行」之前。
    if risk_block_enabled && !whitelist_approved {
        if let Some(risk) = player_risk_service::evaluate_ban_risk_for_access(
            db,
            steam_id64,
            input.ip_address.as_deref(),
        )
        .await?
        {
            if risk.is_medium_or_high() {
                return Ok(reject_access_risk(&risk));
            }
        }
    }

    // 都没开 → 无限制放行
    if !effective_whitelist && !effective_restriction && !effective_cs_prime {
        return Ok(allow_with_data(
            "允许进入服务器。",
            "unrestricted",
            None,
            None,
        ));
    }

    // CS 优先账户：由游戏插件通过 Steam GameServer API 查询后上报
    // 优先账户检查必须在进入限制之前执行，确保优先账号直接放行
    if effective_cs_prime {
        match input.is_cs_prime {
            Some(true) => {
                return Ok(allow_with_data(
                    "已确认 CS 优先账户，允许进入服务器。",
                    "cs_prime",
                    None,
                    None,
                ));
            }
            Some(false) => {
                // 非优先账号，继续后续检查（可能还有白名单/进入限制）
            }
            None => {
                // 插件未上报时，保守处理：不直接拒绝，继续后续检查
            }
        }
    }

    // 进入限制（rating / steam level）

    let mut restriction_failed = false;
    let mut restriction_failure_code: Option<String> = None;
    if effective_restriction {
        match load_player_profile(db, config, steam_id64, gokz_cache).await? {
            Some(profile) => {
                let result = evaluate_restriction(&server, &profile)?;
                if result.allowed {
                    return Ok(result);
                }

                restriction_failed = true;
                restriction_failure_code = result.failure_code.as_deref().map(|code| match code {
                    "low_rating" => "low_rating".to_string(),
                    "low_steam_level" => "low_steam_level".to_string(),
                    other => other.to_string(),
                });
            }
            None => {
                // 仅开启进入限制且资料拉取失败时，给出可重试提示；组合模式下仍按组合拒绝文案处理。
                if !effective_whitelist && !effective_cs_prime {
                    return Ok(reject_with_method(
                        "无法验证您的进入资格，请稍后再试。",
                        "restriction_rejected",
                        "profile_fetch_failed",
                    ));
                }
                restriction_failed = true;
                restriction_failure_code = Some("profile_fetch_failed".to_string());
            }
        }
    }

    // 白名单
    if effective_whitelist && whitelist_approved {
        return Ok(allow_with_data(
            "已通过白名单审核，允许进入服务器。",
            "whitelist",
            None,
            None,
        ));
    }

    // 均未满足：按启用模式组合返回拒绝原因
    let whitelist_failed = effective_whitelist && !whitelist_approved;
    let cs_prime_failed = effective_cs_prime && input.is_cs_prime != Some(true);
    let cs_prime_failure_code = cs_prime_failed.then(|| match input.is_cs_prime {
        Some(false) => "not_cs_prime".to_string(),
        _ => "prime_verification_failed".to_string(),
    });
    Ok(reject_access_modes(
        whitelist_failed,
        restriction_failed,
        cs_prime_failed,
        restriction_failure_code,
        cs_prime_failure_code,
    ))
}

/// 中高风险账号（封禁类风险信号）无白名单时的拒绝结果。
///
/// 玩家侧只看到统一提示；命中明细写入 `audit_message`，仅进服日志可见。
fn reject_access_risk(risk: &player_risk_service::AccessBanRisk) -> AccessCheckResult {
    let audit_message = format!("账号风险拦截（{}）：{}", risk.action.label(), risk.detail);
    let mut result =
        reject_with_method(RISK_BLOCK_MESSAGE, "risk_blocked", risk_failure_code(risk));
    result.audit_message = Some(audit_message);
    result
}

/// 失败原因代码：纯「同 IP 关联封禁」保留既有代码，便于沿用历史筛选口径；
/// 涉及账号自身封禁信号时归入通用的中高风险拦截。
fn risk_failure_code(risk: &player_risk_service::AccessBanRisk) -> &'static str {
    if risk.codes.iter().all(|code| code.starts_with("linked_ip_")) {
        "linked_ip_banned"
    } else {
        "risk_blocked"
    }
}

fn evaluate_restriction(
    server: &server_config_cache::CachedServerConfig,
    profile: &PlayerAccessProfile,
) -> anyhow::Result<AccessCheckResult> {
    let min_rating = server.effective_min_rating();
    let min_steam_level = server.effective_min_steam_level();
    if profile.rating < min_rating {
        return Ok(reject_with_method(
            "当前服务器开启了进入限制\n您的账号未达到最低进入要求\n如有疑问加入Q群275164688寻求帮助",
            "restriction_rejected",
            "low_rating",
        ));
    }
    if profile.steam_level < min_steam_level {
        return Ok(reject_with_method(
            "当前服务器开启了进入限制\n您的账号未达到最低进入要求\n如有疑问加入Q群275164688寻求帮助",
            "restriction_rejected",
            "low_steam_level",
        ));
    }
    Ok(allow_with_data(
        "已满足服务器进入限制，允许进入服务器。",
        "restriction",
        Some(profile.rating),
        Some(profile.steam_level),
    ))
}

/// 根据启用且未通过的准入模式组合生成阻止提示词。
fn reject_access_modes(
    whitelist_failed: bool,
    restriction_failed: bool,
    cs_prime_failed: bool,
    restriction_failure_code: Option<String>,
    cs_prime_failure_code: Option<String>,
) -> AccessCheckResult {
    if cs_prime_failure_code.as_deref() == Some("prime_verification_failed") {
        let mut audit_reasons = Vec::new();
        if whitelist_failed {
            audit_reasons.push("白名单未通过");
        }
        if restriction_failed {
            audit_reasons.push(match restriction_failure_code.as_deref() {
                Some("low_rating") => "Rating 未达标",
                Some("low_steam_level") => "Steam 等级未达标",
                Some("profile_fetch_failed") => "Rating/Steam 等级资料获取失败",
                _ => "进入限制未通过",
            });
        }
        audit_reasons.push("CS 优先账户状态无法验证");

        let audit_message = format!("准入条件未满足：{}", audit_reasons.join("；"));
        let player_message = format!("{audit_message}\n请稍后再试。");
        let mut result = reject_with_method(
            &player_message,
            "cs_prime_rejected",
            "prime_verification_failed",
        );
        result.audit_message = Some(audit_message);
        return result;
    }

    let message = match (whitelist_failed, restriction_failed, cs_prime_failed) {
        (true, false, false) => {
            "当前服务器开启了白名单验证\n请前往以下地址进行申请\nhttps://zzzxbdjbans.cngokz.com/public/apply\n如有疑问加入Q群275164688寻求帮助"
        }
        (false, true, false) => {
            "当前服务器开启了进入限制\n您的账号未达到最低进入要求\n如有疑问加入Q群275164688寻求帮助"
        }
        (false, false, true) => {
            "您的账号非CS优先账户无法进入服务器\n如有疑问加入Q群275164688寻求帮助"
        }
        (true, true, false) => {
            "您的账号未达到服务器最低进入要求并且没有获取白名单资格\n请前往以下地址获取白名单，如有疑问加入Q群275164688寻求帮助\nhttps://zzzxbdjbans.cngokz.com/public/apply"
        }
        (true, false, true) => {
            "您的账号CS为非优先账号并且没有获取白名单资格\n请前往以下地址获取白名单，如有疑问加入Q群275164688寻求帮助\nhttps://zzzxbdjbans.cngokz.com/public/apply"
        }
        (false, true, true) => {
            "您的账号CS为非优先账号并且未达到最低进入要求被阻止进入服务器\n如有疑问加入Q群275164688寻求帮助"
        }
        (true, true, true) => {
            "您的账号CS为非优先账号并且未达到最低进入要求并且没有获取白名单资格\n请前往以下地址获取资格如有疑问加入Q群275164688寻求帮助\nhttps://zzzxbdjbans.cngokz.com/public/apply"
        }
        (false, false, false) => "您无法进入服务器。",
    };

    let (access_method, failure_code) =
        match (whitelist_failed, restriction_failed, cs_prime_failed) {
            (true, false, false) => ("whitelist_rejected", "not_whitelisted"),
            (false, true, false) => (
                "restriction_rejected",
                restriction_failure_code
                    .as_deref()
                    .unwrap_or("restriction_rejected"),
            ),
            (false, false, true) => ("cs_prime_rejected", "not_cs_prime"),
            (true, true, false) => (
                "restriction_rejected",
                restriction_failure_code
                    .as_deref()
                    .unwrap_or("restriction_rejected"),
            ),
            (true, false, true) => ("whitelist_rejected", "not_whitelisted"),
            (false, true, true) => (
                "restriction_rejected",
                restriction_failure_code
                    .as_deref()
                    .unwrap_or("restriction_rejected"),
            ),
            (true, true, true) => (
                "restriction_rejected",
                restriction_failure_code
                    .as_deref()
                    .unwrap_or("restriction_rejected"),
            ),
            (false, false, false) => ("access_rejected", "access_rejected"),
        };

    reject_with_method(message, access_method, failure_code)
}

async fn active_ban(
    db: &Database,
    steam_id64: &str,
    ip_address: Option<&str>,
) -> anyhow::Result<Option<ActiveBanInfo>> {
    // 拆成 steam 与 ip 两段独立查询：每段可各自命中部分索引，
    // 避免 OR 组合条件导致全表扫描（进服检查的热点路径）。
    if let Some(row) = query_active_ban_by_steam(db, steam_id64).await? {
        return Ok(Some(row));
    }
    if let Some(ip) = ip_address {
        if let Some(row) = query_active_ban_by_ip(db, ip).await? {
            return Ok(Some(row));
        }
    }
    Ok(None)
}

async fn query_active_ban_by_steam(
    db: &Database,
    steam_id64: &str,
) -> anyhow::Result<Option<ActiveBanInfo>> {
    let row: Option<(uuid::Uuid, String, Option<DateTime<Utc>>)> = sqlx::query_as(
        r#"SELECT id, reason, expires_at
           FROM ban_records
           WHERE status = 'active'
             AND steam_id = $1
             AND (expires_at IS NULL OR expires_at > now())
           ORDER BY created_at DESC
           LIMIT 1"#,
    )
    .bind(steam_id64)
    .fetch_optional(&db.pool)
    .await?;
    Ok(row.map(|(id, reason, expires_at)| ActiveBanInfo {
        id,
        reason,
        expires_at,
    }))
}

async fn query_active_ban_by_ip(
    db: &Database,
    ip_address: &str,
) -> anyhow::Result<Option<ActiveBanInfo>> {
    let row: Option<(uuid::Uuid, String, Option<DateTime<Utc>>)> = sqlx::query_as(
        r#"SELECT id, reason, expires_at
           FROM ban_records
           WHERE status = 'active'
             AND ip_address = $1
             AND (expires_at IS NULL OR expires_at > now())
           ORDER BY created_at DESC
           LIMIT 1"#,
    )
    .bind(ip_address)
    .fetch_optional(&db.pool)
    .await?;
    Ok(row.map(|(id, reason, expires_at)| ActiveBanInfo {
        id,
        reason,
        expires_at,
    }))
}

async fn load_player_profile(
    db: &Database,
    config: &Config,
    steam_id64: &str,
    gokz_cache: &GokzCacheManager,
) -> anyhow::Result<Option<PlayerAccessProfile>> {
    if let Some(cached) = read_cache(db, steam_id64).await? {
        if cached.expires_at > Utc::now() {
            return Ok(Some(PlayerAccessProfile {
                rating: cached.rating,
                steam_level: cached.steam_level,
            }));
        }
    }

    let Some(profile) = fetch_player_profile(config, steam_id64, gokz_cache).await? else {
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
           WHERE steamid64 = $1 AND rating_source = $2"#,
    )
    .bind(steam_id64)
    .bind(ACCESS_RATING_SOURCE)
    .fetch_optional(&db.pool)
    .await?)
}

async fn write_cache(
    db: &Database,
    steam_id64: &str,
    profile: &PlayerAccessProfile,
) -> anyhow::Result<()> {
    sqlx::query(
        r#"INSERT INTO player_access_cache (steamid64, rating, steam_level, rating_source, expires_at, updated_at)
           VALUES ($1, $2, $3, $4, $5, now())
           ON CONFLICT (steamid64, rating_source) DO UPDATE
           SET rating = EXCLUDED.rating,
               steam_level = EXCLUDED.steam_level,
               expires_at = EXCLUDED.expires_at,
               updated_at = now()"#,
    )
    .bind(steam_id64)
    .bind(profile.rating)
    .bind(profile.steam_level)
    .bind(ACCESS_RATING_SOURCE)
    .bind(Utc::now() + Duration::hours(24))
    .execute(&db.pool)
    .await?;
    Ok(())
}

async fn fetch_player_profile(
    config: &Config,
    steam_id64: &str,
    gokz_cache: &GokzCacheManager,
) -> anyhow::Result<Option<PlayerAccessProfile>> {
    let has_level_key = config.steamchina_level_key.is_some() || config.steam_web_key.is_some();
    if !has_level_key {
        warn!(steam_id64, "缺少 Steam API Key，进入限制将放行");
        return Ok(None);
    }

    let steam_level = timeout(
        StdDuration::from_secs(10),
        fetch_steam_level(config, steam_id64),
    )
    .await
    .ok()
    .flatten();
    let rating = gokz_cache.get(steam_id64).await.and_then(|stats| {
        [stats.kzt, stats.skz, stats.vnl, stats.ovr]
            .into_iter()
            .filter_map(|mode| mode.and_then(|value| value.rating))
            .max_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal))
            .map(|value| value.trunc() as i32)
    });
    let rating = match rating {
        Some(value) => Some(value),
        None => timeout(
            StdDuration::from_secs(10),
            fetch_best_gokz_rating(steam_id64),
        )
        .await
        .ok()
        .flatten(),
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

async fn fetch_best_gokz_rating(steam_id64: &str) -> Option<i32> {
    let negative_cache = GOKZ_NEGATIVE_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if negative_cache
        .lock()
        .ok()
        .and_then(|cache| cache.get(steam_id64).copied())
        .is_some_and(|at| at.elapsed() < StdDuration::from_secs(60))
    {
        return None;
    }
    let ratings = join_all(
        GOKZ_RATING_SCOPES
            .iter()
            .map(|scope| fetch_gokz_scope_rating(steam_id64, scope)),
    )
    .await;

    let best_rating = best_gokz_rating(ratings);
    if best_rating.is_none() {
        if let Ok(mut cache) = negative_cache.lock() {
            cache.retain(|_, at| at.elapsed() < StdDuration::from_secs(60));
            cache.insert(steam_id64.to_string(), std::time::Instant::now());
        }
        warn!(
            steam_id64,
            "GOKZ 四个模式 rating 查询全部失败，进入限制将放行"
        );
    }
    best_rating
}

async fn fetch_gokz_scope_rating(steam_id64: &str, scope: &str) -> Option<i32> {
    let url = format!("https://api.gokz.top/v1/leaderboards/players/{steam_id64}");
    match http_client::http_client()
        .get(&url)
        .query(&[("scope", scope)])
        .send()
        .await
    {
        Ok(response) if response.status().is_success() => {
            match response.json::<GokzPlayerResponse>().await {
                Ok(body) => body.rating.map(|rating| rating.trunc() as i32),
                Err(error) => {
                    warn!(steam_id64, scope, error = %error, "GOKZ scoped 玩家资料解析失败");
                    None
                }
            }
        }
        Ok(response) => {
            warn!(steam_id64, scope, status = %response.status(), "GOKZ scoped 玩家资料请求失败");
            None
        }
        Err(error) => {
            warn!(steam_id64, scope, error = %error, "GOKZ scoped 玩家资料请求异常");
            None
        }
    }
}

fn best_gokz_rating(ratings: impl IntoIterator<Item = Option<i32>>) -> Option<i32> {
    ratings.into_iter().flatten().max()
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
        audit_message: None,
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
        audit_message: None,
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
        audit_message: None,
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
            cs_prime_enabled: false,
            risk_block_enabled: false,
            use_custom_access: true,
            community_whitelist_mode_enabled: false,
            community_min_rating: 0,
            community_min_steam_level: 0,
            community_cs_prime_enabled: false,
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
        assert!(result.message.contains("当前服务器开启了进入限制"));
        assert!(result.message.contains("您的账号未达到最低进入要求"));
        assert!(result.message.contains("Q群275164688"));
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
        assert!(result.message.contains("当前服务器开启了进入限制"));
        assert!(result.message.contains("您的账号未达到最低进入要求"));
        assert!(result.message.contains("Q群275164688"));
    }

    #[test]
    fn reject_access_modes_covers_mode_combinations() {
        assert!(reject_access_modes(true, false, false, None, None)
            .message
            .contains("当前服务器开启了白名单验证"));
        assert!(
            reject_access_modes(false, true, false, Some("low_rating".to_string()), None)
                .message
                .contains("当前服务器开启了进入限制")
        );
        assert!(
            reject_access_modes(false, false, true, None, Some("not_cs_prime".to_string()))
                .message
                .contains("非CS优先账户")
        );
        assert!(
            reject_access_modes(true, true, false, Some("low_rating".to_string()), None)
                .message
                .contains("没有获取白名单资格")
        );
        assert!(
            reject_access_modes(true, false, true, None, Some("not_cs_prime".to_string()))
                .message
                .contains("CS为非优先账号并且没有获取白名单资格")
        );
        assert!(reject_access_modes(
            false,
            true,
            true,
            Some("low_steam_level".to_string()),
            Some("not_cs_prime".to_string())
        )
        .message
        .contains("未达到最低进入要求被阻止进入服务器"));
        assert!(reject_access_modes(
            true,
            true,
            true,
            Some("low_rating".to_string()),
            Some("not_cs_prime".to_string())
        )
        .message
        .contains("没有获取白名单资格"));

        let unknown_prime = reject_access_modes(
            false,
            false,
            true,
            None,
            Some("prime_verification_failed".to_string()),
        );
        assert_eq!(
            unknown_prime.failure_code.as_deref(),
            Some("prime_verification_failed")
        );
        assert!(unknown_prime
            .message
            .contains("准入条件未满足：CS 优先账户状态无法验证"));
        assert_eq!(
            unknown_prime.audit_message.as_deref(),
            Some("准入条件未满足：CS 优先账户状态无法验证")
        );

        let combined_unknown_prime = reject_access_modes(
            true,
            true,
            true,
            Some("low_rating".to_string()),
            Some("prime_verification_failed".to_string()),
        );
        assert_eq!(
            combined_unknown_prime.audit_message.as_deref(),
            Some("准入条件未满足：白名单未通过；Rating 未达标；CS 优先账户状态无法验证")
        );
        let plugin_result = serde_json::to_value(&combined_unknown_prime).unwrap();
        assert!(plugin_result.get("audit_message").is_none());
        assert!(plugin_result["message"]
            .as_str()
            .unwrap()
            .contains("白名单未通过；Rating 未达标；CS 优先账户状态无法验证"));
        assert!(combined_unknown_prime.message.len() < 256);
    }

    fn ban_risk(
        codes: &[&str],
        action: player_risk_service::RiskAction,
    ) -> player_risk_service::AccessBanRisk {
        player_risk_service::AccessBanRisk {
            codes: codes.iter().map(|code| code.to_string()).collect(),
            action,
            detail: "同 IP 关联账号中有 1 个存在本地有效封禁".to_string(),
        }
    }

    #[test]
    fn reject_access_risk_uses_player_message_and_audits_signal_detail() {
        let result = reject_access_risk(&ban_risk(
            &["linked_ip_local_ban"],
            player_risk_service::RiskAction::RequireForce,
        ));

        assert!(!result.allowed);
        assert_eq!(result.access_method.as_deref(), Some("risk_blocked"));
        assert_eq!(result.failure_code.as_deref(), Some("linked_ip_banned"));
        assert!(result
            .message
            .contains("您的账号可能有些问题，本次进入服务器被阻止"));
        assert!(result.message.contains("您可以进行申请白名单后再尝试进入"));
        assert_eq!(
            result.audit_message.as_deref(),
            Some("账号风险拦截（高风险）：同 IP 关联账号中有 1 个存在本地有效封禁")
        );
        // 审计原因不返回给游戏插件
        let plugin_result = serde_json::to_value(&result).unwrap();
        assert!(plugin_result.get("audit_message").is_none());
        assert!(result.message.len() < 256);
    }

    #[test]
    fn reject_access_risk_maps_self_ban_signals_to_generic_failure_code() {
        let self_ban = reject_access_risk(&ban_risk(
            &["self_active_global_ban"],
            player_risk_service::RiskAction::Deny,
        ));
        assert_eq!(self_ban.failure_code.as_deref(), Some("risk_blocked"));

        let mixed = reject_access_risk(&ban_risk(
            &["self_active_global_ban", "linked_ip_global_ban"],
            player_risk_service::RiskAction::Deny,
        ));
        assert_eq!(mixed.failure_code.as_deref(), Some("risk_blocked"));
    }

    #[test]
    fn risk_block_message_stays_within_plugin_chat_limit() {
        assert!(RISK_BLOCK_MESSAGE.contains("Q群275164688"));
        assert!(RISK_BLOCK_MESSAGE.len() < 256);
    }

    #[test]
    fn gokz_player_response_accepts_decimal_rating() {
        let response: GokzPlayerResponse =
            serde_json::from_str(r#"{"steam_name":"PlayerOne","rating":8.352655}"#).unwrap();
        assert_eq!(response.steam_name.as_deref(), Some("PlayerOne"));
        assert_eq!(response.rating.map(|rating| rating.trunc() as i32), Some(8));
    }

    #[test]
    fn best_gokz_rating_uses_highest_available_scope() {
        assert_eq!(
            best_gokz_rating([Some(900), None, Some(1500), Some(1499)]),
            Some(1500)
        );
        assert_eq!(best_gokz_rating([None, None, None]), None);
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

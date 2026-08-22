use crate::routes::{forbidden, invalid_request, AppCtx, ListQuery};
use crate::services::{
    audit_service, ban_service, dashboard_service, global_ban_service, log_service,
    lumi_bot_service, notification_service, permission_service, public_service,
    rate_limit_service::extract_client_ip, whitelist_service,
};
use anyhow;
use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use std::collections::HashSet;
use std::time::Duration;
use uuid::Uuid;

#[derive(serde::Deserialize)]
#[allow(dead_code)]
pub(crate) struct WhitelistBody {
    steam_input: Option<String>,
    nickname: Option<String>,
    contact: Option<String>,
    operator_name: Option<String>,
    steam_token: Option<String>,
}

#[derive(serde::Deserialize)]
pub(crate) struct ResolveSteamBody {
    steam_input: String,
}

#[derive(serde::Serialize)]
pub(crate) struct SteamResolveResponse {
    steamid64: String,
    steamid: Option<String>,
    steamid3: Option<String>,
    profile_url: Option<String>,
    persona_name: Option<String>,
}

#[derive(serde::Deserialize)]
pub(crate) struct GlobalBansBatchBody {
    steamids: Vec<String>,
}

#[derive(serde::Deserialize)]
pub(crate) struct QueryBansBody {
    steam_input: String,
}

pub(crate) async fn public_whitelist(
    State(ctx): State<AppCtx>,
    Query(query): Query<ListQuery>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let result = public_service::list_public_whitelist(&ctx.db, &query)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "加载公开白名单列表失败");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(Json(
        serde_json::json!({ "items": result.items, "total": result.total, "page": result.page, "page_size": result.page_size }),
    ))
}

pub(crate) async fn submit_whitelist(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Json(body): Json<WhitelistBody>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<serde_json::Value>)> {
    let resolver = &ctx.steam_resolver;

    // 如果提供了 steam_token，验证 Steam 认证会话
    let (steam_input, nickname) = if let Some(ref token) = body.steam_token {
        let verified_steamid64 = crate::routes::steam_auth::verify_steam_session(&ctx.db, token)
            .await
            .map_err(invalid_request)?;
        // 获取 Steam 资料
        let persona_name = resolver
            .fetch_profile(&verified_steamid64)
            .await
            .ok()
            .flatten()
            .map(|p| p.persona_name);
        let nick = body
            .nickname
            .clone()
            .unwrap_or_else(|| persona_name.unwrap_or_else(|| verified_steamid64.clone()));
        (Some(verified_steamid64), Some(nick))
    } else {
        (body.steam_input.clone(), body.nickname.clone())
    };

    let si = steam_input
        .ok_or_else(|| invalid_request(anyhow::anyhow!("请提供 Steam 标识符或 Steam 认证令牌")))?;
    let nn = nickname.ok_or_else(|| invalid_request(anyhow::anyhow!("请提供游戏昵称")))?;

    let item = whitelist_service::create_public_whitelist_request(
        &ctx.db,
        whitelist_service::PublicWhitelistRequestInput {
            nickname: nn,
            steam_input: si,
            contact: body.contact,
        },
        resolver,
    )
    .await
    .map_err(invalid_request)?;
    let _ = log_service::create_log(
        &ctx.db,
        "guest",
        "公共展示页",
        "提交白名单申请",
        &item.nickname,
        &extract_client_ip(&headers),
    )
    .await;
    if let Err(e) = notification_service::notify_whitelist_apply(
        &ctx.db,
        &ctx.notification_hub,
        &item.nickname,
        &item.steamid64,
    )
    .await
    {
        tracing::warn!(%e, "whitelist apply notification failed");
    }
    // LumiBot（QQ 机器人）事件上报：新白名单申请立即调用 LumiBot API 上报，
    // 然后由 QQ 机器人立即推送通知；若立即上报失败则降级入队列，由后台任务兜底重试。
    if let Err(e) = lumi_bot_service::report_whitelist_created(&ctx.db, &ctx.config, &item).await {
        tracing::warn!(%e, "LumiBot 白名单申请事件上报失败");
    }
    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({ "item": item })),
    ))
}

pub(crate) async fn public_bans(
    State(ctx): State<AppCtx>,
    Query(query): Query<ListQuery>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let result = public_service::list_public_bans(&ctx.db, &query)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "加载公开封禁列表失败");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    let stats = public_service::ban_stats(&ctx.db).await.map_err(|e| {
        tracing::error!(error = %e, "加载封禁统计失败");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(
        serde_json::json!({ "items": result.items, "total": result.total, "page": result.page, "page_size": result.page_size, "stats": stats }),
    ))
}

pub(crate) async fn resolve_steam(
    State(ctx): State<AppCtx>,
    Json(body): Json<ResolveSteamBody>,
) -> Result<Json<SteamResolveResponse>, (StatusCode, Json<serde_json::Value>)> {
    let resolver = &ctx.steam_resolver;

    // 解析 Steam 标识符
    let parsed = match resolver.resolve(&body.steam_input).await {
        Ok(p) => p,
        Err(e) => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": e.to_string() })),
            ));
        }
    };

    // 尝试获取 Steam 资料（5秒超时，超时则让玩家手动填写）
    let persona_name = match tokio::time::timeout(
        Duration::from_secs(5),
        resolver.fetch_profile(&parsed.steamid64),
    )
    .await
    {
        Ok(Ok(Some(profile))) => Some(profile.persona_name),
        _ => None,
    };

    Ok(Json(SteamResolveResponse {
        steamid64: parsed.steamid64,
        steamid: parsed.steamid,
        steamid3: parsed.steamid3,
        profile_url: parsed.profile_url,
        persona_name,
    }))
}

/// 按 Steam 标识符查询该玩家的活跃封禁记录（供封禁公示页使用）
pub(crate) async fn query_active_bans(
    State(ctx): State<AppCtx>,
    Json(body): Json<QueryBansBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let resolver = &ctx.steam_resolver;
    let parsed = resolver.resolve(&body.steam_input).await.map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": e.to_string()})),
        )
    })?;

    let bans = ban_service::find_active_bans_by_steamid(&ctx.db, &parsed.steamid64)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "查询活跃封禁失败");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "查询失败"})),
            )
        })?;

    Ok(Json(serde_json::json!({
        "steamid64": parsed.steamid64,
        "bans": bans,
    })))
}

/// 查询全球封禁记录。
/// 数据来自后台同步维护的 global_bans 表，避免公开页面直接打 KZTimer 限额。
pub(crate) async fn get_global_bans(
    State(ctx): State<AppCtx>,
    Path(steamid64): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let bans = global_ban_service::public_global_bans_for_steamid(&ctx.db, &steamid64)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, steamid64 = %steamid64, "查询本地全球封禁失败");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "查询失败" })),
            )
        })?;

    Ok(Json(serde_json::json!(bans)))
}

/// 批量查询全球封禁记录（从本地同步表读取，减少 KZTimer 请求）
pub(crate) async fn get_global_bans_batch(
    State(ctx): State<AppCtx>,
    Json(body): Json<GlobalBansBatchBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let mut seen = HashSet::new();
    let steamids: Vec<String> = body
        .steamids
        .into_iter()
        .map(|steamid| steamid.trim().to_string())
        .filter(|steamid| !steamid.is_empty() && seen.insert(steamid.clone()))
        .take(30)
        .collect();

    let results = global_ban_service::public_global_bans_batch(&ctx.db, &steamids)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "批量查询本地全球封禁失败");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "查询失败" })),
            )
        })?;

    Ok(Json(serde_json::json!({ "results": results })))
}

// ---------------------------------------------------------------------------
// gokz.top 玩家统计代理（前端无法直接访问 gokz API，需要后端代理绕过 CORS）
// 使用统一的 GokzCacheManager 进行缓存管理（PostgreSQL + 内存二级缓存）
// ---------------------------------------------------------------------------

use crate::services::gokz_cache::{GokzModeStats, GokzStats};

const GOKZ_SCOPES: [&str; 4] = ["KZT", "SKZ", "VNL", "OVR"];

#[derive(serde::Deserialize)]
pub(crate) struct GokzPlayerStatsQuery {
    scope: String,
}

/// 从 gokz.top 获取单个 scope 的排行榜数据
async fn fetch_gokz_scope(steamid64: &str, scope: &str) -> Option<GokzModeStats> {
    use crate::http_client;

    let url = format!(
        "https://api.gokz.top/v1/leaderboards/players/{}?scope={}",
        steamid64, scope
    );

    let data = tokio::time::timeout(Duration::from_secs(8), async {
        let response = http_client::http_client().get(&url).send().await.ok()?;
        if !response.status().is_success() {
            return None;
        }
        response.json::<serde_json::Value>().await.ok()
    })
    .await
    .ok()
    .flatten()?;

    // 解析 GOKZ API 响应格式
    serde_json::from_value(data).ok()
}

/// 代理 gokz.top 排行榜接口，获取玩家 KZ 统计（带缓存）
pub(crate) async fn get_gokz_player_stats(
    State(ctx): State<AppCtx>,
    Path(steamid64): Path<String>,
    Query(params): Query<GokzPlayerStatsQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    if !GOKZ_SCOPES.contains(&params.scope.as_str()) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "无效的 scope 参数" })),
        ));
    }

    // 尝试从缓存获取
    if let Some(stats) = ctx.gokz_cache.get(&steamid64).await {
        let mode_stats = match params.scope.to_uppercase().as_str() {
            "KZT" => &stats.kzt,
            "SKZ" => &stats.skz,
            "VNL" => &stats.vnl,
            "OVR" => &stats.ovr,
            _ => &None,
        };
        let mut obj = serde_json::Map::new();
        obj.insert(
            params.scope.to_uppercase(),
            serde_json::to_value(mode_stats).unwrap_or(serde_json::Value::Null),
        );
        return Ok(Json(serde_json::Value::Object(obj)));
    }

    // 缓存未命中，从 gokz.top 获取
    let data = fetch_gokz_scope(&steamid64, &params.scope).await;

    // 如果获取成功，写入缓存
    if let Some(mode_stats) = &data {
        let mut stats = GokzStats::default();
        match params.scope.to_uppercase().as_str() {
            "KZT" => stats.kzt = Some(mode_stats.clone()),
            "SKZ" => stats.skz = Some(mode_stats.clone()),
            "VNL" => stats.vnl = Some(mode_stats.clone()),
            "OVR" => stats.ovr = Some(mode_stats.clone()),
            _ => {}
        }
        ctx.gokz_cache.set(&steamid64, &stats).await;
    }

    let mut obj = serde_json::Map::new();
    obj.insert(
        params.scope.to_uppercase(),
        serde_json::to_value(&data).unwrap_or(serde_json::Value::Null),
    );
    Ok(Json(serde_json::Value::Object(obj)))
}

#[derive(serde::Deserialize)]
pub(crate) struct GokzBatchBody {
    steamid64: String,
}

/// 批量查询玩家所有 4 个 scope 的 KZ 统计（带缓存，后端并发请求）
pub(crate) async fn get_gokz_player_stats_batch(
    State(ctx): State<AppCtx>,
    Json(body): Json<GokzBatchBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let steamid64 = body.steamid64;

    // 尝试从缓存获取（包含所有 4 个 scope）
    if let Some(stats) = ctx.gokz_cache.get(&steamid64).await {
        return Ok(Json(serde_json::json!({
            "KZT": stats.kzt,
            "SKZ": stats.skz,
            "VNL": stats.vnl,
            "OVR": stats.ovr,
        })));
    }

    // 缓存未命中，并发请求所有 4 个 scope
    let fetches: Vec<_> = GOKZ_SCOPES
        .iter()
        .map(|scope| fetch_gokz_scope(&steamid64, scope))
        .collect();

    let results = futures::future::join_all(fetches).await;

    // 构建统计数据
    let mut stats = GokzStats::default();
    if let Some(s) = results.first().and_then(|r| r.clone()) {
        stats.kzt = Some(s);
    }
    if let Some(s) = results.get(1).and_then(|r| r.clone()) {
        stats.skz = Some(s);
    }
    if let Some(s) = results.get(2).and_then(|r| r.clone()) {
        stats.vnl = Some(s);
    }
    if let Some(s) = results.get(3).and_then(|r| r.clone()) {
        stats.ovr = Some(s);
    }

    // 写入缓存
    ctx.gokz_cache.set(&steamid64, &stats).await;

    Ok(Json(serde_json::json!({
        "KZT": stats.kzt,
        "SKZ": stats.skz,
        "VNL": stats.vnl,
        "OVR": stats.ovr,
    })))
}

/// 批量预加载多个玩家的 GOKZ 统计数据
#[derive(serde::Deserialize)]
pub(crate) struct GokzBatchPreloadBody {
    steamid64s: Vec<String>,
}

pub(crate) async fn preload_gokz_stats(
    State(ctx): State<AppCtx>,
    Json(body): Json<GokzBatchPreloadBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    if body.steamid64s.is_empty() {
        return Ok(Json(serde_json::json!({})));
    }

    // 获取批量缓存
    let cached = ctx.gokz_cache.get_batch(&body.steamid64s).await;

    // 对于未缓存的玩家，并发请求并写入缓存
    let uncached: Vec<String> = body
        .steamid64s
        .iter()
        .filter(|s| !cached.contains_key(*s))
        .cloned()
        .collect();

    if !uncached.is_empty() {
        let fetches: Vec<_> = uncached
            .iter()
            .flat_map(|sid| {
                GOKZ_SCOPES
                    .iter()
                    .map(|scope| fetch_gokz_scope(sid, scope))
                    .collect::<Vec<_>>()
            })
            .collect();

        let results = futures::future::join_all(fetches).await;

        // 按玩家分组写入缓存
        for (i, sid) in uncached.iter().enumerate() {
            let base = i * 4;
            let mut stats = GokzStats::default();
            if let Some(s) = results.get(base).and_then(|r| r.clone()) {
                stats.kzt = Some(s);
            }
            if let Some(s) = results.get(base + 1).and_then(|r| r.clone()) {
                stats.skz = Some(s);
            }
            if let Some(s) = results.get(base + 2).and_then(|r| r.clone()) {
                stats.vnl = Some(s);
            }
            if let Some(s) = results.get(base + 3).and_then(|r| r.clone()) {
                stats.ovr = Some(s);
            }
            ctx.gokz_cache.set(sid, &stats).await;
        }
    }

    // 返回所有玩家的缓存数据
    let final_cached = ctx.gokz_cache.get_batch(&body.steamid64s).await;
    let mut response = serde_json::Map::new();
    for (steamid64, stats) in final_cached {
        response.insert(
            steamid64,
            serde_json::json!({
                "KZT": stats.kzt,
                "SKZ": stats.skz,
                "VNL": stats.vnl,
                "OVR": stats.ovr,
            }),
        );
    }

    Ok(Json(serde_json::Value::Object(response)))
}

// QQ 机器人集成：获取待审核统计
pub(crate) async fn qq_review_stats(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    // 验证 QQ 集成令牌
    let token = headers
        .get("x-qq-token")
        .and_then(|v| v.to_str().ok())
        .or_else(|| {
            headers
                .get(axum::http::header::AUTHORIZATION)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.strip_prefix("Bearer "))
        });

    let expected_token = ctx.config.qq_integration_token.as_deref();

    match (token, expected_token) {
        (Some(provided), Some(expected))
            if constant_time_eq(provided.as_bytes(), expected.as_bytes()) =>
        {
            // 令牌验证通过
        }
        (None, None) => {
            // 未配置令牌，拒绝访问
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({
                    "error": "QQ 集成未启用。请在后端配置 QQ_INTEGRATION_TOKEN 环境变量。"
                })),
            ));
        }
        _ => {
            // 令牌不匹配或未提供
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"error": "无效的集成令牌"})),
            ));
        }
    }

    // 获取待审核数量（包含所有类型）
    let counts = dashboard_service::get_review_counts(&ctx.db)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "QQ 集成：获取待审核数量失败");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "获取待审核数量失败"})),
            )
        })?;

    Ok(Json(serde_json::json!({
        "whitelist": counts.whitelist,
        "abnormal_record": counts.abnormal_record,
        "total": counts.whitelist + counts.abnormal_record,
    })))
}

// QQ 机器人集成：获取待审核白名单详情
pub(crate) async fn qq_pending_whitelist(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    // 验证 QQ 集成令牌（复用 qq_review_stats 的验证逻辑）
    let token = headers
        .get("x-qq-token")
        .and_then(|v| v.to_str().ok())
        .or_else(|| {
            headers
                .get(axum::http::header::AUTHORIZATION)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.strip_prefix("Bearer "))
        });

    let expected_token = ctx.config.qq_integration_token.as_deref();

    match (token, expected_token) {
        (Some(provided), Some(expected))
            if constant_time_eq(provided.as_bytes(), expected.as_bytes()) =>
        {
            // 令牌验证通过
        }
        (None, None) => {
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({
                    "error": "QQ 集成未启用。请在后端配置 QQ_INTEGRATION_TOKEN 环境变量。"
                })),
            ));
        }
        _ => {
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"error": "无效的集成令牌"})),
            ));
        }
    }

    // 获取待审核白名单列表（只返回 pending 状态，最多 20 条）
    let query = ListQuery {
        status: Some("pending".to_string()),
        search: None,
        source: None,
        page: Some(1),
        page_size: Some(20),
    };

    let result = whitelist_service::list_whitelist(&ctx.db, &query)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "QQ 集成：获取待审核白名单失败");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "获取待审核白名单失败"})),
            )
        })?;

    // 简化返回数据，只保留必要字段
    let simplified: Vec<serde_json::Value> = result
        .items
        .into_iter()
        .map(|item| {
            serde_json::json!({
                "steamid64": item.steamid64,
                "steamid": item.steamid,
                "nickname": item.nickname,
                "steam_persona_name": item.steam_persona_name,
                "applied_at": item.applied_at,
                "profile_url": item.profile_url,
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "items": simplified,
        "total": result.total,
    })))
}

// QQ 机器人集成：获取所有类型的待审核详情（综合视图）
pub(crate) async fn qq_pending_all(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    // 验证 QQ 集成令牌
    let token = headers
        .get("x-qq-token")
        .and_then(|v| v.to_str().ok())
        .or_else(|| {
            headers
                .get(axum::http::header::AUTHORIZATION)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.strip_prefix("Bearer "))
        });

    let expected_token = ctx.config.qq_integration_token.as_deref();

    match (token, expected_token) {
        (Some(provided), Some(expected))
            if constant_time_eq(provided.as_bytes(), expected.as_bytes()) => {}
        (None, None) => {
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({
                    "error": "QQ 集成未启用。请在后端配置 QQ_INTEGRATION_TOKEN 环境变量。"
                })),
            ));
        }
        _ => {
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"error": "无效的集成令牌"})),
            ));
        }
    }

    // 并发获取所有类型的待审核数据（各取最多 10 条）
    let query = ListQuery {
        status: Some("pending".to_string()),
        search: None,
        source: None,
        page: Some(1),
        page_size: Some(10),
    };

    let whitelist_result = whitelist_service::list_whitelist(&ctx.db, &query).await;

    // 格式化白名单申请
    let whitelist_items: Vec<serde_json::Value> = whitelist_result
        .map(|r| r.items)
        .unwrap_or_default()
        .into_iter()
        .map(|item| {
            serde_json::json!({
                "type": "whitelist",
                "steamid": item.steamid,
                "nickname": item.nickname,
                "steam_persona_name": item.steam_persona_name,
                "applied_at": item.applied_at,
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "whitelist": whitelist_items,
        "counts": {
            "whitelist": whitelist_items.len(),
            "total": whitelist_items.len(),
        }
    })))
}

/// 校验 QQ 集成令牌（`x-qq-token` 或 `Authorization: Bearer`）。
fn verify_qq_token(
    ctx: &AppCtx,
    headers: &HeaderMap,
) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
    let token = headers
        .get("x-qq-token")
        .and_then(|v| v.to_str().ok())
        .or_else(|| {
            headers
                .get(axum::http::header::AUTHORIZATION)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.strip_prefix("Bearer "))
        });

    let expected_token = ctx.config.qq_integration_token.as_deref();

    match (token, expected_token) {
        (Some(provided), Some(expected))
            if constant_time_eq(provided.as_bytes(), expected.as_bytes()) =>
        {
            Ok(())
        }
        (None, None) => Err((
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({
                "error": "QQ 集成未启用。请在后端配置 QQ_INTEGRATION_TOKEN 环境变量。"
            })),
        )),
        _ => Err((
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "无效的集成令牌" })),
        )),
    }
}

/// QQ 机器人集成：通过 QQ 聊天审批白名单申请。
/// body: { action: "approve"|"reject", openid, interaction_id, reason?, force? }
/// 按 openid 定位后台管理员并记为操作人，渠道标记为 'qq'。
/// 所有判定（openid 绑定 / 启用 / 角色权限）都会写入 audit_logs（source='qq_bot'），
/// 供 LumiBot 状态页展示，方便线上排查"点了没反应 / 无权限"的问题。
#[derive(serde::Deserialize)]
pub(crate) struct QqWhitelistReviewBody {
    action: String,
    openid: String,
    interaction_id: String,
    reason: Option<String>,
    force: Option<bool>,
}

fn qq_whitelist_review_replay(
    audit_id: Uuid,
    details: Option<serde_json::Value>,
    whitelist_id: Uuid,
    action: &str,
    openid: &str,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let expected_whitelist_id = whitelist_id.to_string();
    let matches_request = details.as_ref().is_some_and(|value| {
        value.get("whitelist_id").and_then(|v| v.as_str()) == Some(expected_whitelist_id.as_str())
            && value.get("action").and_then(|v| v.as_str()) == Some(action)
            && value.get("reviewer_openid").and_then(|v| v.as_str()) == Some(openid)
    });
    if !matches_request {
        return Err((
            StatusCode::CONFLICT,
            Json(serde_json::json!({ "error": "interaction_id 已用于其他审批请求" })),
        ));
    }
    let item = details
        .as_ref()
        .and_then(|value| value.get("item"))
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    Ok(Json(serde_json::json!({
        "ok": true,
        "idempotent": true,
        "audit_id": audit_id,
        "item": item,
    })))
}

pub(crate) async fn qq_whitelist_review(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(body): Json<QqWhitelistReviewBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    verify_qq_token(&ctx, &headers)?;

    let action = body.action.trim();
    let openid = body.openid.trim();

    // 审批请求的每个判定阶段都写入 audit_logs（source='qq_bot'），
    // 供 LumiBot 状态页展示，方便线上排查"点了没反应 / 无权限"的问题。
    #[allow(unused)]
    let write_review_log =
        |ctx: &AppCtx, operation: &str, message: String, details: serde_json::Value| {
            let result = async {
                audit_service::write_audit_log_with_context(
                    &ctx.db,
                    audit_service::AuditLogInput {
                        operation: operation.to_string(),
                        target: id.to_string(),
                        target_type: "whitelist".to_string(),
                        player_name: None,
                        reason: None,
                        duration_minutes: None,
                        operator_name: openid.to_string(),
                        operator_steamid: None,
                        source: "qq_bot".to_string(),
                        server_id: None,
                        server_name: None,
                        server_port: None,
                        success: true,
                        message: Some(message),
                        idempotency_key: None,
                    },
                    None,
                    Some(details),
                )
                .await
            };
            let _ = result;
        };
    if openid.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "缺少审批者 openid" })),
        ));
    }

    let interaction_id = body.interaction_id.trim();
    if interaction_id.is_empty() || interaction_id.len() > 200 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "interaction_id 不能为空且不能超过 200 字符" })),
        ));
    }
    let reason = body
        .reason
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let force = body.force.unwrap_or(false);
    match action {
        "approve" => {}
        "reject" if reason.is_none() => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "拒绝申请必须填写原因" })),
            ));
        }
        "reject" => {}
        _ => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "action 只能为 approve 或 reject" })),
            ));
        }
    }

    // 按 openid 查找可审批的后台管理员。通知开关只控制是否发送新申请通知，
    // 不参与审批授权：只要账号已启用、openid 匹配且角色允许审批即可操作。
    // 判定结果写入 audit_logs（source='qq_bot'），便于 LumiBot 状态页排查。
    let user: Option<(Uuid, String, String, String, Option<String>)> = sqlx::query_as(
        r#"SELECT id, username, display_name, role, remark FROM users
           WHERE openid = $1
             AND enabled = true
             AND role IN ('developer', 'admin', 'normal')
           LIMIT 1"#,
    )
    .bind(openid)
    .fetch_optional(&ctx.db.pool)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "QQ 审批：按 openid 查询管理员失败");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "查询审批者失败" })),
        )
    })?;

    let (user_id, username, display_name, role, remark) = match user {
        Some(user) => user,
        None => {
            tracing::warn!(openid = %openid, "QQ 审批拒绝：openid 未绑定启用中的可审批管理员");
            let _ = audit_service::write_audit_log_with_context(
                &ctx.db,
                audit_service::AuditLogInput {
                    operation: "qq_review_denied".to_string(),
                    target: id.to_string(),
                    target_type: "whitelist".to_string(),
                    player_name: None,
                    reason: Some("该 openid 未绑定到有效管理员，或管理员已被禁用".to_string()),
                    duration_minutes: None,
                    operator_name: openid.to_string(),
                    operator_steamid: None,
                    source: "qq_bot".to_string(),
                    server_id: None,
                    server_name: None,
                    server_port: None,
                    success: false,
                    message: Some("QQ 审批被拒绝：openid 未绑定启用中的可审批管理员".to_string()),
                    idempotency_key: None,
                },
                None,
                Some(serde_json::json!({
                    "action": action,
                    "openid": openid,
                    "whitelist_id": id,
                    "interaction_id": interaction_id,
                    "reason": reason,
                })),
            )
            .await;
            return Err((
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({
                    "error": "该 openid 未绑定到有效管理员，或管理员已被禁用"
                })),
            ));
        }
    };

    // 审批记录落库的操作人名称：优先用网站在「备注」里配置的名称，
    // 其次退到 display_name，再退到 username，避免把 QQ 相关标识写进审批记录。
    let operator_label = {
        let r = remark.as_deref().map(str::trim).unwrap_or("");
        let d = display_name.trim();
        let u = username.trim();
        if !r.is_empty() {
            r.to_string()
        } else if !d.is_empty() {
            d.to_string()
        } else if !u.is_empty() {
            u.to_string()
        } else {
            "管理员".to_string()
        }
    };

    // 权限校验：是否可审批白名单
    let operator = crate::models::Operator {
        id: user_id,
        username,
        display_name: display_name.clone(),
        role: role.clone(),
    };
    if !permission_service::can_review_whitelist(&operator) {
        tracing::warn!(
            openid = %openid,
            role = %operator.role,
            "QQ 审批拒绝：管理员角色没有白名单审批权限"
        );
        let _ = audit_service::write_audit_log_with_context(
            &ctx.db,
            audit_service::AuditLogInput {
                operation: "qq_review_denied".to_string(),
                target: id.to_string(),
                target_type: "whitelist".to_string(),
                player_name: None,
                reason: Some("该管理员角色没有白名单审批权限".to_string()),
                duration_minutes: None,
                operator_name: operator_label.clone(),
                operator_steamid: None,
                source: "qq_bot".to_string(),
                server_id: None,
                server_name: None,
                server_port: None,
                success: false,
                message: Some(format!(
                    "QQ 审批被拒绝：角色 {} 无白名单审批权限",
                    operator.role
                )),
                idempotency_key: None,
            },
            None,
            Some(serde_json::json!({
                "action": action,
                "openid": openid,
                "whitelist_id": id,
                "interaction_id": interaction_id,
                "role": operator.role,
                "user_id": user_id,
                "reason": reason,
            })),
        )
        .await;
        return Err(forbidden());
    }

    let idempotency_key = format!("qq-whitelist-review:{interaction_id}");
    if let Some((audit_id, details)) = sqlx::query_as::<_, (Uuid, Option<serde_json::Value>)>(
        "SELECT id, details FROM audit_logs WHERE idempotency_key = $1",
    )
    .bind(&idempotency_key)
    .fetch_optional(&ctx.db.pool)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "QQ 审批：查询幂等记录失败");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "查询审批幂等记录失败" })),
        )
    })? {
        return qq_whitelist_review_replay(audit_id, details, id, action, openid);
    }

    match action {
        "approve" => whitelist_service::validate_whitelist_approval(&ctx.db, id, force, reason)
            .await
            .map_err(invalid_request)?,
        "reject" => {}
        _ => unreachable!(),
    }

    let mut tx = ctx.db.pool.begin().await.map_err(|e| {
        tracing::error!(error = %e, "QQ 审批：开启事务失败");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "开启审批事务失败" })),
        )
    })?;

    // 同一 interaction_id 跨实例串行处理；拿到锁后再次查询审计记录，
    // 重复投递直接返回第一次提交的权威结果。
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(&idempotency_key)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "QQ 审批：获取幂等锁失败");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "获取审批幂等锁失败" })),
            )
        })?;

    if let Some((audit_id, details)) = sqlx::query_as::<_, (Uuid, Option<serde_json::Value>)>(
        "SELECT id, details FROM audit_logs WHERE idempotency_key = $1",
    )
    .bind(&idempotency_key)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "QQ 审批：查询幂等记录失败");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "查询审批幂等记录失败" })),
        )
    })? {
        tx.rollback().await.ok();
        return qq_whitelist_review_replay(audit_id, details, id, action, openid);
    }

    let result = match action {
        "approve" => {
            whitelist_service::approve_whitelist_tx(
                &mut tx,
                id,
                whitelist_service::ApproveWhitelistInput {
                    operator_name: &operator_label,
                    reason,
                    force,
                    via: "qq",
                },
            )
            .await
        }
        "reject" => {
            whitelist_service::reject_whitelist_tx(
                &mut tx,
                id,
                reason.unwrap_or_default(),
                &operator_label,
                "qq",
            )
            .await
        }
        _ => unreachable!(),
    };

    let item = result.map_err(|e| {
        let msg = e.to_string();
        if msg.contains("已被他人审批") {
            (
                StatusCode::CONFLICT,
                Json(serde_json::json!({ "error": msg, "already_reviewed": true })),
            )
        } else {
            tracing::warn!(error = %e, whitelist_id = %id, "QQ 审批失败");
            (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": msg })),
            )
        }
    })?;

    let response_item = if action == "approve" {
        serde_json::json!({
            "whitelist_id": id,
            "steamid64": item.steamid64,
            "nickname": item.nickname,
            "status": "approved",
            "approved_by": operator_label,
            "approved_via": "qq",
            "rejected_by": null,
            "rejected_via": null,
        })
    } else {
        serde_json::json!({
            "whitelist_id": id,
            "steamid64": item.steamid64,
            "nickname": item.nickname,
            "status": "rejected",
            "approved_by": null,
            "approved_via": null,
            "rejected_by": operator_label,
            "rejected_via": "qq",
        })
    };

    let operation = if action == "approve" {
        "whitelist_approve"
    } else {
        "whitelist_reject"
    };
    let details = serde_json::json!({
        "channel": "qq",
        "interaction_id": interaction_id,
        "whitelist_id": id,
        "reviewer_openid": openid,
        "reviewer_user_id": user_id,
        "reviewer_username": operator.username,
        "reviewer_role": operator.role,
        "action": action,
        "force": force,
        "item": response_item,
    });
    let client_ip = {
        let value = extract_client_ip(&headers);
        (!value.trim().is_empty()).then_some(value)
    };
    let audit = audit_service::write_audit_log_in_transaction(
        &mut tx,
        audit_service::AuditLogInput {
            operation: operation.to_string(),
            target: item.steamid64.clone(),
            target_type: "whitelist".to_string(),
            player_name: Some(item.nickname.clone()),
            reason: reason.map(ToOwned::to_owned),
            duration_minutes: None,
            operator_name: operator_label.clone(),
            operator_steamid: None,
            source: "qq".to_string(),
            server_id: None,
            server_name: None,
            server_port: None,
            success: true,
            message: Some(format!(
                "QQ {}白名单申请，ID: {id}",
                if action == "approve" {
                    "通过"
                } else {
                    "拒绝"
                }
            )),
            idempotency_key: Some(idempotency_key),
        },
        client_ip,
        Some(details),
    )
    .await
    .map_err(|e| {
        tracing::error!(error = %e, whitelist_id = %id, "QQ 审批：审计写入失败，事务回滚");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "审批审计写入失败" })),
        )
    })?;
    tx.commit().await.map_err(|e| {
        tracing::error!(error = %e, whitelist_id = %id, "QQ 审批：提交事务失败");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "提交审批事务失败" })),
        )
    })?;

    let via_label = if action == "approve" {
        "通过白名单申请(QQ)"
    } else {
        "拒绝白名单申请(QQ)"
    };
    log_service::log_action(
        &ctx.db,
        &operator_label,
        "白名单管理",
        via_label,
        &item.nickname,
        "qq",
    )
    .await;

    Ok(Json(serde_json::json!({
        "ok": true,
        "idempotent": false,
        "audit_id": audit.id,
        "item": response_item,
    })))
}

/// 常量时间比较，防止通过响应时间差异推断令牌内容
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut result = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        result |= x ^ y;
    }
    result == 0
}

use crate::routes::{invalid_request, AppCtx, ListQuery};
use anyhow;
use crate::services::{
    ban_service, dashboard_service, global_ban_service, log_service, lumi_bot_service,
    notification_service, public_service, rate_limit_service::extract_client_ip, whitelist_service,
};
use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use std::collections::HashSet;
use std::time::Duration;

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
        let verified_steamid64 =
            crate::routes::steam_auth::verify_steam_session(&ctx.db, token)
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
        (
            Some(verified_steamid64),
            Some(nick),
        )
    } else {
        (body.steam_input.clone(), body.nickname.clone())
    };

    let si = steam_input.ok_or_else(|| {
        invalid_request(anyhow::anyhow!("请提供 Steam 标识符或 Steam 认证令牌"))
    })?;
    let nn = nickname.ok_or_else(|| {
        invalid_request(anyhow::anyhow!("请提供游戏昵称"))
    })?;

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
    // LumiBot（QQ 机器人）事件队列：新白名单申请入队，
    // 由后台任务每 30 分钟集中通过 LumiBot API 上报，再由 QQ 机器人通知管理员。
    if ctx.config.lumi_bot_enabled() {
        if let Err(e) = lumi_bot_service::enqueue_whitelist_created(&ctx.db, &item).await {
            tracing::warn!(%e, "LumiBot 白名单申请事件入队失败");
        }
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

use crate::routes::{AppCtx, ListQuery, invalid_request};
use crate::services::{
    ban_appeal_service, ban_service, log_service, notification_service, public_service, r2_storage,
    rate_limit_service::extract_client_ip, whitelist_service,
};
use axum::{
    Json,
    extract::{Multipart, Path, Query, State},
    http::{HeaderMap, StatusCode},
};
use futures::stream::StreamExt;
use std::collections::{HashMap, HashSet};
use std::time::Duration;
use uuid::Uuid;

#[derive(serde::Deserialize)]
#[allow(dead_code)]
pub(crate) struct WhitelistBody {
    steam_input: String,
    nickname: String,
    operator_name: Option<String>,
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

#[derive(serde::Deserialize)]
pub(crate) struct SubmitAppealBody {
    pub ban_id: Uuid,
    pub steam_id: String,
    pub player_name: String,
    pub appeal_reason: String,
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
    let item = whitelist_service::create_public_whitelist_request(
        &ctx.db,
        whitelist_service::PublicWhitelistRequestInput {
            nickname: body.nickname,
            steam_input: body.steam_input,
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
    Ok((StatusCode::CREATED, Json(serde_json::json!({"item": item}))))
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
    let stats = public_service::ban_stats(&ctx.db)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "加载封禁统计失败");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(Json(
        serde_json::json!({ "items": result.items, "total": result.total, "page": result.page, "page_size": result.page_size, "stats": stats }),
    ))
}

pub(crate) async fn public_ban_appeals_info() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "ok": true,
        "message": "Use POST /api/public/ban-appeals to submit a ban appeal."
    }))
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

/// 按 Steam 标识符查询该玩家的活跃封禁记录（供公开申诉页使用）
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

/// 查询全球封禁记录（优先从本地 global_bans 表查，本地无则代理第三方 API，带缓存）
pub(crate) async fn get_global_bans(
    State(ctx): State<AppCtx>,
    Path(steamid64): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    // 优先从本地 global_bans 表查询（全球封禁同步功能维护）
    let local_bans: Vec<(i64, String, Option<String>, Option<String>, Option<String>, Option<String>, Option<String>)> = sqlx::query_as(
        r#"SELECT kzt_ban_id, ban_type, notes, stats, expires_on, created_on, player_name
           FROM global_bans WHERE steam_id64 = $1 AND is_expired = false"#
    ).bind(&steamid64).fetch_all(&ctx.db.pool).await.unwrap_or_default();

    if !local_bans.is_empty() {
        let ban_items: Vec<serde_json::Value> = local_bans.into_iter().map(|(id, ban_type, notes, stats, expires_on, created_on, player_name)| {
            serde_json::json!({
                "id": id,
                "ban_type": ban_type,
                "notes": notes,
                "stats": stats,
                "expires_on": expires_on,
                "created_on": created_on,
                "player_name": player_name,
                "steamid64": steamid64.clone(),
            })
        }).collect();
        return Ok(Json(serde_json::json!(ban_items)));
    }

    // 本地无数据 → 检查缓存，再回退到第三方 API
    {
        let cache = ctx.global_bans_cache.read().await;
        if let Some((data, timestamp)) = cache.get(&steamid64) {
            if chrono::Utc::now() - *timestamp < chrono::Duration::minutes(30) {
                return Ok(Json(data.clone()));
            }
        }
    }

let data = fetch_global_bans_from_api(&steamid64).await.map_err(|e| {
        tracing::error!(error = ?e, steamid64 = %steamid64, "查询全球封禁失败");
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "查询失败" })),
        )
    })?;

    // 写入缓存
    {
        let mut cache = ctx.global_bans_cache.write().await;
        cache.insert(steamid64.clone(), (data.clone(), chrono::Utc::now()));
        // 清理过期缓存
        let now = chrono::Utc::now();
        cache.retain(|_, (_, ts)| now - *ts < chrono::Duration::minutes(30));
        // 硬上限：超出时移除最旧的条目
        if cache.len() > 500 {
            let mut entries: Vec<_> = cache
                .keys()
                .cloned()
                .zip(cache.values().map(|(_, ts)| *ts))
                .collect();
            entries.sort_by_key(|(_, ts)| *ts);
            let to_remove = cache.len() - 400;
            let keys_to_remove: Vec<_> = entries
                .into_iter()
                .take(to_remove)
                .map(|(k, _)| k)
                .collect();
            for key in keys_to_remove {
                cache.remove(&key);
            }
        }
    }

    Ok(Json(data))
}

/// 批量查询全球封禁记录（减少请求次数）
pub(crate) async fn get_global_bans_batch(
    State(ctx): State<AppCtx>,
    Json(body): Json<GlobalBansBatchBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    // 限制单次最多查询 30 个 ID，并去掉空值/重复值，避免浪费外部请求。
    let mut seen = HashSet::new();
    let steamids: Vec<String> = body
        .steamids
        .into_iter()
        .map(|steamid| steamid.trim().to_string())
        .filter(|steamid| !steamid.is_empty() && seen.insert(steamid.clone()))
        .take(30)
        .collect();
    let mut results: HashMap<String, serde_json::Value> = HashMap::new();
    let mut to_fetch: Vec<String> = Vec::new();

    // 优先从本地 global_bans 表查询（全球封禁同步功能维护的数据）
    for steamid64 in &steamids {
        let local_bans: Vec<(i64, String, Option<String>, Option<String>, Option<String>, Option<String>, Option<String>)> = sqlx::query_as(
            r#"SELECT kzt_ban_id, ban_type, notes, stats, expires_on, created_on, player_name
               FROM global_bans WHERE steam_id64 = $1 AND is_expired = false"#
        ).bind(steamid64).fetch_all(&ctx.db.pool).await.unwrap_or_default();

        if !local_bans.is_empty() {
            // 本地有数据 → 直接使用，不请求第三方 API
            let ban_items: Vec<serde_json::Value> = local_bans.into_iter().map(|(id, ban_type, notes, stats, expires_on, created_on, player_name)| {
                serde_json::json!({
                    "id": id,
                    "ban_type": ban_type,
                    "notes": notes,
                    "stats": stats,
                    "expires_on": expires_on,
                    "created_on": created_on,
                    "player_name": player_name,
                    "steamid64": steamid64,
                })
            }).collect();
            results.insert(steamid64.clone(), serde_json::json!(ban_items));
            continue;
        }

        // 本地无数据 → 检查缓存，再回退到第三方 API
        {
            let cache = ctx.global_bans_cache.read().await;
            if let Some((data, timestamp)) = cache.get(steamid64) {
                if chrono::Utc::now() - *timestamp < chrono::Duration::minutes(30) {
                    results.insert(steamid64.clone(), data.clone());
                    continue;
                }
            }
        }
        to_fetch.push(steamid64.clone());
    }

    // 批量请求（限制并发数，最多同时 10 个 SteamID 查询）
    if !to_fetch.is_empty() {
        let fetch_ids = to_fetch;
        let fetch_ids_for_timeout = fetch_ids.clone();
        let results_vec = tokio::time::timeout(std::time::Duration::from_secs(15), async {
            let stream = futures::stream::iter(fetch_ids.into_iter().map(|id| async move {
                let result = fetch_global_bans_from_api(&id).await;
                (id, result)
            }));
            stream.buffer_unordered(10).collect::<Vec<_>>().await
        })
        .await
        .unwrap_or_else(|_| {
            tracing::warn!("global bans batch query timed out after 15s");
            fetch_ids_for_timeout
                .into_iter()
                .map(|s| (s, Err(())))
                .collect()
        });

        // 写入缓存和结果
        let mut cache = ctx.global_bans_cache.write().await;
        for (steamid64, result) in results_vec {
            match result {
                Ok(data) => {
                    results.insert(steamid64.clone(), data.clone());
                    cache.insert(steamid64, (data, chrono::Utc::now()));
                }
                Err(_) => {
                    results.insert(steamid64, serde_json::json!({ "data": [], "count": 0 }));
                }
            }
        }
        // 清理过期和超量缓存
        let now = chrono::Utc::now();
        cache.retain(|_, (_, ts)| now - *ts < chrono::Duration::minutes(30));
        if cache.len() > 500 {
            let mut entries: Vec<_> = cache
                .keys()
                .cloned()
                .zip(cache.values().map(|(_, ts)| *ts))
                .collect();
            entries.sort_by_key(|(_, ts)| *ts);
            let to_remove = cache.len() - 400;
            let keys_to_remove: Vec<_> = entries
                .into_iter()
                .take(to_remove)
                .map(|(k, _)| k)
                .collect();
            for key in keys_to_remove {
                cache.remove(&key);
            }
        }
    }

    Ok(Json(serde_json::json!({ "results": results })))
}

/// 公开页提交封禁申诉
pub(crate) async fn submit_ban_appeal(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Json(body): Json<SubmitAppealBody>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<serde_json::Value>)> {
    let item = ban_appeal_service::create_appeal(
        &ctx.db,
        ban_appeal_service::CreateAppealInput {
            ban_id: body.ban_id,
            steam_id: body.steam_id,
            player_name: body.player_name,
            appeal_reason: body.appeal_reason,
        },
    )
    .await
    .map_err(invalid_request)?;

    let log_target = format!("{} ({})", item.player_name, item.steam_id);
    let _ = log_service::create_log(
        &ctx.db,
        "guest",
        "公共展示页",
        "提交封禁申诉",
        &log_target,
        &extract_client_ip(&headers),
    )
    .await;

    if let Err(e) = notification_service::notify_all_admins(
        &ctx.db,
        &ctx.notification_hub,
        None,
        "ban_appeal",
        "新封禁申诉",
        &format!("玩家 {} 提交了封禁申诉，请尽快审核。", item.player_name),
        Some("/ban-appeal"),
    )
    .await
    {
        tracing::warn!(%e, "ban appeal notification failed");
    }

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({
            "id": item.id,
            "appeal_id": item.id,
            "upload_token": item.upload_token,
            "item": item,
        })),
    ))
}

/// 查询玩家的申诉状态（公开接口，玩家通过 SteamID 查询自己的申诉记录和审核结果）
pub(crate) async fn query_appeal_status(
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

    let appeals = ban_appeal_service::query_appeals_by_steam_id(&ctx.db, &parsed.steamid64)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "查询申诉状态失败");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "查询申诉状态失败"})),
            )
        })?;

    Ok(Json(serde_json::json!({
        "steamid64": parsed.steamid64,
        "appeals": appeals,
    })))
}

async fn fetch_global_bans_json(url: String, timeout: Duration) -> Option<serde_json::Value> {
    use crate::http_client;

    tokio::time::timeout(timeout, async {
        let response = http_client::http_client().get(&url).send().await.ok()?;
        if !response.status().is_success() {
            return None;
        }
        response.json::<serde_json::Value>().await.ok()
    })
    .await
    .ok()
    .flatten()
}

/// 从第三方 API 获取封禁记录
async fn fetch_global_bans_from_api(steamid64: &str) -> Result<serde_json::Value, ()> {
    let timeout = Duration::from_secs(5);

    // 主 API（KZTimerGlobal）
    let primary_url = format!(
        "https://kztimerglobal.com/api/v2.0/bans?steamid64={}&limit=30&offset=0",
        steamid64
    );

    // 备用 API（GOKZ.TOP）
    let fallback_url = format!(
        "https://api.gokz.top/api/v1/bans?steamid64={}&is_expired=false&limit=100",
        steamid64
    );

    // 主备接口并发请求，避免主 API 慢或超时时再串行等待备用 API。
    let (primary, fallback) = tokio::join!(
        fetch_global_bans_json(primary_url, timeout),
        fetch_global_bans_json(fallback_url, timeout)
    );

    if let Some(data) = primary.or(fallback) {
        return Ok(data);
    }

    Err(())
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
async fn fetch_gokz_scope(
    steamid64: &str,
    scope: &str,
) -> Option<GokzModeStats> {
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
        obj.insert(params.scope.to_uppercase(), serde_json::to_value(mode_stats).unwrap_or(serde_json::Value::Null));
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
    obj.insert(params.scope.to_uppercase(), serde_json::to_value(&data).unwrap_or(serde_json::Value::Null));
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
    if let Some(s) = results.get(0).and_then(|r| r.clone()) {
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
    let uncached: Vec<String> = body.steamid64s
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
        response.insert(steamid64, serde_json::json!({
            "KZT": stats.kzt,
            "SKZ": stats.skz,
            "VNL": stats.vnl,
            "OVR": stats.ovr,
        }));
    }

    Ok(Json(serde_json::Value::Object(response)))
}

/// 上传申诉辅助文件（录像、图片、录音）到 R2
pub(crate) async fn upload_appeal_files(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(appeal_id): Path<Uuid>,
    mut multipart: Multipart,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let r2 = ctx.r2_storage.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": "文件上传服务未配置"})),
        )
    })?;

    // 验证申诉存在且为 pending 状态
    let appeal_exists: Option<(String, Option<String>)> =
        sqlx::query_as("SELECT status, upload_token_hash FROM ban_appeals WHERE id = $1")
            .bind(appeal_id)
            .fetch_optional(&ctx.db.pool)
            .await
            .map_err(|_| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": "查询申诉失败"})),
                )
            })?;

    let (status, upload_token_hash) = appeal_exists.ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "申诉记录不存在"})),
        )
    })?;

    let upload_token = headers
        .get("x-appeal-upload-token")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    if !ban_appeal_service::verify_upload_token(upload_token_hash.as_deref(), upload_token) {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "上传凭证无效，请重新提交申诉"})),
        ));
    }

    if status != "pending" {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "该申诉已被处理，无法上传文件"})),
        ));
    }

    let max_size = ctx.config.appeal_file_max_size_bytes;
    let mut uploaded: Vec<serde_json::Value> = Vec::new();
    let mut errors: Vec<String> = Vec::new();

    loop {
        let field = match multipart.next_field().await {
            Ok(Some(field)) => field,
            Ok(None) => break,
            Err(e) => {
                tracing::warn!(%e, "读取 multipart 字段失败");
                errors.push("读取上传内容失败".to_string());
                break;
            }
        };

        let file_name = match field.file_name() {
            Some(name) => name.to_string(),
            None => continue,
        };

        if !r2_storage::is_allowed_file_type(&file_name) {
            errors.push(format!("不支持的文件类型: {file_name}"));
            continue;
        }

        let content_type = field
            .content_type()
            .map(|c| c.to_string())
            .unwrap_or_else(|| r2_storage::guess_content_type(&file_name).to_string());

        let data = match field.bytes().await {
            Ok(bytes) => bytes.to_vec(),
            Err(e) => {
                errors.push(format!("读取文件失败: {file_name} - {e}"));
                continue;
            }
        };
        let file_size = data.len();

        if file_size > max_size {
            errors.push(format!(
                "文件 {} 超出大小限制（最大 {}MB）",
                file_name,
                max_size / 1024 / 1024
            ));
            continue;
        }

        if file_size == 0 {
            errors.push(format!("文件为空: {file_name}"));
            continue;
        }

        // 上传到 R2
        match r2.upload(appeal_id, &file_name, &content_type, data).await {
            Ok(key) => {
                let category = r2_storage::file_category(&file_name);

                // 将文件记录写入数据库
                if let Err(e) = sqlx::query(
                    r#"INSERT INTO appeal_files (id, appeal_id, file_name, file_size, content_type, storage_key, category)
                       VALUES ($1, $2, $3, $4, $5, $6, $7)"#,
                )
                .bind(Uuid::new_v4())
                .bind(appeal_id)
                .bind(&file_name)
                .bind(file_size as i64)
                .bind(&content_type)
                .bind(&key)
                .bind(category)
                .execute(&ctx.db.pool)
                .await
                {
                    tracing::warn!(%e, "写入文件记录失败");
                    // 文件已上传到 R2，数据库记录失败不影响上传结果
                }

                uploaded.push(serde_json::json!({
                    "file_name": file_name,
                    "file_size": file_size,
                    "category": category,
                }));
            }
            Err(e) => {
                tracing::error!(%e, "R2 upload failed for {file_name}");
                errors.push(format!("上传文件 {file_name} 失败，请稍后重试"));
            }
        }
    }

    if uploaded.is_empty() && errors.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "未选择可上传的文件"})),
        ));
    }

    if uploaded.is_empty() && !errors.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "所有文件上传失败",
                "errors": errors,
            })),
        ));
    }

    Ok(Json(serde_json::json!({
        "uploaded": uploaded,
        "errors": if errors.is_empty() { None } else { Some(errors) },
    })))
}

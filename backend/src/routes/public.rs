use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use std::collections::HashMap;
use std::time::Duration;
use futures::stream::StreamExt;
use crate::routes::{AppCtx, ListQuery, invalid_request};
use crate::services::{whitelist_service, public_service, log_service, rate_limit_service::extract_client_ip};

#[derive(serde::Deserialize)]
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

pub(crate) async fn public_whitelist(State(ctx): State<AppCtx>, Query(query): Query<ListQuery>) -> Result<Json<serde_json::Value>, StatusCode> {
    let result = public_service::list_public_whitelist(&ctx.db, &query)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::json!({ "items": result.items, "total": result.total, "page": result.page, "page_size": result.page_size })))
}

pub(crate) async fn submit_whitelist(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Json(body): Json<WhitelistBody>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<serde_json::Value>)> {
    let resolver = whitelist_service::steam_resolver(&ctx.config);
    let item = whitelist_service::create_public_whitelist_request(
        &ctx.db,
        whitelist_service::PublicWhitelistRequestInput {
            nickname: body.nickname,
            steam_input: body.steam_input,
        },
        &resolver,
    )
    .await
    .map_err(invalid_request)?;
    let _ = log_service::create_log(&ctx.db, "guest", "公共展示页", "提交白名单申请", &item.nickname, &extract_client_ip(&headers)).await;
    Ok((StatusCode::CREATED, Json(serde_json::json!({"item": item}))))
}

pub(crate) async fn public_bans(State(ctx): State<AppCtx>, Query(query): Query<ListQuery>) -> Result<Json<serde_json::Value>, StatusCode> {
    let result = public_service::list_public_bans(&ctx.db, &query)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let stats = public_service::ban_stats(&ctx.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::json!({ "items": result.items, "total": result.total, "page": result.page, "page_size": result.page_size, "stats": stats })))
}

pub(crate) async fn resolve_steam(
    State(ctx): State<AppCtx>,
    Json(body): Json<ResolveSteamBody>,
) -> Result<Json<SteamResolveResponse>, (StatusCode, Json<serde_json::Value>)> {
    let resolver = whitelist_service::steam_resolver(&ctx.config);

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
    ).await {
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

/// 查询全球封禁记录（代理 API，解决 CORS 问题，带缓存）
pub(crate) async fn get_global_bans(
    State(ctx): State<AppCtx>,
    Path(steamid64): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    // 检查缓存（缓存30分钟，减少外部 API 调用）
    {
        let cache = ctx.global_bans_cache.read().await;
        if let Some((data, timestamp)) = cache.get(&steamid64) {
            if chrono::Utc::now() - *timestamp < chrono::Duration::minutes(30) {
                return Ok(Json(data.clone()));
            }
        }
    }

    let data = fetch_global_bans_from_api(&steamid64).await
        .map_err(|_| (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": "查询失败" }))))?;

    // 写入缓存
    {
        let mut cache = ctx.global_bans_cache.write().await;
        cache.insert(steamid64.clone(), (data.clone(), chrono::Utc::now()));
        // 清理过期缓存
        let now = chrono::Utc::now();
        cache.retain(|_, (_, ts)| now - *ts < chrono::Duration::minutes(30));
        // 硬上限：超出时移除最旧的条目
        if cache.len() > 500 {
            let mut entries: Vec<_> = cache.keys().cloned().zip(cache.values().map(|(_, ts)| *ts)).collect();
            entries.sort_by_key(|(_, ts)| *ts);
            let to_remove = cache.len() - 400;
            let keys_to_remove: Vec<_> = entries.into_iter().take(to_remove).map(|(k, _)| k).collect();
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
    // 限制单次最多查询 30 个 ID
    let steamids: Vec<String> = body.steamids.into_iter().take(30).collect();
    let mut results: HashMap<String, serde_json::Value> = HashMap::new();
    let mut to_fetch: Vec<String> = Vec::new();

    // 先检查缓存
    {
        let cache = ctx.global_bans_cache.read().await;
        for steamid64 in &steamids {
            if let Some((data, timestamp)) = cache.get(steamid64) {
                if chrono::Utc::now() - *timestamp < chrono::Duration::minutes(30) {
                    results.insert(steamid64.clone(), data.clone());
                } else {
                    to_fetch.push(steamid64.clone());
                }
            } else {
                to_fetch.push(steamid64.clone());
            }
        }
    }

    // 批量请求（限制并发数，最多同时 8 个外部请求）
    if !to_fetch.is_empty() {
        let fetch_ids = to_fetch;
        let fetch_count = fetch_ids.len();
        let fetch_ids_for_timeout = fetch_ids.clone();
        let results_vec = tokio::time::timeout(
            std::time::Duration::from_secs(15),
            async {
                let stream = futures::stream::iter(
                    fetch_ids.into_iter().map(|id| async move {
                        let result = fetch_global_bans_from_api(&id).await;
                        (id, result)
                    })
                );
                stream.buffer_unordered(8).collect::<Vec<_>>().await
            },
        )
        .await
        .unwrap_or_else(|_| {
            tracing::warn!("global bans batch query timed out after 15s");
            fetch_ids_for_timeout.into_iter().map(|s| (s, Err(()))).collect()
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
            let mut entries: Vec<_> = cache.keys().cloned().zip(cache.values().map(|(_, ts)| *ts)).collect();
            entries.sort_by_key(|(_, ts)| *ts);
            let to_remove = cache.len() - 400;
            let keys_to_remove: Vec<_> = entries.into_iter().take(to_remove).map(|(k, _)| k).collect();
            for key in keys_to_remove {
                cache.remove(&key);
            }
        }
    }

    Ok(Json(serde_json::json!({ "results": results })))
}

/// 从第三方 API 获取封禁记录
async fn fetch_global_bans_from_api(
    steamid64: &str,
) -> Result<serde_json::Value, ()> {
    use crate::http_client::HTTP_CLIENT;

    let timeout = std::time::Duration::from_secs(5);

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

    // 先尝试主 API
    if let Ok(response) = tokio::time::timeout(timeout, HTTP_CLIENT.get(&primary_url).send()).await {
        if let Ok(response) = response {
            if response.status().is_success() {
                if let Ok(Ok(data)) = tokio::time::timeout(timeout, response.json::<serde_json::Value>()).await {
                    return Ok(data);
                }
            }
        }
    }

    // 主 API 失败，尝试备用 API
    if let Ok(response) = tokio::time::timeout(timeout, HTTP_CLIENT.get(&fallback_url).send()).await {
        if let Ok(response) = response {
            if response.status().is_success() {
                if let Ok(Ok(data)) = tokio::time::timeout(timeout, response.json::<serde_json::Value>()).await {
                    return Ok(data);
                }
            }
        }
    }

    Err(())
}

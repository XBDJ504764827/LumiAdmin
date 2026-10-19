use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use chrono::Utc;
use serde::Deserialize;

use crate::routes::{
    current_operator, forbidden, internal_error, invalid_request, invalid_request_status, AppCtx,
    AppError,
};
use crate::services::{
    access_log_service, access_service, access_snapshot_service, community_service,
    permission_service,
};

#[derive(Deserialize)]
pub(crate) struct PluginAccessCheckBody {
    report_token: String,
    port: i32,
    steam_id64: String,
    ip_address: Option<String>,
    player: Option<String>,
    server_port: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct PluginAccessSnapshotBody {
    report_token: String,
    port: i32,
    /// 可选：插件上次收到的快照版本。命中时后端省略 item，仅返回 unchanged=true。
    #[serde(default)]
    etag: Option<String>,
}

pub(crate) async fn check_plugin_access(
    State(ctx): State<AppCtx>,
    Json(body): Json<PluginAccessCheckBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let result = access_service::check_access(
        &ctx.db,
        &ctx.config,
        &ctx.access_snapshot,
        &ctx.server_config_cache,
        &ctx.active_ban_cache,
        &ctx.whitelist_cache,
        access_service::AccessCheckInput {
            report_token: body.report_token.clone(),
            port: body.port,
            steam_id64: body.steam_id64.clone(),
            ip_address: body.ip_address.clone(),
            player: body.player.clone(),
            server_port: body.server_port,
        },
    )
    .await
    .map_err(invalid_request)?;

    // 记录进服日志（无论成功/失败）
    if let Ok(Some(server)) = ctx
        .server_config_cache
        .get_by_token_port(&ctx.db, &body.report_token, body.port)
        .await
    {
        let access_method_str = result
            .access_method
            .clone()
            .unwrap_or_else(|| "unknown".to_string());
        let community_name = community_service::find_group_name(&ctx.db, server.community_id)
            .await
            .ok();
        let access_method = access_log_service::AccessMethod::from_str(&access_method_str);
        let reject_reason = if result.allowed {
            None
        } else {
            result
                .audit_message
                .as_deref()
                .or(Some(result.message.as_str()))
        };
        if let Err(e) = access_log_service::create_access_log(
            &ctx.db,
            &body.steam_id64,
            body.player.as_deref(),
            body.ip_address.as_deref(),
            server.id,
            &server.name,
            server.port,
            server.community_id,
            community_name.as_deref(),
            result.allowed,
            &access_method,
            result.failure_code.as_deref(),
            reject_reason,
            result.rating,
            result.steam_level,
        )
        .await
        {
            tracing::warn!(
                error = %e,
                steam_id64 = %body.steam_id64,
                server_port = server.port,
                "写入进服日志失败（access/check）"
            );
        }
    }

    Ok(Json(serde_json::json!({ "result": result })))
}

/// 游戏插件本地裁决结果上报。
///
/// LumiAuth 本地自治（Data Plane）后，进服主链路由插件读取本地快照同步裁决，
/// 不再逐个玩家调用 `/api/plugin/access/check`。插件在本地裁决完成后调用本接口
/// 上报结果，供管理后台「进服监控」展示（成功与拒绝原因）。
#[derive(Deserialize)]
pub(crate) struct PluginAccessRecordBody {
    report_token: String,
    port: i32,
    steam_id64: String,
    ip_address: Option<String>,
    player: Option<String>,
    /// 是否放行
    allowed: bool,
    /// 进服方式（access_method 字符串，如 whitelist / banned / risk_blocked 等）
    access_method: Option<String>,
    /// 失败码（可选，如 banned / whitelist_rejected 等）
    failure_code: Option<String>,
    /// 拒绝原因（可选，拒绝时展示）
    reject_reason: Option<String>,
    rating: Option<i32>,
    steam_level: Option<i32>,
}

pub(crate) async fn record_plugin_access(
    State(ctx): State<AppCtx>,
    Json(body): Json<PluginAccessRecordBody>,
) -> Result<Json<serde_json::Value>, AppError> {
    let server = ctx
        .server_config_cache
        .get_by_token_port(&ctx.db, &body.report_token, body.port)
        .await
        .map_err(AppError::internal)?
        .ok_or_else(|| AppError::bad_request(anyhow::anyhow!("服务器令牌或端口不匹配")))?;

    let community_name = community_service::find_group_name(&ctx.db, server.community_id)
        .await
        .ok();
    let access_method_str = body
        .access_method
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(if body.allowed {
            "unrestricted"
        } else {
            "unknown"
        });
    let access_method = access_log_service::AccessMethod::from_str(access_method_str);
    // 空字符串归一为 NULL，避免成功记录里出现空的失败原因
    let failure_code = crate::services::normalize_optional_text(body.failure_code.as_deref());
    let reject_reason = crate::services::normalize_optional_text(body.reject_reason.as_deref());

    access_log_service::create_access_log(
        &ctx.db,
        &body.steam_id64,
        body.player.as_deref(),
        body.ip_address.as_deref(),
        server.id,
        &server.name,
        server.port,
        server.community_id,
        community_name.as_deref(),
        body.allowed,
        &access_method,
        failure_code.as_deref(),
        reject_reason.as_deref(),
        body.rating,
        body.steam_level,
    )
    .await
    .map_err(|e| {
        tracing::warn!(
            error = %e,
            steam_id64 = %body.steam_id64,
            server_port = server.port,
            "写入进服日志失败（access/record）"
        );
        AppError::internal(e)
    })?;

    spawn_restriction_profile_refresh(
        &ctx,
        &body.steam_id64,
        !body.allowed,
        failure_code.as_deref(),
    );

    Ok(Json(serde_json::json!({ "ok": true })))
}

/// 进服记录上报时异步刷新玩家准入资料（rating + Steam 等级）。
///
/// 本地裁决不再逐玩家在线复核，快照 access_profiles 的唯一写入链路是
/// player_access_cache；这里用 tokio spawn 保证 record 响应不被外部 API 延迟阻塞。
/// 限制类拒绝走强制复核（绕过缓存有效期），成功写入后立即重建快照，
/// 让插件在下一次快照拉取内拿到新鲜资料，实现「拒绝一次、重进即过」的自愈。
fn spawn_restriction_profile_refresh(
    ctx: &AppCtx,
    steam_id64: &str,
    denied: bool,
    failure_code: Option<&str>,
) {
    if denied && access_service::is_restriction_failure_code(failure_code) {
        let db = ctx.db.clone();
        let config = ctx.config.clone();
        let snapshot_store = ctx.access_snapshot.clone();
        let steam_id64 = steam_id64.to_string();
        tokio::spawn(async move {
            match access_service::force_refresh_player_profile(&db, &config, &steam_id64, false)
                .await
            {
                Ok(true) => {
                    access_snapshot_service::request_immediate_snapshot_refresh(db, snapshot_store);
                }
                Ok(false) => {}
                Err(error) => {
                    tracing::warn!(
                        %error,
                        steamid64 = %steam_id64,
                        "拒绝后强制刷新进服资料失败"
                    );
                }
            }
        });
        return;
    }
    let db = ctx.db.clone();
    let config = ctx.config.clone();
    let steam_id64 = steam_id64.to_string();
    tokio::spawn(async move {
        access_service::refresh_player_profile(&db, &config, &steam_id64).await;
    });
}

/// 单玩家进服资料点查（插件缺资料时的同步直取，配合进服 3s 宽限）。
///
/// 命中缓存或有界拉取成功即返回资料；拿不到返回 404，插件按零容忍踢出。
/// 404 的同时补一次异步强刷：本次进不去的玩家重进时缓存大概率已就绪。
#[derive(Deserialize)]
pub(crate) struct PluginAccessProfileBody {
    report_token: String,
    port: i32,
    steam_id64: String,
}

pub(crate) async fn plugin_access_profile(
    State(ctx): State<AppCtx>,
    Json(body): Json<PluginAccessProfileBody>,
) -> Result<Json<serde_json::Value>, AppError> {
    let _server = ctx
        .server_config_cache
        .get_by_token_port(&ctx.db, &body.report_token, body.port)
        .await
        .map_err(AppError::internal)?
        .ok_or_else(|| AppError::bad_request(anyhow::anyhow!("服务器令牌或端口不匹配")))?;

    let steam_id64 = body.steam_id64.trim().to_string();
    match access_service::fetch_player_profile_bounded(&ctx.db, &ctx.config, &steam_id64).await {
        Ok(Some((profile, expires_at))) => Ok(Json(serde_json::json!({
            "ok": true,
            "item": {
                "steam_id64": steam_id64,
                "rating": profile.rating,
                "steam_level": profile.steam_level,
                "expires_at_unix": expires_at.timestamp(),
            }
        }))),
        Ok(None) => {
            let db = ctx.db.clone();
            let config = ctx.config.clone();
            tokio::spawn(async move {
                let _ =
                    access_service::force_refresh_player_profile(&db, &config, &steam_id64, false)
                        .await;
            });
            Err(AppError::not_found("玩家进服资料暂不可用"))
        }
        Err(error) => Err(AppError::bad_request(error)),
    }
}

/// 单玩家白名单实时点查（插件本地判白名单缺失时的同步回源，配合进服宽限）。
///
/// 直查 DB（`whitelist_requests.status='approved'` + 有效封禁），不读快照文件，
/// 因此比快照拉取新鲜一个数量级。查到已通过返回 `whitelisted=true`，插件直接
/// 合并进本地白名单表；查到有效封禁返回 `banned=true`，插件立即踢出；
/// 均未命中返回 `whitelisted=false, banned=false`，插件等宽限到期后按零容忍踢出。
#[derive(Deserialize)]
pub(crate) struct PluginWhitelistLiveBody {
    report_token: String,
    port: i32,
    steam_id64: String,
}

pub(crate) async fn plugin_whitelist_live(
    State(ctx): State<AppCtx>,
    Json(body): Json<PluginWhitelistLiveBody>,
) -> Result<Json<serde_json::Value>, AppError> {
    let _server = ctx
        .server_config_cache
        .get_by_token_port(&ctx.db, &body.report_token, body.port)
        .await
        .map_err(AppError::internal)?
        .ok_or_else(|| AppError::bad_request(anyhow::anyhow!("服务器令牌或端口不匹配")))?;

    let steam_id64 = body.steam_id64.trim().to_string();
    if steam_id64.len() != 17 || !steam_id64.chars().all(|ch| ch.is_ascii_digit()) {
        return Err(AppError::bad_request(anyhow::anyhow!("SteamID64 不合法")));
    }

    let ban_reason: Option<String> = sqlx::query_scalar::<_, String>(
        r#"SELECT reason FROM ban_records
           WHERE status = 'active' AND steam_id = $1
             AND (expires_at IS NULL OR expires_at > now())
           ORDER BY created_at DESC LIMIT 1"#,
    )
    .bind(&steam_id64)
    .fetch_optional(&ctx.db.pool)
    .await
    .map_err(|e| AppError::internal(e.into()))?;
    if let Some(reason) = ban_reason {
        return Ok(Json(serde_json::json!({
            "ok": true,
            "item": {
                "steam_id64": steam_id64,
                "whitelisted": false,
                "banned": true,
                "reason": reason,
            }
        })));
    }

    let whitelisted: bool = sqlx::query_scalar::<_, String>(
        r#"SELECT steamid64 FROM whitelist_requests
           WHERE steamid64 = $1 AND status = 'approved' LIMIT 1"#,
    )
    .bind(&steam_id64)
    .fetch_optional(&ctx.db.pool)
    .await
    .map_err(|e| AppError::internal(e.into()))?
    .is_some();

    Ok(Json(serde_json::json!({
        "ok": true,
        "item": {
            "steam_id64": steam_id64,
            "whitelisted": whitelisted,
            "banned": false,
        }
    })))
}

pub(crate) async fn plugin_access_snapshot(
    State(ctx): State<AppCtx>,
    Json(body): Json<PluginAccessSnapshotBody>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    // 先校验服务器归属（token+port），再解析它所属的游戏节点。
    let server = crate::services::plugin_ban_service::ServerAuth::authenticate(
        &ctx.db,
        body.port,
        &body.report_token,
    )
    .await
    .map_err(|e| {
        tracing::warn!(error = %e, "plugin access snapshot: server auth failed");
        invalid_request_status(e)
    })?;
    let node_id = crate::services::game_node_service::resolve_or_assign_node(&ctx.db, server.id)
        .await
        .map_err(internal_error)?
        .ok_or(StatusCode::PRECONDITION_FAILED)?;

    let snapshot = match ctx
        .access_snapshot
        .read_snapshot()
        .await
        .map_err(internal_error)?
    {
        Some(snapshot) => snapshot,
        None => return Err(StatusCode::SERVICE_UNAVAILABLE),
    };
    // 节点快照：该机器上所有服的规则 + 全局 bans/whitelist/profiles。
    let item = access_snapshot_service::snapshot_for_node(&snapshot, node_id, Utc::now())
        .map_err(invalid_request_status)?;

    // etag 增量：插件回传的版本与当前快照一致时跳过全量传输。
    let unchanged = body
        .etag
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_some_and(|value| value == snapshot.version);

    if unchanged {
        return Ok(Json(serde_json::json!({
            "unchanged": true,
            "version": snapshot.version,
        })));
    }

    Ok(Json(serde_json::json!({ "item": item })))
}

pub(crate) async fn list_access_logs(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Query(params): Query<access_log_service::AccessLogQueryParams>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_view_access_logs(&actor) {
        return Err(forbidden());
    }

    let page = params.page();
    let page_size = params.page_size();

    match access_log_service::query_access_logs(&ctx.db, &params).await {
        Ok((items, total)) => Ok(Json(serde_json::json!({
            "items": items,
            "total": total,
            "page": page,
            "page_size": page_size,
        }))),
        Err(e) => {
            tracing::error!(%e, "查询进服记录失败");
            Ok(Json(serde_json::json!({
                "error": "查询进服记录失败",
                "items": [],
                "total": 0,
                "page": page,
                "page_size": page_size,
            })))
        }
    }
}

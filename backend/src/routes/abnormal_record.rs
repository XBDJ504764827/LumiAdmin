use axum::{
    body::Bytes,
    extract::{Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    Json,
};
use uuid::Uuid;

use crate::routes::{current_operator, forbidden, invalid_request, AppCtx};
use crate::services::rate_limit_service::extract_client_ip;
use crate::services::{
    abnormal_record_service, external_ban_api_service, log_service, notification_service,
    permission_service, plugin_ban_service,
};

#[derive(serde::Deserialize)]
pub(crate) struct PollApprovedBody {
    report_token: String,
    port: i32,
    limit: Option<i64>,
}

fn can_manage(
    actor: &crate::models::Operator,
) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
    if permission_service::can_manage_abnormal_records(actor) {
        Ok(())
    } else {
        Err(forbidden())
    }
}

pub(crate) async fn list_records(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Query(query): Query<abnormal_record_service::AbnormalRecordListQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    can_manage(&actor)?;

    let result = abnormal_record_service::list_records(&ctx.db, &query)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "加载异常记录失败");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "加载异常记录失败" })),
            )
        })?;
    Ok(Json(serde_json::json!({
        "items": result.items,
        "total": result.total,
        "page": result.page,
        "page_size": result.page_size,
    })))
}

pub(crate) async fn get_record(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    can_manage(&actor)?;

    let item = abnormal_record_service::get_record(&ctx.db, id)
        .await
        .map_err(invalid_request)?;
    Ok(Json(serde_json::json!({ "item": item })))
}

pub(crate) async fn get_replay_url(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    can_manage(&actor)?;

    let url = abnormal_record_service::replay_url(&ctx.db, ctx.r2_storage.as_ref(), id)
        .await
        .map_err(invalid_request)?;
    Ok(Json(serde_json::json!({ "url": url })))
}

pub(crate) async fn approve_record(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(body): Json<abnormal_record_service::ReviewInput>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    can_manage(&actor)?;

    let item = abnormal_record_service::approve_record(&ctx.db, id, &actor.display_name, body)
        .await
        .map_err(invalid_request)?;

    let _ = log_service::create_log(
        &ctx.db,
        &actor.display_name,
        "异常记录审核",
        "放行异常记录",
        &format!(
            "{} ({}) {} {:.3}s",
            item.player_name.as_deref().unwrap_or("-"),
            item.steam_id64,
            item.map_name,
            item.run_time_seconds
        ),
        &extract_client_ip(&headers),
    )
    .await;

    Ok(Json(serde_json::json!({ "item": item })))
}

pub(crate) async fn reject_record(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(body): Json<abnormal_record_service::ReviewInput>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    can_manage(&actor)?;

    let item = abnormal_record_service::reject_record(&ctx.db, id, &actor.display_name, body)
        .await
        .map_err(invalid_request)?;

    let _ = log_service::create_log(
        &ctx.db,
        &actor.display_name,
        "异常记录审核",
        "作废异常记录",
        &format!(
            "{} ({}) {} {:.3}s",
            item.player_name.as_deref().unwrap_or("-"),
            item.steam_id64,
            item.map_name,
            item.run_time_seconds
        ),
        &extract_client_ip(&headers),
    )
    .await;

    Ok(Json(serde_json::json!({ "item": item })))
}

pub(crate) async fn reject_and_ban(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(body): Json<abnormal_record_service::RejectAndBanInput>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    can_manage(&actor)?;

    let (item, ban) = abnormal_record_service::reject_and_ban(
        &ctx.db,
        &ctx.config,
        id,
        &actor.display_name,
        body,
    )
    .await
    .map_err(invalid_request)?;

    let _ = log_service::create_log(
        &ctx.db,
        &actor.display_name,
        "异常记录审核",
        "作废异常记录并封禁",
        &format!(
            "{} ({}) {} {:.3}s | 封禁 {}",
            item.player_name.as_deref().unwrap_or("-"),
            item.steam_id64,
            item.map_name,
            item.run_time_seconds,
            ban.id
        ),
        &extract_client_ip(&headers),
    )
    .await;

    if let Err(e) = notification_service::notify_ban_create(
        &ctx.db,
        &ctx.notification_hub,
        &actor.id,
        &actor.display_name,
        ban.player.as_deref(),
        &ban.steam_id,
        &ban.reason,
    )
    .await
    {
        tracing::warn!(%e, "abnormal record ban notification failed");
    }
    external_ban_api_service::sync_ban_if_enabled(&ctx.db, &ban).await;

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({ "item": item, "ban": ban })),
    ))
}

pub(crate) async fn retry_submit(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    can_manage(&actor)?;

    let item = abnormal_record_service::retry_submit(&ctx.db, id, &actor.display_name)
        .await
        .map_err(invalid_request)?;
    Ok(Json(serde_json::json!({ "item": item })))
}

pub(crate) async fn list_rules(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Query(query): Query<abnormal_record_service::RuleListQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    can_manage(&actor)?;

    let result = abnormal_record_service::list_rules(&ctx.db, &query)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "加载异常时间规则失败");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "加载异常时间规则失败" })),
            )
        })?;
    Ok(Json(serde_json::json!({
        "items": result.items,
        "total": result.total,
        "page": result.page,
        "page_size": result.page_size,
    })))
}

pub(crate) async fn create_rule(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Json(body): Json<abnormal_record_service::RuleInput>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    can_manage(&actor)?;

    let item = abnormal_record_service::create_rule(&ctx.db, &actor.display_name, body)
        .await
        .map_err(invalid_request)?;
    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({ "item": item })),
    ))
}

pub(crate) async fn update_rule(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(body): Json<abnormal_record_service::RuleInput>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    can_manage(&actor)?;

    let item = abnormal_record_service::update_rule(&ctx.db, id, &actor.display_name, body)
        .await
        .map_err(invalid_request)?;
    Ok(Json(serde_json::json!({ "item": item })))
}

pub(crate) async fn delete_rule(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    can_manage(&actor)?;

    let deleted = abnormal_record_service::delete_rule(&ctx.db, id)
        .await
        .map_err(invalid_request)?;
    Ok(Json(serde_json::json!({ "deleted": deleted })))
}

pub(crate) async fn plugin_rules(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let _server = authenticate_plugin_headers(&ctx, &headers).await?;
    let items = abnormal_record_service::list_enabled_rules(&ctx.db)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "加载插件异常时间规则失败");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "加载规则失败" })),
            )
        })?;
    Ok(Json(serde_json::json!({ "items": items })))
}

pub(crate) async fn plugin_create_record(
    State(ctx): State<AppCtx>,
    Json(body): Json<abnormal_record_service::CreatePluginAbnormalRecordInput>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<serde_json::Value>)> {
    let item = abnormal_record_service::create_plugin_record(&ctx.db, body)
        .await
        .map_err(invalid_request)?;

    if item.status == "pending" {
        if let Err(e) = notification_service::notify_all_admins(
            &ctx.db,
            &ctx.notification_hub,
            None,
            "abnormal_record",
            "新异常记录",
            &format!(
                "{} 在 {} 提交了 {:.3}s 的异常成绩，请审核。",
                item.player_name.as_deref().unwrap_or(&item.steam_id64),
                item.map_name,
                item.run_time_seconds
            ),
            Some("/abnormal-records?status=pending"),
        )
        .await
        {
            tracing::warn!(%e, "abnormal record notification failed");
        }
    }

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({ "item": item })),
    ))
}

pub(crate) async fn plugin_upload_replay(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    body: Bytes,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let r2 = ctx.r2_storage.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({ "error": "文件上传服务未配置" })),
        )
    })?;
    let report_token = header_value(&headers, "x-cngokz-report-token").ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "缺少服务器上传凭证" })),
        )
    })?;
    let port = header_value(&headers, "x-cngokz-server-port")
        .and_then(|value| value.parse::<i32>().ok())
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "服务器端口无效" })),
            )
        })?;
    let file_name = header_value(&headers, "x-cngokz-file-name").unwrap_or("run.replay");
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok());
    let file_size = body.len() as i64;

    let item = abnormal_record_service::upload_replay(
        &ctx.db,
        r2,
        abnormal_record_service::ReplayUploadInput {
            record_id: id,
            report_token,
            port,
            file_name,
            content_type,
            data: body.to_vec(),
        },
    )
    .await
    .map_err(invalid_request)?;
    let _ = abnormal_record_service::update_replay_metadata_size(&ctx.db, id, file_size).await;

    Ok(Json(serde_json::json!({ "item": item })))
}

pub(crate) async fn plugin_poll_approved(
    State(ctx): State<AppCtx>,
    Json(body): Json<PollApprovedBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let result = abnormal_record_service::poll_approved(
        &ctx.db,
        &body.report_token,
        body.port,
        body.limit.unwrap_or(5),
    )
    .await
    .map_err(invalid_request)?;
    Ok(Json(serde_json::json!({ "items": result.items })))
}

pub(crate) async fn plugin_submit_result(
    State(ctx): State<AppCtx>,
    Path(id): Path<Uuid>,
    Json(body): Json<abnormal_record_service::SubmitResultInput>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let item = abnormal_record_service::submit_result(&ctx.db, id, body)
        .await
        .map_err(invalid_request)?;

    if item.global_submit_status == "failed" {
        if let Err(e) = notification_service::notify_all_admins(
            &ctx.db,
            &ctx.notification_hub,
            None,
            "abnormal_record",
            "异常记录补交失败",
            &format!(
                "{} 的 {} 异常记录补交全球失败。",
                item.player_name.as_deref().unwrap_or(&item.steam_id64),
                item.map_name
            ),
            Some("/abnormal-records?global_submit_status=failed"),
        )
        .await
        {
            tracing::warn!(%e, "abnormal record submit failed notification failed");
        }
    }

    Ok(Json(serde_json::json!({ "item": item })))
}

fn header_value<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name).and_then(|value| value.to_str().ok())
}

async fn authenticate_plugin_headers(
    ctx: &AppCtx,
    headers: &HeaderMap,
) -> Result<plugin_ban_service::ServerAuth, (StatusCode, Json<serde_json::Value>)> {
    let report_token = header_value(headers, "x-cngokz-report-token").ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "缺少服务器上传凭证" })),
        )
    })?;
    let port = header_value(headers, "x-cngokz-server-port")
        .and_then(|value| value.parse::<i32>().ok())
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "服务器端口无效" })),
            )
        })?;

    plugin_ban_service::ServerAuth::authenticate(&ctx.db, port, report_token)
        .await
        .map_err(|e| {
            (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
        })
}

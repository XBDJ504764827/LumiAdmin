use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use serde::Deserialize;
use serde::Serialize;
use uuid::Uuid;

use crate::routes::{current_operator, forbidden, invalid_request, AppCtx};
use crate::services::{
    log_service, permission_service, player_detail_service, rate_limit_service::extract_client_ip,
};

#[derive(Deserialize)]
pub(crate) struct PlayerDetailQuery {
    pub steam_input: String,
}

#[derive(Deserialize)]
pub(crate) struct PlayerSearchQuery {
    pub query: String,
}

#[derive(Deserialize)]
pub(crate) struct PlayerInternalBody {
    pub note: Option<String>,
    pub tags: Vec<String>,
}

#[derive(Deserialize)]
pub(crate) struct EvidenceMetadataBody {
    pub note: Option<String>,
    pub tags: Vec<String>,
}

#[derive(Deserialize)]
pub(crate) struct PlayerTagBody {
    pub name: String,
    pub color: Option<String>,
    pub description: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct LinkedAccountBatchBody {
    pub steamids64: Vec<String>,
    pub action: String,
    pub tag: String,
}

#[derive(Serialize)]
struct InvestigationReport {
    generated_at: chrono::DateTime<chrono::Utc>,
    generated_by: String,
    player: player_detail_service::PlayerDetail,
}

pub(crate) async fn get_player_detail(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Query(query): Query<PlayerDetailQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_view_audit_logs(&actor) {
        return Err(forbidden());
    }

    let detail =
        player_detail_service::get_player_detail(&ctx.db, &ctx.steam_resolver, &query.steam_input)
            .await
            .map_err(invalid_request)?;

    Ok(Json(serde_json::json!({ "data": detail })))
}

pub(crate) async fn search_player_candidates(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Query(query): Query<PlayerSearchQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_view_audit_logs(&actor) {
        return Err(forbidden());
    }

    let items =
        player_detail_service::search_player_candidates(&ctx.db, &ctx.steam_resolver, &query.query)
            .await
            .map_err(invalid_request)?;

    Ok(Json(serde_json::json!({ "items": items })))
}

/// 获取玩家内部备注（管理员只读）
pub(crate) async fn get_player_internal_profile(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(steamid64): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_view_player_internal_data(&actor) {
        return Err(forbidden());
    }

    let profile = player_detail_service::fetch_internal_profile(&ctx.db, &steamid64)
        .await
        .map_err(invalid_request)?;

    Ok(Json(serde_json::json!({ "internal_profile": profile })))
}

pub(crate) async fn update_player_internal_profile(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(steamid64): Path<String>,
    Json(body): Json<PlayerInternalBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_manage_player_internal_data(&actor) {
        return Err(forbidden());
    }

    let item = player_detail_service::upsert_player_internal_profile(
        &ctx.db,
        &steamid64,
        player_detail_service::PlayerInternalProfileInput {
            note: body.note.clone(),
            tags: body.tags.clone(),
        },
        Some(actor.id),
        &actor.display_name,
    )
    .await
    .map_err(invalid_request)?;

    if let Err(e) = log_service::create_log(
        &ctx.db,
        &actor.display_name,
        "玩家详情",
        "更新玩家内部档案",
        &steamid64,
        &extract_client_ip(&headers),
    )
    .await
    {
        tracing::warn!(%e, "日志写入失败");
    }

    Ok(Json(serde_json::json!({ "item": item })))
}

pub(crate) async fn player_internal_note_history(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(steamid64): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_view_player_internal_data(&actor) {
        return Err(forbidden());
    }
    let items = player_detail_service::fetch_internal_note_history(&ctx.db, &steamid64)
        .await
        .map_err(invalid_request)?;
    Ok(Json(serde_json::json!({ "items": items })))
}

pub(crate) async fn update_evidence_metadata(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path((source_type, file_id)): Path<(String, Uuid)>,
    Json(body): Json<EvidenceMetadataBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_manage_player_internal_data(&actor) {
        return Err(forbidden());
    }

    let item = player_detail_service::update_evidence_metadata(
        &ctx.db,
        &source_type,
        file_id,
        player_detail_service::EvidenceMetadataInput {
            note: body.note,
            tags: body.tags,
        },
    )
    .await
    .map_err(invalid_request)?;

    if let Err(e) = log_service::create_log(
        &ctx.db,
        &actor.display_name,
        "玩家详情",
        "更新证据元数据",
        &format!("{}/{}", source_type, file_id),
        &extract_client_ip(&headers),
    )
    .await
    {
        tracing::warn!(%e, "日志写入失败");
    }

    Ok(Json(serde_json::json!({ "item": item })))
}

pub(crate) async fn list_player_tags(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let _actor = current_operator(&ctx, &headers).await?;
    let tags = player_detail_service::list_player_tags(&ctx.db)
        .await
        .map_err(invalid_request)?;
    Ok(Json(serde_json::json!({ "items": tags })))
}

pub(crate) async fn create_player_tag(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Json(body): Json<PlayerTagBody>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_manage_player_internal_data(&actor) {
        return Err(forbidden());
    }
    let tag = player_detail_service::create_player_tag(
        &ctx.db,
        player_detail_service::PlayerTagInput {
            name: body.name,
            color: body.color.unwrap_or_default(),
            description: body.description.unwrap_or_default(),
        },
        actor.id,
    )
    .await
    .map_err(invalid_request)?;
    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({ "item": tag })),
    ))
}

pub(crate) async fn delete_player_tag(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(tag_id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_manage_player_internal_data(&actor) {
        return Err(forbidden());
    }
    player_detail_service::delete_player_tag(&ctx.db, tag_id)
        .await
        .map_err(invalid_request)?;
    Ok(StatusCode::NO_CONTENT)
}

pub(crate) async fn linked_account_batch_action(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(steamid64): Path<String>,
    Json(body): Json<LinkedAccountBatchBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_manage_player_internal_data(&actor) {
        return Err(forbidden());
    }
    if body.action != "add_tag" && body.action != "remove_tag" {
        return Err(invalid_request(anyhow::anyhow!("批量操作类型无效")));
    }
    let mut steamids = body.steamids64;
    steamids.push(steamid64.clone());
    let changed = player_detail_service::batch_update_player_tag(
        &ctx.db,
        &steamids,
        &body.tag,
        body.action == "add_tag",
        actor.id,
        &actor.display_name,
    )
    .await
    .map_err(invalid_request)?;
    let _ = crate::services::audit_service::write_audit_log_with_context(
        &ctx.db,
        crate::services::audit_service::AuditLogInput {
            operation: "player_linked_accounts_batch_tag".to_string(),
            target: steamid64,
            target_type: "player".to_string(),
            player_name: None,
            reason: Some(body.tag),
            duration_minutes: None,
            operator_id: Some(actor.id),
            operator_name: actor.display_name,
            operator_steamid: None,
            source: "web".to_string(),
            server_id: None,
            server_name: None,
            server_port: None,
            success: true,
            message: Some(body.action),
            idempotency_key: None,
        },
        Some(extract_client_ip(&headers)),
        None,
    )
    .await;
    Ok(Json(serde_json::json!({ "changed": changed })))
}

pub(crate) async fn player_report(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(steamid64): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_view_audit_logs(&actor) {
        return Err(forbidden());
    }
    let player = player_detail_service::get_player_detail(&ctx.db, &ctx.steam_resolver, &steamid64)
        .await
        .map_err(invalid_request)?;
    let report = InvestigationReport {
        generated_at: chrono::Utc::now(),
        generated_by: actor.display_name.clone(),
        player,
    };
    let _ = crate::services::audit_service::write_audit_log_with_context(
        &ctx.db,
        crate::services::audit_service::AuditLogInput {
            operation: "player_investigation_report_export".to_string(),
            target: steamid64,
            target_type: "player".to_string(),
            player_name: None,
            reason: None,
            duration_minutes: None,
            operator_id: Some(actor.id),
            operator_name: actor.display_name.clone(),
            operator_steamid: None,
            source: "web".to_string(),
            server_id: None,
            server_name: None,
            server_port: None,
            success: true,
            message: Some("导出玩家调查报告".to_string()),
            idempotency_key: None,
        },
        Some(extract_client_ip(&headers)),
        None,
    )
    .await;
    Ok(Json(serde_json::to_value(report).map_err(|_| {
        invalid_request(anyhow::anyhow!("报告序列化失败"))
    })?))
}

pub(crate) async fn download_evidence(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path((steamid64, source_type, file_id)): Path<(String, String, Uuid)>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_view_audit_logs(&actor) {
        return Err(forbidden());
    }
    let r2 = ctx
        .r2_storage
        .as_ref()
        .ok_or_else(|| invalid_request(anyhow::anyhow!("R2 未配置")))?;
    let url = player_detail_service::evidence_download_url(
        &ctx.db,
        r2,
        &steamid64,
        &source_type,
        file_id,
    )
    .await
    .map_err(invalid_request)?;
    let _ = crate::services::audit_service::write_audit_log_with_context(
        &ctx.db,
        crate::services::audit_service::AuditLogInput {
            operation: "player_evidence_download".to_string(),
            target: file_id.to_string(),
            target_type: "evidence".to_string(),
            player_name: None,
            reason: None,
            duration_minutes: None,
            operator_id: Some(actor.id),
            operator_name: actor.display_name,
            operator_steamid: None,
            source: "web".to_string(),
            server_id: None,
            server_name: None,
            server_port: None,
            success: true,
            message: Some(format!("玩家 {steamid64} 证据下载")),
            idempotency_key: None,
        },
        Some(extract_client_ip(&headers)),
        None,
    )
    .await;
    Ok(Json(serde_json::json!({ "url": url })))
}

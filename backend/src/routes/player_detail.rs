use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use serde::Deserialize;
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
pub(crate) struct PlayerInternalBody {
    pub note: Option<String>,
    pub tags: Vec<String>,
}

#[derive(Deserialize)]
pub(crate) struct EvidenceMetadataBody {
    pub note: Option<String>,
    pub tags: Vec<String>,
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

    let detail = player_detail_service::get_player_detail(
        &ctx.db,
        &ctx.steam_resolver,
        ctx.r2_storage.as_ref(),
        &query.steam_input,
    )
    .await
    .map_err(invalid_request)?;

    Ok(Json(serde_json::json!({ "data": detail })))
}

/// 获取玩家内部备注（管理员只读）
pub(crate) async fn get_player_internal_profile(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(steamid64): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_manage_player_internal_data(&actor) {
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

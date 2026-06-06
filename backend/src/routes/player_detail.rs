use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::routes::{current_operator, forbidden, invalid_request, AppCtx};
use crate::services::{permission_service, player_detail_service};

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
            note: body.note,
            tags: body.tags,
        },
        &actor.display_name,
    )
    .await
    .map_err(invalid_request)?;

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

    Ok(Json(serde_json::json!({ "item": item })))
}

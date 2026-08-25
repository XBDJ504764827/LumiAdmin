use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::routes::{current_operator, forbidden, invalid_request, AppCtx};
use crate::services::{control_service, log_service, permission_service};

use axum::http::header;

// Public install script
pub(crate) async fn install_script() -> impl IntoResponse {
    let body = control_service::install_script_content();
    (
        [(header::CONTENT_TYPE, "text/x-shellscript; charset=utf-8")],
        body,
    )
}

#[derive(Deserialize)]
pub(crate) struct CreateInstallTokenBody {
    #[serde(default)]
    pub base_url: Option<String>,
}

pub(crate) async fn create_install_token(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(community_id): Path<Uuid>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if actor.role != "developer" {
        return Err(forbidden());
    }
    // base_url from body or header
    let parsed: Option<CreateInstallTokenBody> = serde_json::from_value(body).ok();
    let base_url = parsed
        .and_then(|b| b.base_url)
        .filter(|s| !s.trim().is_empty())
        .or_else(|| {
            headers
                .get(header::ORIGIN)
                .and_then(|v| v.to_str().ok())
                .map(|s| s.trim_end_matches('/').to_string())
        })
        .or_else(|| ctx.config.cors_origin.clone().map(|o| o.split(',').next().unwrap_or("").trim().to_string()))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "https://YOUR_DOMAIN".to_string());

    let info =
        control_service::create_install_token(&ctx.db, community_id, actor.id, &base_url)
            .await
            .map_err(invalid_request)?;
    if let Err(e) = log_service::create_log(
        &ctx.db,
        &actor.display_name,
        "社区组管理",
        "生成控制脚本安装口令",
        &format!("社区 {}", community_id),
        &crate::services::rate_limit_service::extract_client_ip(&headers),
    )
    .await
    {
        tracing::warn!(%e, "日志写入失败");
    }
    Ok(Json(serde_json::json!({
        "token": info.token,
        "expires_at": info.expires_at.to_rfc3339(),
        "install_command": info.install_command
    })))
}

pub(crate) async fn list_discoveries(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(community_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_manage_community_mutation(&actor) {
        return Err(forbidden());
    }
    let items = control_service::list_discoveries(&ctx.db, community_id)
        .await
        .map_err(invalid_request)?;
    Ok(Json(serde_json::json!({ "discoveries": items })))
}

pub(crate) async fn list_agents(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(community_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_manage_community_mutation(&actor) {
        return Err(forbidden());
    }
    let agents = control_service::list_agents(&ctx.db, community_id)
        .await
        .map_err(invalid_request)?;
    Ok(Json(serde_json::json!({ "agents": agents })))
}

#[derive(Deserialize)]
pub(crate) struct ConfirmDiscoveriesBody {
    pub ids: Vec<Uuid>,
}

pub(crate) async fn confirm_discoveries(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(community_id): Path<Uuid>,
    Json(body): Json<ConfirmDiscoveriesBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if actor.role != "developer" {
        return Err(forbidden());
    }
    let server_ids = control_service::confirm_discoveries(&ctx.db, community_id, body.ids)
        .await
        .map_err(invalid_request)?;
    ctx.server_config_cache.invalidate_all().await;
    if let Err(e) = log_service::create_log(
        &ctx.db,
        &actor.display_name,
        "社区组管理",
        "确认自动发现服务器",
        &format!("社区 {} 新增 {} 台", community_id, server_ids.len()),
        &crate::services::rate_limit_service::extract_client_ip(&headers),
    )
    .await
    {
        tracing::warn!(%e, "日志写入失败");
    }
    Ok(Json(serde_json::json!({ "server_ids": server_ids })))
}

#[derive(Deserialize)]
pub(crate) struct PowerBody {
    pub action: String,
}

pub(crate) async fn power_server(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(server_id): Path<Uuid>,
    Json(body): Json<PowerBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    let action = body.action.trim().to_lowercase();
    if action == "restart" {
        if !permission_service::can_execute_rcon(&actor) {
            return Err(forbidden());
        }
        // admin/normal can restart, developer can all
    } else if action == "start" || action == "stop" {
        if actor.role != "developer" {
            return Err(forbidden());
        }
    } else {
        return Err(invalid_request(anyhow::anyhow!("action 只能为 restart/start/stop")));
    }
    let job = control_service::create_power_job(&ctx.db, server_id, &action, actor.id)
        .await
        .map_err(invalid_request)?;
    let action_label: &str = match action.as_str() { "restart"=>"强制重启", "start"=>"强制启动", "stop"=>"强制关机", _=>action.as_str() };
    if let Err(e) = log_service::create_log(
        &ctx.db,
        &actor.display_name,
        "服务器控制",
        Box::leak(format!("下发{}", action_label).into_boxed_str()),
        &format!("服务器 {} 任务 {}", server_id, job.id),
        &crate::services::rate_limit_service::extract_client_ip(&headers),
    )
    .await
    {
        tracing::warn!(%e, "日志写入失败");
    }
    Ok(Json(serde_json::json!({ "job": job })))
}

pub(crate) async fn list_jobs(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(server_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let _actor = current_operator(&ctx, &headers).await?;
    let jobs = control_service::list_jobs_for_server(&ctx.db, server_id)
        .await
        .map_err(invalid_request)?;
    Ok(Json(serde_json::json!({ "jobs": jobs })))
}

// Agent unauthenticated endpoints

pub(crate) async fn register_agent(
    State(ctx): State<AppCtx>,
    Json(body): Json<control_service::RegisterAgentInput>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let agent = control_service::register_agent(&ctx.db, body)
        .await
        .map_err(invalid_request)?;
    Ok(Json(serde_json::json!({
        "token": agent.token,
        "community_id": agent.community_id,
        "agent_id": agent.id
    })))
}

pub(crate) async fn discover(
    State(ctx): State<AppCtx>,
    Json(body): Json<control_service::DiscoverInput>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let rows = control_service::report_discoveries(&ctx.db, body)
        .await
        .map_err(invalid_request)?;
    Ok(Json(serde_json::json!({ "discoveries": rows })))
}

pub(crate) async fn poll(
    State(ctx): State<AppCtx>,
    Json(body): Json<control_service::PollInput>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let job = control_service::poll_job(&ctx.db, &body.token)
        .await
        .map_err(|e| {
            if e.to_string().contains("Agent口令无效") {
                (StatusCode::UNAUTHORIZED, Json(serde_json::json!({ "error": e.to_string() })))
            } else {
                invalid_request(e)
            }
        })?;
    if let Some(j) = job {
        // enrich with lgsm_instance
        let lgsm: Option<(Option<String>,)> = sqlx::query_as(r#"SELECT lgsm_instance FROM servers WHERE id = $1"#)
            .bind(j.server_id)
            .fetch_optional(&ctx.db.pool)
            .await
            .map_err(|e| invalid_request(anyhow::anyhow!(e.to_string())))?;
        let lgsm_instance = lgsm.and_then(|r| r.0).unwrap_or_else(|| "csgoserver".to_string());
        Ok(Json(serde_json::json!({
            "job": {
                "id": j.id,
                "server_id": j.server_id,
                "action": j.action,
                "status": j.status,
                "lgsm_instance": lgsm_instance
            }
        })))
    } else {
        Ok(Json(serde_json::json!({ "job": null })))
    }
}

pub(crate) async fn report_result(
    State(ctx): State<AppCtx>,
    Json(body): Json<control_service::ResultInput>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let job = control_service::report_job_result(&ctx.db, body)
        .await
        .map_err(|e| {
            if e.to_string().contains("Agent口令无效") {
                (StatusCode::UNAUTHORIZED, Json(serde_json::json!({ "error": e.to_string() })))
            } else {
                invalid_request(e)
            }
        })?;
    Ok(Json(serde_json::json!({ "job": job })))
}

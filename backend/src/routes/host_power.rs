use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    Json,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::routes::{current_operator, forbidden, invalid_request, AppCtx};
use crate::services::{
    host_power_service, log_service, permission_service, rate_limit_service::extract_client_ip,
};

async fn agent_operator(
    ctx: &AppCtx,
    headers: &HeaderMap,
) -> Result<host_power_service::AgentRow, (StatusCode, Json<serde_json::Value>)> {
    let token = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or((
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "missing agent token" })),
        ))?;
    host_power_service::authenticate_agent(&ctx.db, token)
        .await
        .map_err(|_| {
            (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({ "error": "Agent 口令无效" })),
            )
        })
}

// ---------------------------------------------------------------------------
// 管理侧：安装口令签发
// ---------------------------------------------------------------------------

pub(crate) async fn create_install_token(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if actor.role != "developer" {
        return Err(forbidden());
    }
    let info = host_power_service::create_install_token(&ctx.db, actor.id)
        .await
        .map_err(invalid_request)?;
    if let Err(e) = log_service::create_log(
        &ctx.db,
        &actor.display_name,
        "服务器控制",
        "签发Agent安装口令",
        &format!("15 分钟有效，过期 {}", info.expires_at.to_rfc3339()),
        &extract_client_ip(&headers),
    )
    .await
    {
        tracing::warn!(%e, "日志写入失败");
    }
    Ok(Json(serde_json::json!({
        "token": info.token,
        "expires_at": info.expires_at.to_rfc3339(),
    })))
}

pub(crate) async fn list_agents(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_manage_community_mutation(&actor) {
        return Err(forbidden());
    }
    let agents = host_power_service::list_agents(&ctx.db)
        .await
        .map_err(invalid_request)?;
    Ok(Json(serde_json::json!({ "agents": agents })))
}

pub(crate) async fn power_overview(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_manage_community_mutation(&actor) {
        return Err(forbidden());
    }
    let overview = host_power_service::overview(&ctx.db)
        .await
        .map_err(invalid_request)?;
    Ok(Json(serde_json::json!({ "overview": overview })))
}

// ---------------------------------------------------------------------------
// 管理侧：电源下发与任务查询
// ---------------------------------------------------------------------------

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
    if action == host_power_service::ACTION_RESTART {
        if !permission_service::can_execute_rcon(&actor) {
            return Err(forbidden());
        }
    } else if matches!(action.as_str(), "start" | "stop" | "force-restart") {
        if actor.role != "developer" {
            return Err(forbidden());
        }
    } else {
        return Err(invalid_request(anyhow::anyhow!(
            "action 只能为 restart/start/stop/force-restart"
        )));
    }

    let job = host_power_service::create_power_job(&ctx.db, server_id, &action, actor.id)
        .await
        .map_err(invalid_request)?;
    let action_label = match action.as_str() {
        "restart" => "重启服务器",
        "force-restart" => "强制重启服务器",
        "start" => "开启服务器",
        _ => "关闭服务器",
    };
    if let Err(e) = log_service::create_log(
        &ctx.db,
        &actor.display_name,
        "服务器控制",
        action_label,
        &format!("服务器 {} 任务 {}", server_id, job.id),
        &extract_client_ip(&headers),
    )
    .await
    {
        tracing::warn!(%e, "日志写入失败");
    }
    Ok(Json(serde_json::json!({ "job": job })))
}

#[derive(Deserialize)]
pub(crate) struct JobListQuery {
    pub limit: Option<i64>,
}

pub(crate) async fn list_power_jobs(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(server_id): Path<Uuid>,
    Query(query): Query<JobListQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_execute_rcon(&actor) {
        return Err(forbidden());
    }
    let jobs = host_power_service::list_server_jobs(&ctx.db, server_id, query.limit.unwrap_or(10))
        .await
        .map_err(invalid_request)?;
    Ok(Json(serde_json::json!({ "jobs": jobs })))
}

// ---------------------------------------------------------------------------
// Agent 侧：注册 / 心跳 / 领任务 / 回结果
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub(crate) struct RegisterAgentBody {
    pub install_token: String,
    #[serde(default)]
    pub hostname: String,
    #[serde(default)]
    pub lgsm_dir: String,
    #[serde(default)]
    pub instances: Vec<String>,
}

pub(crate) async fn register_agent(
    State(ctx): State<AppCtx>,
    Json(body): Json<RegisterAgentBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let registered = host_power_service::register_agent(
        &ctx.db,
        &body.install_token,
        &body.hostname,
        &body.lgsm_dir,
        &body.instances,
    )
    .await
    .map_err(invalid_request)?;
    Ok(Json(serde_json::json!({
        "agent_id": registered.agent_id,
        "agent_token": registered.agent_token,
    })))
}

#[derive(Deserialize)]
pub(crate) struct HeartbeatBody {
    #[serde(default)]
    pub hostname: String,
    #[serde(default)]
    pub instances: Vec<String>,
}

pub(crate) async fn agent_heartbeat(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Json(body): Json<HeartbeatBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let agent = agent_operator(&ctx, &headers).await?;
    host_power_service::heartbeat(&ctx.db, agent.id, &body.hostname, &body.instances)
        .await
        .map_err(invalid_request)?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

#[derive(Deserialize)]
pub(crate) struct PollJobsBody {
    #[serde(default = "default_poll_max")]
    pub max: i64,
}

fn default_poll_max() -> i64 {
    5
}

pub(crate) async fn poll_jobs(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Json(body): Json<PollJobsBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let agent = agent_operator(&ctx, &headers).await?;
    if agent.disabled {
        return Ok(Json(serde_json::json!({ "jobs": [] })));
    }
    let jobs = host_power_service::poll_jobs(&ctx.db, agent.id, body.max)
        .await
        .map_err(invalid_request)?;
    Ok(Json(serde_json::json!({ "jobs": jobs })))
}

#[derive(Deserialize)]
pub(crate) struct UpdateAgentBody {
    pub display_name: Option<String>,
    pub disabled: Option<bool>,
}

pub(crate) async fn update_agent(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(agent_id): Path<Uuid>,
    Json(body): Json<UpdateAgentBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if actor.role != "developer" {
        return Err(forbidden());
    }
    let agent =
        host_power_service::update_agent(&ctx.db, agent_id, body.display_name, body.disabled)
            .await
            .map_err(invalid_request)?;
    if let Err(e) = log_service::create_log(
        &ctx.db,
        &actor.display_name,
        "服务器控制",
        "编辑宿主机Agent",
        &format!("Agent {}（停用：{}）", agent_id, agent.disabled),
        &extract_client_ip(&headers),
    )
    .await
    {
        tracing::warn!(%e, "日志写入失败");
    }
    Ok(Json(serde_json::json!({ "agent": agent })))
}

pub(crate) async fn delete_agent(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(agent_id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if actor.role != "developer" {
        return Err(forbidden());
    }
    host_power_service::delete_agent(&ctx.db, agent_id)
        .await
        .map_err(invalid_request)?;
    if let Err(e) = log_service::create_log(
        &ctx.db,
        &actor.display_name,
        "服务器控制",
        "删除宿主机Agent",
        &format!("Agent {}（已绑定的服务器自动解绑）", agent_id),
        &extract_client_ip(&headers),
    )
    .await
    {
        tracing::warn!(%e, "日志写入失败");
    }
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub(crate) struct JobResultBody {
    #[serde(default)]
    pub exit_code: i32,
    #[serde(default)]
    pub output: Option<String>,
    #[serde(default)]
    pub timed_out: bool,
}

pub(crate) async fn report_job_result(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(job_id): Path<Uuid>,
    Json(body): Json<JobResultBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let agent = agent_operator(&ctx, &headers).await?;
    let job = host_power_service::report_result(
        &ctx.db,
        agent.id,
        job_id,
        body.exit_code,
        body.output,
        body.timed_out,
    )
    .await
    .map_err(invalid_request)?;
    Ok(Json(serde_json::json!({ "job": job })))
}

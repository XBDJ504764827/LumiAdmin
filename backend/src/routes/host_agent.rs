use axum::{
    body::Body,
    extract::{Path, State},
    http::{header, HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};

use crate::routes::{current_operator, forbidden, AppCtx};
use crate::services::{host_power_service, permission_service};

const INSTALL_SCRIPT: &str = include_str!("../../agent/install.sh");
const SERVICE_UNIT: &str = include_str!("../../agent/lumi-host-agent.service");

/// 预编译二进制存在性（磁盘托管，路径见 AGENT_BINARY_PATH）。
fn binary_info(agent_binary_path: Option<&str>) -> (u64, bool) {
    match agent_binary_path {
        Some(path) => std::fs::metadata(path)
            .map(|meta| (meta.len(), meta.is_file()))
            .unwrap_or((0, false)),
        None => (0, false),
    }
}

/// 网站托管下发的 Agent 文件清单（与 install.sh 引用的下载端点保持一致）。
fn hosted_files(agent_binary_path: Option<&str>) -> Vec<serde_json::Value> {
    let (binary_size, binary_available) = binary_info(agent_binary_path);
    vec![
        serde_json::json!({
            "name": "install.sh",
            "size_bytes": INSTALL_SCRIPT.len(),
            "available": true,
            "description": "一键安装脚本",
        }),
        serde_json::json!({
            "name": "lumi-host-agent.service",
            "size_bytes": SERVICE_UNIT.len(),
            "available": true,
            "description": "systemd 单元模板",
        }),
        serde_json::json!({
            "name": "lumi-server-agent-x86_64",
            "size_bytes": binary_size,
            "available": binary_available,
            "description": "预编译二进制（网站服务器磁盘托管）",
        }),
    ]
}

pub(crate) async fn get_setup(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_manage_community_mutation(&actor) {
        return Err(forbidden());
    }
    Ok(Json(serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
        "files": hosted_files(ctx.config.agent_binary_path.as_deref()),
    })))
}

fn bearer_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::to_string)
}

/// 下载鉴权（三选一）：管理员会话（需社区管理权限）、已注册 Agent 口令、
/// 有效未使用的安装口令（安装阶段用，消费只发生在注册时）。
async fn authorize_download(
    ctx: &AppCtx,
    headers: &HeaderMap,
) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
    if let Ok(actor) = current_operator(ctx, headers).await {
        if !permission_service::can_manage_community_mutation(&actor) {
            return Err(forbidden());
        }
        return Ok(());
    }
    let token = bearer_token(headers).ok_or((
        StatusCode::UNAUTHORIZED,
        Json(serde_json::json!({ "error": "missing token" })),
    ))?;
    if host_power_service::authenticate_agent(&ctx.db, &token)
        .await
        .is_ok()
    {
        return Ok(());
    }
    if host_power_service::check_install_token_valid(&ctx.db, &token)
        .await
        .is_ok()
    {
        return Ok(());
    }
    Err((
        StatusCode::UNAUTHORIZED,
        Json(serde_json::json!({ "error": "口令无效" })),
    ))
}

pub(crate) async fn download_file(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(filename): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    authorize_download(&ctx, &headers).await?;
    let name = filename.trim();
    let (body, content_type) = match name {
        "install.sh" => (
            Body::from(INSTALL_SCRIPT.to_string()),
            "text/x-shellscript; charset=utf-8",
        ),
        "lumi-host-agent.service" => (
            Body::from(SERVICE_UNIT.to_string()),
            "text/plain; charset=utf-8",
        ),
        "lumi-server-agent-x86_64" => {
            let path = ctx.config.agent_binary_path.clone().unwrap_or_default();
            match tokio::fs::read(&path).await {
                Ok(bytes) if !bytes.is_empty() => (Body::from(bytes), "application/octet-stream"),
                _ => {
                    return Err((
                        StatusCode::NOT_IMPLEMENTED,
                        Json(serde_json::json!({
                            "error": "二进制包尚未托管：请在网站服务器放置 release 二进制并配置 AGENT_BINARY_PATH",
                        })),
                    ));
                }
            }
        }
        _ => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "文件不存在" })),
            ));
        }
    };
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        content_type.parse().unwrap_or_else(|_| {
            "text/plain; charset=utf-8"
                .parse()
                .expect("默认类型写死，应恒成立")
        }),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        format!("attachment; filename=\"{}\"", name)
            .parse()
            .unwrap_or_else(|_| "attachment".parse().expect("默认处置写死，应恒成立")),
    );
    Ok((headers, body))
}

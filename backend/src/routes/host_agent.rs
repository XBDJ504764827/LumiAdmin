use axum::{
    extract::{Path, State},
    http::{header, HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};

use crate::routes::{current_operator, forbidden, AppCtx};
use crate::services::permission_service;

const INSTALL_SCRIPT: &str = include_str!("../../agent/install.sh");
const SERVICE_UNIT: &str = include_str!("../../agent/lumi-host-agent.service");

/// 网站托管下发的 Agent 文件清单（与 install.sh 引用的下载端点保持一致）。
fn hosted_files() -> Vec<serde_json::Value> {
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
            "size_bytes": 0,
            "available": false,
            "description": "预编译二进制（待发布流程落地后提供）",
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
        "files": hosted_files(),
    })))
}

pub(crate) async fn download_file(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(filename): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_manage_community_mutation(&actor) {
        return Err(forbidden());
    }
    let name = filename.trim();
    let (content, content_type) = match name {
        "install.sh" => (INSTALL_SCRIPT, "text/x-shellscript; charset=utf-8"),
        "lumi-host-agent.service" => (SERVICE_UNIT, "text/plain; charset=utf-8"),
        "lumi-server-agent-x86_64" => {
            return Err((
                StatusCode::NOT_IMPLEMENTED,
                Json(serde_json::json!({
                    "error": "二进制包尚未发布：需先在 LumiServerAgent 落地 release 产物上传流程",
                })),
            ));
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
    Ok((headers, content.to_string()))
}

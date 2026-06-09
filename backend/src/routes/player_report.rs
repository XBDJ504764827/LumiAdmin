use axum::{
    extract::{Multipart, Path, Query, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::routes::{current_operator, forbidden, invalid_request, AppCtx, ListQuery};
use crate::services::rate_limit_service::extract_client_ip;
use crate::services::{
    external_ban_api_service, log_service, notification_service, permission_service,
    player_report_service, r2_storage,
};

#[derive(Deserialize)]
pub(crate) struct SubmitPlayerReportBody {
    pub steam_input: String,
    pub target_player_name: Option<String>,
    pub reporter_contact: Option<String>,
    pub report_reason: String,
}

#[derive(Deserialize)]
pub(crate) struct ReviewPlayerReportBody {
    pub status: String,
    pub review_note: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct BanPlayerReportBody {
    pub player: Option<String>,
    pub steam_id: Option<String>,
    pub ip_address: Option<String>,
    pub ban_type: String,
    pub reason: String,
}

pub(crate) async fn submit_player_report(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Json(body): Json<SubmitPlayerReportBody>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<serde_json::Value>)> {
    let item = player_report_service::create_report(
        &ctx.db,
        &ctx.config,
        player_report_service::CreatePlayerReportInput {
            steam_input: body.steam_input,
            target_player_name: body.target_player_name,
            reporter_contact: body.reporter_contact,
            report_reason: body.report_reason,
        },
    )
    .await
    .map_err(invalid_request)?;

    let log_target = format!(
        "{} ({})",
        item.target_player_name.as_deref().unwrap_or("未知玩家"),
        item.target_steam_id
    );
    let _ = log_service::create_log(
        &ctx.db,
        "guest",
        "公共展示页",
        "提交玩家举报",
        &log_target,
        &extract_client_ip(&headers),
    )
    .await;

    if let Err(e) = notification_service::notify_all_admins(
        &ctx.db,
        &ctx.notification_hub,
        None,
        "player_report",
        "新玩家举报",
        &format!(
            "玩家 {} 收到新的公开举报，请尽快审核。",
            item.target_steam_id
        ),
        Some("/player-reports"),
    )
    .await
    {
        tracing::warn!(%e, "player report notification failed");
    }

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({
            "id": item.id,
            "report_id": item.id,
            "upload_token": item.upload_token,
            "item": item,
        })),
    ))
}

pub(crate) async fn upload_player_report_files(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(report_id): Path<Uuid>,
    mut multipart: Multipart,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let r2 = ctx.r2_storage.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({ "error": "文件上传服务未配置" })),
        )
    })?;

    let (status, upload_token_hash) =
        player_report_service::find_upload_token_hash(&ctx.db, report_id)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, report_id = %report_id, "查询举报上传凭证失败");
                (
                    StatusCode::NOT_FOUND,
                    Json(serde_json::json!({ "error": "举报记录不存在" })),
                )
            })?;

    let upload_token = headers
        .get("x-report-upload-token")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    if !player_report_service::verify_upload_token(upload_token_hash.as_deref(), upload_token) {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "上传凭证无效，请重新提交举报" })),
        ));
    }

    if status != "pending" {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "该举报已被处理，无法上传文件" })),
        ));
    }

    let max_size = ctx.config.appeal_file_max_size_bytes;
    let max_files = 10usize;
    let mut file_count = 0usize;
    let mut uploaded: Vec<serde_json::Value> = Vec::new();
    let mut errors: Vec<String> = Vec::new();

    loop {
        let field = match multipart.next_field().await {
            Ok(Some(field)) => field,
            Ok(None) => break,
            Err(e) => {
                tracing::warn!(%e, "读取举报附件 multipart 字段失败");
                errors.push("读取上传内容失败".to_string());
                break;
            }
        };

        let file_name = match field.file_name() {
            Some(name) => name.to_string(),
            None => continue,
        };

        file_count += 1;
        if file_count > max_files {
            errors.push(format!("单次最多上传 {max_files} 个文件"));
            continue;
        }

        if !r2_storage::is_allowed_file_type(&file_name) {
            errors.push(format!("不支持的文件类型: {file_name}"));
            continue;
        }

        let content_type = field
            .content_type()
            .map(|value| value.to_string())
            .unwrap_or_else(|| r2_storage::guess_content_type(&file_name).to_string());
        let data = match field.bytes().await {
            Ok(bytes) => bytes.to_vec(),
            Err(e) => {
                errors.push(format!("读取文件失败: {file_name} - {e}"));
                continue;
            }
        };
        let file_size = data.len();

        if file_size > max_size {
            errors.push(format!(
                "文件 {} 超出大小限制（最大 {}MB）",
                file_name,
                max_size / 1024 / 1024
            ));
            continue;
        }
        if file_size == 0 {
            errors.push(format!("文件为空: {file_name}"));
            continue;
        }

        match r2
            .upload_with_prefix("player-reports", report_id, &file_name, &content_type, data)
            .await
        {
            Ok(key) => {
                let category = r2_storage::file_category(&file_name);
                if let Err(e) = sqlx::query(
                    r#"INSERT INTO player_report_files (id, report_id, file_name, file_size, content_type, storage_key, category)
                       VALUES ($1, $2, $3, $4, $5, $6, $7)"#,
                )
                .bind(Uuid::new_v4())
                .bind(report_id)
                .bind(&file_name)
                .bind(file_size as i64)
                .bind(&content_type)
                .bind(&key)
                .bind(category)
                .execute(&ctx.db.pool)
                .await
                {
                    tracing::warn!(%e, "写入举报附件记录失败");
                }
                uploaded.push(serde_json::json!({
                    "file_name": file_name,
                    "file_size": file_size,
                    "category": category,
                }));
            }
            Err(e) => {
                tracing::error!(%e, "player report file R2 upload failed for {file_name}");
                errors.push(format!("上传文件 {file_name} 失败，请稍后重试"));
            }
        }
    }

    if uploaded.is_empty() && errors.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "未选择可上传的文件" })),
        ));
    }
    if uploaded.is_empty() && !errors.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "所有文件上传失败", "errors": errors })),
        ));
    }

    if let Err(e) = sqlx::query("UPDATE player_reports SET upload_token_hash = NULL WHERE id = $1")
        .bind(report_id)
        .execute(&ctx.db.pool)
        .await
    {
        tracing::warn!(%e, "清理玩家举报上传凭证失败");
    }

    Ok(Json(serde_json::json!({
        "uploaded": uploaded,
        "errors": if errors.is_empty() { None } else { Some(errors) },
    })))
}

pub(crate) async fn list_reports(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Query(query): Query<ListQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_review_player_reports(&actor) {
        return Err(forbidden());
    }

    let result = player_report_service::list_reports(&ctx.db, &query)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "加载举报列表失败");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "加载举报列表失败" })),
            )
        })?;
    Ok(Json(serde_json::json!({
        "items": result.items,
        "total": result.total,
        "page": result.page,
        "page_size": result.page_size,
    })))
}

pub(crate) async fn review_report(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(body): Json<ReviewPlayerReportBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_review_player_reports(&actor) {
        return Err(forbidden());
    }

    let item = player_report_service::review_report(
        &ctx.db,
        id,
        &actor.display_name,
        player_report_service::ReviewPlayerReportInput {
            status: body.status,
            review_note: body.review_note,
        },
    )
    .await
    .map_err(invalid_request)?;

    let _ = log_service::create_log(
        &ctx.db,
        &actor.display_name,
        "玩家举报",
        if item.status == "approved" {
            "通过玩家举报"
        } else {
            "驳回玩家举报"
        },
        &format!(
            "{} ({})",
            item.target_player_name.as_deref().unwrap_or("未知玩家"),
            item.target_steam_id
        ),
        &extract_client_ip(&headers),
    )
    .await;

    Ok(Json(serde_json::json!({ "item": item })))
}

pub(crate) async fn ban_report(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(body): Json<BanPlayerReportBody>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_review_player_reports(&actor) {
        return Err(forbidden());
    }

    let result = player_report_service::ban_report(
        &ctx.db,
        &ctx.config,
        id,
        &actor.display_name,
        player_report_service::BanPlayerReportInput {
            player: body.player,
            steam_id: body.steam_id,
            ip_address: body.ip_address,
            ban_type: body.ban_type,
            reason: body.reason,
        },
    )
    .await
    .map_err(invalid_request)?;

    let log_target = format!(
        "{} ({}) | 类型: {} | 理由: {}",
        result.ban.player.as_deref().unwrap_or("未知"),
        result.ban.steam_id,
        result.ban.ban_type,
        result.ban.reason
    );
    let client_ip = extract_client_ip(&headers);
    let log_ip = client_ip.clone();
    let _ = log_service::create_log(
        &ctx.db,
        &actor.display_name,
        "玩家举报",
        "根据举报封禁玩家",
        &log_target,
        &log_ip,
    )
    .await;

    if let Err(e) = notification_service::notify_ban_create(
        &ctx.db,
        &ctx.notification_hub,
        &actor.id,
        &actor.display_name,
        result.ban.player.as_deref(),
        &result.ban.steam_id,
        &result.ban.reason,
    )
    .await
    {
        tracing::warn!(%e, "player report ban notification failed");
    }
    external_ban_api_service::sync_ban_if_enabled(&ctx.db, &result.ban).await;

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({
            "ban": result.ban,
            "report": result.report,
            "copied_files": result.copied_files,
        })),
    ))
}

pub(crate) async fn list_report_files(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(report_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_review_player_reports(&actor) {
        return Err(forbidden());
    }

    #[derive(sqlx::FromRow)]
    struct FileRow {
        id: Uuid,
        file_name: String,
        file_size: i64,
        content_type: String,
        storage_key: String,
        category: String,
        uploaded_by: Option<String>,
        uploaded_at: chrono::DateTime<chrono::Utc>,
    }

    let files: Vec<FileRow> = sqlx::query_as(
        r#"SELECT id, file_name, file_size, content_type, storage_key, category, uploaded_by, uploaded_at
           FROM player_report_files WHERE report_id = $1 ORDER BY uploaded_at ASC"#,
    )
    .bind(report_id)
    .fetch_all(&ctx.db.pool)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "加载举报文件失败");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "加载举报文件失败" })),
        )
    })?;

    let r2 = ctx.r2_storage.as_ref();
    let items: Vec<serde_json::Value> = files
        .into_iter()
        .map(|file| {
            let url = r2.map(|storage| storage.presigned_url(&file.storage_key, 3600));
            serde_json::json!({
                "id": file.id,
                "file_name": file.file_name,
                "file_size": file.file_size,
                "content_type": file.content_type,
                "category": file.category,
                "uploaded_by": file.uploaded_by,
                "uploaded_at": file.uploaded_at.to_rfc3339(),
                "url": url,
            })
        })
        .collect();

    Ok(Json(serde_json::json!({ "files": items })))
}

pub(crate) async fn upload_report_files(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(report_id): Path<Uuid>,
    mut multipart: Multipart,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_review_player_reports(&actor) {
        return Err(forbidden());
    }

    // Verify report exists
    sqlx::query(r#"SELECT id FROM player_reports WHERE id = $1"#)
        .bind(report_id)
        .fetch_optional(&ctx.db.pool)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "查询举报记录失败");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "查询举报记录失败" })),
            )
        })?
        .ok_or((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "举报记录不存在" })),
        ))?;

    let r2 = ctx.r2_storage.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({ "error": "文件上传服务未配置" })),
        )
    })?;

    let max_size = ctx.config.appeal_file_max_size_bytes;
    let mut uploaded: Vec<serde_json::Value> = Vec::new();
    let mut errors: Vec<String> = Vec::new();

    loop {
        let field = match multipart.next_field().await {
            Ok(Some(field)) => field,
            Ok(None) => break,
            Err(e) => {
                tracing::warn!(%e, "读取举报附件 multipart 字段失败");
                errors.push("读取上传内容失败".to_string());
                break;
            }
        };

        let file_name = match field.file_name() {
            Some(name) => name.to_string(),
            None => continue,
        };

        if !r2_storage::is_allowed_file_type(&file_name) {
            errors.push(format!("不支持的文件类型: {file_name}"));
            continue;
        }

        let content_type = field
            .content_type()
            .map(|value| value.to_string())
            .unwrap_or_else(|| r2_storage::guess_content_type(&file_name).to_string());

        let data = match field.bytes().await {
            Ok(bytes) => bytes.to_vec(),
            Err(e) => {
                errors.push(format!("读取文件失败: {file_name} - {e}"));
                continue;
            }
        };
        let file_size = data.len();

        if file_size > max_size {
            errors.push(format!(
                "文件 {} 超出大小限制（最大 {}MB）",
                file_name,
                max_size / 1024 / 1024
            ));
            continue;
        }

        if file_size == 0 {
            errors.push(format!("文件为空: {file_name}"));
            continue;
        }

        match r2
            .upload_with_prefix("player-reports", report_id, &file_name, &content_type, data)
            .await
        {
            Ok(key) => {
                let category = r2_storage::file_category(&file_name);
                if let Err(e) = sqlx::query(
                    r#"INSERT INTO player_report_files (id, report_id, file_name, file_size, content_type, storage_key, category, uploaded_by)
                       VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"#,
                )
                .bind(Uuid::new_v4())
                .bind(report_id)
                .bind(&file_name)
                .bind(file_size as i64)
                .bind(&content_type)
                .bind(&key)
                .bind(category)
                .bind(&actor.display_name)
                .execute(&ctx.db.pool)
                .await
                {
                    tracing::warn!(%e, "写入举报附件记录失败");
                }

                uploaded.push(serde_json::json!({
                    "file_name": file_name,
                    "file_size": file_size,
                    "category": category,
                }));
            }
            Err(e) => {
                tracing::error!(%e, "report file R2 upload failed for {file_name}");
                errors.push(format!("上传文件 {file_name} 失败，请稍后重试"));
            }
        }
    }

    if uploaded.is_empty() && errors.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "未选择可上传的文件" })),
        ));
    }

    if uploaded.is_empty() && !errors.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "所有文件上传失败",
                "errors": errors,
            })),
        ));
    }

    if !uploaded.is_empty() {
        if let Err(e) = log_service::create_log(
            &ctx.db,
            &actor.display_name,
            "玩家举报",
            "上传举报证据文件",
            &format!("举报 {} — 上传 {} 个文件", report_id, uploaded.len()),
            &extract_client_ip(&headers),
        )
        .await
        {
            tracing::warn!(%e, "日志写入失败");
        }
    }

    Ok(Json(serde_json::json!({
        "uploaded": uploaded,
        "errors": if errors.is_empty() { None } else { Some(errors) },
    })))
}

// ---------------------------------------------------------------------------
// Public: query report status by SteamID
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub(crate) struct QueryReportStatusBody {
    pub steam_input: String,
}

pub(crate) async fn query_report_status(
    State(ctx): State<AppCtx>,
    Json(body): Json<QueryReportStatusBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let parsed = ctx.steam_resolver.resolve(&body.steam_input).await.map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": e.to_string()})),
        )
    })?;

    let reports = player_report_service::query_reports_by_target_steam_id(&ctx.db, &parsed.steamid64)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "查询举报状态失败");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "查询举报状态失败"})),
            )
        })?;

    Ok(Json(serde_json::json!({
        "steamid64": parsed.steamid64,
        "reports": reports,
    })))
}

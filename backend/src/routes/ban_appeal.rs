use axum::{
    extract::{Multipart, Path, Query, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::routes::{current_operator, forbidden, invalid_request, AppCtx, ListQuery};
use crate::services::rate_limit_service::extract_client_ip;
use crate::services::{ban_appeal_service, log_service, permission_service, r2_storage};

pub(crate) async fn list_appeals(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Query(query): Query<ListQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_review_ban_appeals(&actor) {
        return Err(forbidden());
    }

    let result = ban_appeal_service::list_appeals(&ctx.db, &query)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "加载申诉列表失败");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "加载申诉列表失败" })),
            )
        })?;
    Ok(Json(
        serde_json::json!({ "items": result.items, "total": result.total, "page": result.page, "page_size": result.page_size }),
    ))
}

#[derive(Deserialize)]
pub(crate) struct ReviewBody {
    pub review_note: Option<String>,
}

pub(crate) async fn approve_appeal(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(body): Json<ReviewBody>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_review_ban_appeals(&actor) {
        return Err(forbidden());
    }

    let item =
        ban_appeal_service::approve_appeal(&ctx.db, id, &actor.display_name, body.review_note)
            .await
            .map_err(invalid_request)?;

    let log_target = format!("{} ({})", item.player_name, item.steam_id);
    if let Err(e) = log_service::create_log(
        &ctx.db,
        &actor.display_name,
        "封禁申诉",
        "通过申诉并解封",
        &log_target,
        &extract_client_ip(&headers),
    )
    .await
    {
        tracing::warn!(%e, "日志写入失败");
    }

    Ok((StatusCode::OK, Json(serde_json::json!({ "item": item }))))
}

pub(crate) async fn reject_appeal(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(body): Json<ReviewBody>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_review_ban_appeals(&actor) {
        return Err(forbidden());
    }

    let item =
        ban_appeal_service::reject_appeal(&ctx.db, id, &actor.display_name, body.review_note)
            .await
            .map_err(invalid_request)?;

    let log_target = format!("{} ({})", item.player_name, item.steam_id);
    if let Err(e) = log_service::create_log(
        &ctx.db,
        &actor.display_name,
        "封禁申诉",
        "驳回申诉",
        &log_target,
        &extract_client_ip(&headers),
    )
    .await
    {
        tracing::warn!(%e, "日志写入失败");
    }

    Ok((StatusCode::OK, Json(serde_json::json!({ "item": item }))))
}

/// 管理员查看申诉关联文件列表
pub(crate) async fn list_appeal_files(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(appeal_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_review_ban_appeals(&actor) {
        return Err(forbidden());
    }

    #[derive(serde::Serialize, sqlx::FromRow)]
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
        "SELECT id, file_name, file_size, content_type, storage_key, category, uploaded_by, uploaded_at
         FROM appeal_files WHERE appeal_id = $1 ORDER BY uploaded_at ASC",
    )
    .bind(appeal_id)
    .fetch_all(&ctx.db.pool)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "加载申诉文件失败");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "加载申诉文件失败" })),
        )
    })?;

    let r2 = ctx.r2_storage.as_ref();

    let items: Vec<serde_json::Value> = files
        .into_iter()
        .map(|f| {
            let presigned_url = r2.map(|r| r.presigned_url(&f.storage_key, 3600));
            serde_json::json!({
                "id": f.id,
                "file_name": f.file_name,
                "file_size": f.file_size,
                "content_type": f.content_type,
                "category": f.category,
                "uploaded_by": f.uploaded_by,
                "url": presigned_url,
                "uploaded_at": f.uploaded_at.to_rfc3339(),
            })
        })
        .collect();

    Ok(Json(serde_json::json!({ "files": items })))
}

/// 管理员为申诉上传证据文件
pub(crate) async fn upload_appeal_files(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(appeal_id): Path<Uuid>,
    mut multipart: Multipart,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_review_ban_appeals(&actor) {
        return Err(forbidden());
    }

    // Verify appeal exists
    sqlx::query(r#"SELECT id FROM ban_appeals WHERE id = $1"#)
        .bind(appeal_id)
        .fetch_optional(&ctx.db.pool)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "查询申诉记录失败");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "查询申诉记录失败" })),
            )
        })?
        .ok_or((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "申诉记录不存在" })),
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
                tracing::warn!(%e, "读取申诉附件 multipart 字段失败");
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
            .upload_with_prefix("appeals", appeal_id, &file_name, &content_type, data)
            .await
        {
            Ok(key) => {
                let category = r2_storage::file_category(&file_name);
                if let Err(e) = sqlx::query(
                    r#"INSERT INTO appeal_files (id, appeal_id, file_name, file_size, content_type, storage_key, category, uploaded_by)
                       VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"#,
                )
                .bind(Uuid::new_v4())
                .bind(appeal_id)
                .bind(&file_name)
                .bind(file_size as i64)
                .bind(&content_type)
                .bind(&key)
                .bind(category)
                .bind(&actor.display_name)
                .execute(&ctx.db.pool)
                .await
                {
                    tracing::warn!(%e, "写入申诉附件记录失败");
                }

                uploaded.push(serde_json::json!({
                    "file_name": file_name,
                    "file_size": file_size,
                    "category": category,
                }));
            }
            Err(e) => {
                tracing::error!(%e, "appeal file R2 upload failed for {file_name}");
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
            "封禁申诉",
            "上传申诉证据文件",
            &format!("申诉 {} — 上传 {} 个文件", appeal_id, uploaded.len()),
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

/// 获取单个文件的预签名下载 URL（管理员使用）
pub(crate) async fn get_appeal_file_url(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(file_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_review_ban_appeals(&actor) {
        return Err(forbidden());
    }

    let r2 = ctx.r2_storage.as_ref().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        Json(serde_json::json!({ "error": "文件服务未配置" })),
    ))?;

    #[derive(sqlx::FromRow)]
    struct FileRow {
        storage_key: String,
        file_name: String,
        content_type: String,
    }

    let file: FileRow = sqlx::query_as(
        "SELECT storage_key, file_name, content_type FROM appeal_files WHERE id = $1",
    )
    .bind(file_id)
    .fetch_optional(&ctx.db.pool)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "查询申诉文件失败");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "查询文件失败" })),
        )
    })?
    .ok_or((
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({ "error": "文件不存在" })),
    ))?;

    let presigned_url = r2.presigned_url(&file.storage_key, 3600);

    Ok(Json(serde_json::json!({
        "url": presigned_url,
        "file_name": file.file_name,
        "content_type": file.content_type,
    })))
}

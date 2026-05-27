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
    audit_service, ban_service, log_service, notification_service, permission_service,
    plugin_ban_service, r2_storage,
};

#[derive(Deserialize)]
pub(crate) struct BanBody {
    pub player: Option<String>,
    pub steam_id: String,
    pub ban_type: String,
    pub ip_address: Option<String>,
    pub reason: String,
}

#[derive(Deserialize)]
pub(crate) struct PluginBanBody {
    pub report_token: String,
    pub port: i32,
    pub ban_type: String,
    pub steam_id: Option<String>,
    pub ip_address: Option<String>,
    pub player: Option<String>,
    pub duration_minutes: i32,
    pub reason: String,
    pub operator_name: String,
}

#[derive(Deserialize)]
pub(crate) struct PluginBanPollBody {
    pub report_token: String,
    pub port: i32,
}

#[derive(Deserialize)]
pub(crate) struct PluginBanPollIncrementalBody {
    pub report_token: String,
    pub port: i32,
    pub cursor: Option<String>,
    pub limit: Option<i32>,
}

#[derive(Deserialize)]
pub(crate) struct PluginUnbanBody {
    pub report_token: String,
    pub port: i32,
    pub target: String,
    pub reason: Option<String>,
    pub operator_name: String,
    pub operator_steamid: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct PluginBanCheckBody {
    pub report_token: String,
    pub port: i32,
    pub steam_id: Option<String>,
    pub ip_address: Option<String>,
    pub player: Option<String>,
    pub server_port: Option<i32>,
}

pub(crate) async fn bans(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Query(query): Query<ListQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let _actor = current_operator(&ctx, &headers).await?;

    let result = ban_service::list_bans(&ctx.db, &query).await.map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "加载封禁列表失败" })),
        )
    })?;
    Ok(Json(
        serde_json::json!({ "items": result.items, "total": result.total, "page": result.page, "page_size": result.page_size }),
    ))
}

pub(crate) async fn get_ban(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let _actor = current_operator(&ctx, &headers).await?;
    let item = ban_service::get_ban(&ctx.db, id)
        .await
        .map_err(invalid_request)?;
    Ok(Json(serde_json::json!({ "item": item })))
}

pub(crate) async fn create_ban(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Json(body): Json<BanBody>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_create_ban(&actor) {
        return Err(forbidden());
    }

    let operator_name = actor.display_name.clone();
    let item = ban_service::create_ban(
        &ctx.db,
        &ctx.config,
        ban_service::CreateBanInput {
            player: body.player,
            steam_id: body.steam_id,
            ip_address: body.ip_address,
            ban_type: body.ban_type,
            reason: body.reason,
            operator_name: operator_name.clone(),
        },
    )
    .await
    .map_err(invalid_request)?;

    let log_target = item.player.as_deref().unwrap_or(&item.steam_id);
    let client_ip = extract_client_ip(&headers);
    let log_ip = item.ip_address.as_deref().unwrap_or(&client_ip);
    if let Err(e) = log_service::create_log(
        &ctx.db,
        &operator_name,
        "封禁管理",
        "添加封禁",
        log_target,
        log_ip,
    )
    .await
    {
        tracing::warn!(%e, "日志写入失败");
    }
    if let Err(e) = notification_service::notify_ban_create(
        &ctx.db,
        &ctx.notification_hub,
        &actor.id,
        &operator_name,
        item.player.as_deref(),
        &item.steam_id,
        &item.reason,
    )
    .await
    {
        tracing::warn!(%e, "ban create notification failed");
    }
    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({ "item": item })),
    ))
}

pub(crate) async fn update_ban(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(body): Json<BanBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    let record = ban_service::find_ban(&ctx.db, id)
        .await
        .map_err(invalid_request)?;
    if !permission_service::can_unban_record(&actor, &record) {
        return Err(forbidden());
    }

    let item = ban_service::update_ban(
        &ctx.db,
        &ctx.config,
        id,
        ban_service::UpdateBanInput {
            player: body.player,
            steam_id: body.steam_id,
            ip_address: body.ip_address,
            ban_type: body.ban_type,
            reason: body.reason,
        },
    )
    .await
    .map_err(invalid_request)?;
    let log_target = item.player.as_deref().unwrap_or(&item.steam_id);
    if let Err(e) = log_service::create_log(
        &ctx.db,
        &actor.display_name,
        "封禁管理",
        "编辑封禁",
        log_target,
        &extract_client_ip(&headers),
    )
    .await
    {
        tracing::warn!(%e, "日志写入失败");
    }
    Ok(Json(serde_json::json!({ "item": item })))
}

pub(crate) async fn delete_ban(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    let record = ban_service::find_ban(&ctx.db, id)
        .await
        .map_err(invalid_request)?;
    if !permission_service::can_unban_record(&actor, &record) {
        return Err(forbidden());
    }

    ban_service::delete_ban(&ctx.db, id)
        .await
        .map_err(invalid_request)?;
    let log_target = record.player.as_deref().unwrap_or(&record.steam_id);
    if let Err(e) = log_service::create_log(
        &ctx.db,
        &actor.display_name,
        "封禁管理",
        "删除封禁",
        log_target,
        &extract_client_ip(&headers),
    )
    .await
    {
        tracing::warn!(%e, "日志写入失败");
    }
    Ok(StatusCode::NO_CONTENT)
}

pub(crate) async fn unban_ban(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    let record = ban_service::find_ban(&ctx.db, id)
        .await
        .map_err(invalid_request)?;
    if !permission_service::can_unban_record(&actor, &record) {
        return Err(forbidden());
    }

    let item = ban_service::unban(&ctx.db, id, &actor.display_name)
        .await
        .map_err(invalid_request)?;
    let log_target = item.player.as_deref().unwrap_or(&item.steam_id);
    if let Err(e) = log_service::create_log(
        &ctx.db,
        &actor.display_name,
        "封禁管理",
        "解封",
        log_target,
        &extract_client_ip(&headers),
    )
    .await
    {
        tracing::warn!(%e, "日志写入失败");
    }
    Ok(Json(serde_json::json!({ "item": item })))
}

pub(crate) async fn list_ban_files(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(ban_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let _actor = current_operator(&ctx, &headers).await?;
    ban_service::find_ban(&ctx.db, ban_id)
        .await
        .map_err(invalid_request)?;

    #[derive(serde::Serialize, sqlx::FromRow)]
    struct FileRow {
        id: Uuid,
        file_name: String,
        file_size: i64,
        content_type: String,
        storage_key: String,
        category: String,
        uploaded_by: String,
        uploaded_at: chrono::DateTime<chrono::Utc>,
    }

    let files: Vec<FileRow> = sqlx::query_as(
        r#"SELECT id, file_name, file_size, content_type, storage_key, category, uploaded_by, uploaded_at
           FROM ban_files WHERE ban_id = $1 ORDER BY uploaded_at ASC"#,
    )
    .bind(ban_id)
    .fetch_all(&ctx.db.pool)
    .await
    .map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "加载封禁文件失败" })),
        )
    })?;

    let r2 = ctx.r2_storage.as_ref();
    let items: Vec<serde_json::Value> = files
        .into_iter()
        .map(|f| {
            let presigned_url = r2.map(|storage| storage.presigned_url(&f.storage_key, 3600));
            serde_json::json!({
                "id": f.id,
                "file_name": f.file_name,
                "file_size": f.file_size,
                "content_type": f.content_type,
                "category": f.category,
                "uploaded_by": f.uploaded_by,
                "uploaded_at": f.uploaded_at.to_rfc3339(),
                "url": presigned_url,
            })
        })
        .collect();

    Ok(Json(serde_json::json!({ "files": items })))
}

pub(crate) async fn get_ban_file_url(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(file_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let _actor = current_operator(&ctx, &headers).await?;

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

    let file: FileRow =
        sqlx::query_as("SELECT storage_key, file_name, content_type FROM ban_files WHERE id = $1")
            .bind(file_id)
            .fetch_optional(&ctx.db.pool)
            .await
            .map_err(|_| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": "查询文件失败" })),
                )
            })?
            .ok_or((
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "文件不存在" })),
            ))?;

    Ok(Json(serde_json::json!({
        "url": r2.presigned_url(&file.storage_key, 3600),
        "file_name": file.file_name,
        "content_type": file.content_type,
    })))
}

pub(crate) async fn upload_ban_files(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(ban_id): Path<Uuid>,
    mut multipart: Multipart,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_create_ban(&actor) {
        return Err(forbidden());
    }

    ban_service::find_ban(&ctx.db, ban_id)
        .await
        .map_err(invalid_request)?;

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
                tracing::warn!(%e, "读取封禁附件 multipart 字段失败");
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
            .upload_with_prefix("bans", ban_id, &file_name, &content_type, data)
            .await
        {
            Ok(key) => {
                let category = r2_storage::file_category(&file_name);
                if let Err(e) = sqlx::query(
                    r#"INSERT INTO ban_files (id, ban_id, file_name, file_size, content_type, storage_key, category, uploaded_by)
                       VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"#,
                )
                .bind(Uuid::new_v4())
                .bind(ban_id)
                .bind(&file_name)
                .bind(file_size as i64)
                .bind(&content_type)
                .bind(&key)
                .bind(category)
                .bind(&actor.display_name)
                .execute(&ctx.db.pool)
                .await
                {
                    tracing::warn!(%e, "写入封禁附件记录失败");
                }

                uploaded.push(serde_json::json!({
                    "file_name": file_name,
                    "file_size": file_size,
                    "category": category,
                }));
            }
            Err(e) => {
                tracing::error!(%e, "ban file R2 upload failed for {file_name}");
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

    Ok(Json(serde_json::json!({
        "uploaded": uploaded,
        "errors": if errors.is_empty() { None } else { Some(errors) },
    })))
}

pub(crate) async fn create_plugin_ban(
    State(ctx): State<AppCtx>,
    Json(body): Json<PluginBanBody>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<serde_json::Value>)> {
    let item = plugin_ban_service::create_plugin_ban(
        &ctx.db,
        plugin_ban_service::PluginBanInput {
            report_token: body.report_token,
            port: body.port,
            ban_type: body.ban_type,
            steam_id: body.steam_id,
            ip_address: body.ip_address,
            player: body.player,
            duration_minutes: body.duration_minutes,
            reason: body.reason,
            operator_name: body.operator_name,
        },
    )
    .await
    .map_err(invalid_request)?;

    if let Err(e) = notification_service::notify_plugin_ban(
        &ctx.db,
        &ctx.notification_hub,
        item.server_name.as_deref().unwrap_or("未知服务器"),
        item.player.as_deref(),
        &item.steam_id,
        &item.reason,
    )
    .await
    {
        tracing::warn!(%e, "plugin ban notification failed");
    }

    let duration_text = if item.duration_minutes == 0 {
        "永久".to_string()
    } else {
        format!("{}分钟", item.duration_minutes)
    };
    let log_target = format!(
        "{} | SteamID: {} | 时长: {} | 理由: {}",
        item.player.as_deref().unwrap_or("未知玩家"),
        item.steam_id,
        duration_text,
        item.reason
    );
    let server_info = item.server_name.as_deref().unwrap_or("未知服务器");
    if let Err(e) = log_service::create_log(
        &ctx.db,
        &item.operator_name,
        "游戏封禁",
        &format!("通过 {} 封禁玩家", server_info),
        &log_target,
        "",
    )
    .await
    {
        tracing::warn!(%e, "插件封禁审计日志写入失败");
    }
    if let Err(e) = audit_service::write_audit_log(
        &ctx.db,
        audit_service::AuditLogInput {
            operation: "ban".to_string(),
            target: item.steam_id.clone(),
            target_type: item.ban_type.clone(),
            player_name: item.player.clone(),
            reason: Some(item.reason.clone()),
            duration_minutes: Some(item.duration_minutes),
            operator_name: item.operator_name.clone(),
            operator_steamid: None,
            source: "game_plugin".to_string(),
            server_id: item.server_id,
            server_name: item.server_name.clone(),
            server_port: item.server_port,
            success: true,
            message: Some(format!("封禁成功，ID: {}", item.id)),
            idempotency_key: None,
        },
    )
    .await
    {
        tracing::warn!(%e, "plugin ban audit log write failed");
    }

    let kick_message = if item.duration_minutes == 0 {
        format!("你已被永久封禁，原因：{}", item.reason)
    } else {
        format!(
            "你已被封禁，原因：{}，到期时间：{}",
            item.reason,
            item.expires_at.as_deref().unwrap_or("未知")
        )
    };
    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({ "item": item, "kick_message": kick_message })),
    ))
}

pub(crate) async fn poll_plugin_bans(
    State(ctx): State<AppCtx>,
    Json(body): Json<PluginBanPollBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let items = plugin_ban_service::poll_active_bans(
        &ctx.db,
        plugin_ban_service::PluginBanPollInput {
            report_token: body.report_token,
            port: body.port,
        },
    )
    .await
    .map_err(invalid_request)?;
    Ok(Json(serde_json::json!({ "items": items })))
}

pub(crate) async fn poll_plugin_bans_incremental(
    State(ctx): State<AppCtx>,
    Json(body): Json<PluginBanPollIncrementalBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let result = plugin_ban_service::poll_active_bans_incremental(
        &ctx.db,
        plugin_ban_service::PluginBanPollInput {
            report_token: body.report_token,
            port: body.port,
        },
        body.cursor,
        body.limit,
    )
    .await
    .map_err(invalid_request)?;
    Ok(Json(serde_json::json!({
        "items": result.items,
        "cursor": result.cursor,
        "has_more": result.has_more,
        "total_count": result.total_count
    })))
}

pub(crate) async fn unban_plugin_ban(
    State(ctx): State<AppCtx>,
    Json(body): Json<PluginUnbanBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let item = plugin_ban_service::unban_plugin_target(
        &ctx.db,
        plugin_ban_service::PluginUnbanInput {
            report_token: body.report_token,
            port: body.port,
            target: body.target,
            reason: body.reason,
            operator_name: body.operator_name,
            operator_steamid: body.operator_steamid,
        },
    )
    .await
    .map_err(invalid_request)?;

    let log_target = format!(
        "{} | SteamID: {} | 理由: {}",
        item.player.as_deref().unwrap_or("未知玩家"),
        item.steam_id,
        item.removed_reason.as_deref().unwrap_or("未填写")
    );
    let unban_operator = item.removed_by.as_deref().unwrap_or("未知管理员");
    if let Err(e) = log_service::create_log(
        &ctx.db,
        unban_operator,
        "游戏解封",
        "通过游戏服务器解封玩家",
        &log_target,
        "",
    )
    .await
    {
        tracing::warn!(%e, "插件解封审计日志写入失败");
    }
    if let Err(e) = audit_service::write_audit_log(
        &ctx.db,
        audit_service::AuditLogInput {
            operation: "unban".to_string(),
            target: item.steam_id.clone(),
            target_type: item.ban_type.clone(),
            player_name: item.player.clone(),
            reason: item.removed_reason.clone(),
            duration_minutes: None,
            operator_name: unban_operator.to_string(),
            operator_steamid: None,
            source: "game_plugin".to_string(),
            server_id: item.server_id,
            server_name: item.server_name.clone(),
            server_port: item.server_port,
            success: true,
            message: Some(format!("解封成功，ID: {}", item.id)),
            idempotency_key: None,
        },
    )
    .await
    {
        tracing::warn!(%e, "plugin unban audit log write failed");
    }

    Ok(Json(serde_json::json!({ "item": item })))
}

pub(crate) async fn check_plugin_ban(
    State(ctx): State<AppCtx>,
    Json(body): Json<PluginBanCheckBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let result = plugin_ban_service::check_plugin_ban(
        &ctx.db,
        &ctx.access_snapshot,
        plugin_ban_service::PluginBanCheckInput {
            report_token: body.report_token,
            port: body.port,
            steam_id: body.steam_id,
            ip_address: body.ip_address,
            player: body.player,
            server_port: body.server_port,
        },
    )
    .await
    .map_err(invalid_request)?;
    Ok(Json(serde_json::json!(result)))
}

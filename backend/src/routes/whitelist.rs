use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    Json,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::routes::{current_operator, AppCtx, AppError, ListQuery};
use crate::services::rate_limit_service::extract_client_ip;
use crate::services::{
    audit_service, log_service, lumi_bot_service, permission_service, whitelist_qq_service,
    whitelist_service,
};

#[derive(Deserialize)]
#[allow(dead_code)]
pub(crate) struct WhitelistBody {
    pub steam_input: String,
    pub nickname: String,
    pub operator_name: Option<String>,
    pub force: Option<bool>,
    pub reason: Option<String>,
    /// 白名单期限天数（7/30/120），不传或 0 表示永久
    pub duration_days: Option<i32>,
}

#[derive(Deserialize)]
#[allow(dead_code)]
pub(crate) struct RejectWhitelistBody {
    pub reason: String,
    pub operator_name: Option<String>,
}

#[derive(Deserialize)]
#[allow(dead_code)]
pub(crate) struct WhitelistActionBody {
    pub operator_name: Option<String>,
    pub reason: Option<String>,
    pub force: Option<bool>,
    /// 白名单期限天数（7/30/120），不传或 0 表示永久
    pub duration_days: Option<i32>,
}

#[derive(Deserialize)]
pub(crate) struct RefreshSteamNamesBody {
    pub status: Option<String>,
}

#[derive(Deserialize)]
#[allow(dead_code)]
pub(crate) struct QqChatBody {
    /// 管理员发送的消息内容
    pub content: Option<String>,
}

fn optional_client_ip(headers: &HeaderMap) -> Option<String> {
    let ip = extract_client_ip(headers);
    (!ip.trim().is_empty()).then_some(ip)
}

struct WhitelistAuditEvent<'a> {
    operation: &'a str,
    item: &'a whitelist_service::WhitelistItem,
    reason: Option<String>,
    message: String,
    details: serde_json::Value,
}

async fn write_whitelist_audit(
    ctx: &AppCtx,
    headers: &HeaderMap,
    actor: &crate::models::Operator,
    event: WhitelistAuditEvent<'_>,
) {
    if let Err(e) = audit_service::write_audit_log_with_context(
        &ctx.db,
        audit_service::AuditLogInput {
            operation: event.operation.to_string(),
            target: event.item.steamid64.clone(),
            target_type: "whitelist".to_string(),
            player_name: Some(event.item.nickname.clone()),
            reason: event.reason,
            duration_minutes: None,
            operator_id: Some(actor.id),
            operator_name: actor.display_name.clone(),
            operator_steamid: None,
            source: "web".to_string(),
            server_id: None,
            server_name: None,
            server_port: None,
            success: true,
            message: Some(event.message),
            idempotency_key: None,
        },
        optional_client_ip(headers),
        Some(event.details),
    )
    .await
    {
        tracing::warn!(%e, "whitelist audit log write failed");
    }
}

async fn write_whitelist_batch_audit(
    ctx: &AppCtx,
    headers: &HeaderMap,
    actor: &crate::models::Operator,
    operation: &str,
    target: impl Into<String>,
    message: impl Into<String>,
    details: serde_json::Value,
) {
    if let Err(e) = audit_service::write_audit_log_with_context(
        &ctx.db,
        audit_service::AuditLogInput {
            operation: operation.to_string(),
            target: target.into(),
            target_type: "whitelist".to_string(),
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
            message: Some(message.into()),
            idempotency_key: None,
        },
        optional_client_ip(headers),
        Some(details),
    )
    .await
    {
        tracing::warn!(%e, "whitelist batch audit log write failed");
    }
}

pub(crate) async fn whitelist(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Query(query): Query<ListQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    let _actor = current_operator(&ctx, &headers).await?;
    let result = whitelist_service::list_whitelist(&ctx.db, &query)
        .await
        .map_err(AppError::internal)?;
    Ok(Json(
        serde_json::json!({ "items": result.items, "total": result.total, "page": result.page, "page_size": result.page_size }),
    ))
}

pub(crate) async fn create_whitelist(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Json(body): Json<WhitelistBody>,
) -> Result<(axum::http::StatusCode, Json<serde_json::Value>), AppError> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_manage_whitelist_manually(&actor) {
        return Err(AppError::forbidden());
    }
    let operator_name = actor.display_name.clone();
    let resolver = &ctx.steam_resolver;
    let force = body.force.unwrap_or(false);
    let (mut item, _) = whitelist_service::create_manual_whitelist(
        &ctx.db,
        whitelist_service::ManualWhitelistInput {
            nickname: body.nickname,
            steam_input: body.steam_input,
            force,
            force_reason: body.reason.clone(),
            duration_days: body.duration_days,
        },
        &operator_name,
        resolver,
    )
    .await
    .map_err(AppError::bad_request)?;
    log_service::log_action(
        &ctx.db,
        &operator_name,
        "白名单管理",
        "手动添加白名单",
        &item.nickname,
        &extract_client_ip(&headers),
    )
    .await;
    item.risk_profile = whitelist_service::risk_profile_for_whitelist(&ctx.db, item.id)
        .await
        .ok();
    write_whitelist_audit(
        &ctx,
        &headers,
        &actor,
        WhitelistAuditEvent {
            operation: "whitelist_add",
            item: &item,
            reason: item.approval_reason.clone(),
            message: format!("后台手动添加白名单，ID: {}", item.id),
            details: serde_json::json!({
                "action": "create_manual_whitelist",
                "whitelist_id": item.id,
                "steamid64": item.steamid64,
                "steamid": item.steamid,
                "steamid3": item.steamid3,
                "nickname": item.nickname,
                "status": item.status,
                "approved_at": item.approved_at,
                "approved_by": item.approved_by,
                "duration_days": item.duration_days,
                "expires_at": item.expires_at,
                "force_approve": force,
                "risk_profile": item.risk_profile,
                "operator_username": actor.username,
                "operator_role": actor.role,
            }),
        },
    )
    .await;
    Ok((
        axum::http::StatusCode::CREATED,
        Json(serde_json::json!({"item": item})),
    ))
}

pub(crate) async fn approve_whitelist_request(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(body): Json<WhitelistActionBody>,
) -> Result<Json<serde_json::Value>, AppError> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_review_whitelist(&actor) {
        return Err(AppError::forbidden());
    }
    let operator_name = actor.display_name.clone();
    let force = body.force.unwrap_or(false);
    let mut item = whitelist_service::approve_whitelist(
        &ctx.db,
        id,
        whitelist_service::ApproveWhitelistInput {
            operator_name: &operator_name,
            reason: body.reason.as_deref(),
            force,
            via: "web", // 后台网页审批
            duration_days: body.duration_days,
        },
    )
    .await
    .map_err(AppError::bad_request)?;
    item.risk_profile = whitelist_service::risk_profile_for_whitelist(&ctx.db, item.id)
        .await
        .ok();
    log_service::log_action(
        &ctx.db,
        &operator_name,
        "白名单管理",
        "通过白名单申请",
        &item.nickname,
        &extract_client_ip(&headers),
    )
    .await;
    write_whitelist_audit(
        &ctx,
        &headers,
        &actor,
        WhitelistAuditEvent {
            operation: "whitelist_approve",
            item: &item,
            reason: item.approval_reason.clone(),
            message: format!("后台通过白名单申请，ID: {}", item.id),
            details: serde_json::json!({
                "action": "approve_whitelist",
                "whitelist_id": item.id,
                "steamid64": item.steamid64,
                "nickname": item.nickname,
                "status": item.status,
                "approval_reason": item.approval_reason,
                "approved_at": item.approved_at,
                "approved_by": item.approved_by,
                "duration_days": item.duration_days,
                "expires_at": item.expires_at,
                "force_approve": force,
                "risk_profile": item.risk_profile,
                "operator_username": actor.username,
                "operator_role": actor.role,
            }),
        },
    )
    .await;
    Ok(Json(serde_json::json!({"item": item})))
}

pub(crate) async fn reject_whitelist_request(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(body): Json<RejectWhitelistBody>,
) -> Result<Json<serde_json::Value>, AppError> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_review_whitelist(&actor) {
        return Err(AppError::forbidden());
    }
    let operator_name = actor.display_name.clone();
    let item =
        whitelist_service::reject_whitelist(&ctx.db, id, &body.reason, &operator_name, "web")
            .await
            .map_err(AppError::bad_request)?;
    log_service::log_action(
        &ctx.db,
        &operator_name,
        "白名单管理",
        "拒绝白名单申请",
        &item.nickname,
        &extract_client_ip(&headers),
    )
    .await;
    write_whitelist_audit(
        &ctx,
        &headers,
        &actor,
        WhitelistAuditEvent {
            operation: "whitelist_reject",
            item: &item,
            reason: Some(body.reason),
            message: format!("后台拒绝白名单申请，ID: {}", item.id),
            details: serde_json::json!({
                "action": "reject_whitelist",
                "whitelist_id": item.id,
                "steamid64": item.steamid64,
                "nickname": item.nickname,
                "status": item.status,
                "rejection_reason": item.rejection_reason,
                "rejected_at": item.rejected_at,
                "rejected_by": item.rejected_by,
                "operator_username": actor.username,
                "operator_role": actor.role,
            }),
        },
    )
    .await;
    Ok(Json(serde_json::json!({"item": item})))
}

pub(crate) async fn restore_whitelist_request(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(body): Json<WhitelistActionBody>,
) -> Result<Json<serde_json::Value>, AppError> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_review_whitelist(&actor) {
        return Err(AppError::forbidden());
    }
    let operator_name = actor.display_name.clone();
    let force = body.force.unwrap_or(false);
    let mut item = whitelist_service::restore_whitelist(
        &ctx.db,
        id,
        &operator_name,
        body.reason.as_deref(),
        force,
        body.duration_days,
    )
    .await
    .map_err(AppError::bad_request)?;
    item.risk_profile = whitelist_service::risk_profile_for_whitelist(&ctx.db, item.id)
        .await
        .ok();
    log_service::log_action(
        &ctx.db,
        &operator_name,
        "白名单管理",
        "恢复白名单通过",
        &item.nickname,
        &extract_client_ip(&headers),
    )
    .await;
    write_whitelist_audit(
        &ctx,
        &headers,
        &actor,
        WhitelistAuditEvent {
            operation: "whitelist_restore",
            item: &item,
            reason: item.approval_reason.clone(),
            message: format!("后台恢复白名单通过状态，ID: {}", item.id),
            details: serde_json::json!({
                "action": "restore_whitelist",
                "whitelist_id": item.id,
                "steamid64": item.steamid64,
                "nickname": item.nickname,
                "status": item.status,
                "approved_at": item.approved_at,
                "approved_by": item.approved_by,
                "approval_reason": item.approval_reason,
                "force_approve": force,
                "risk_profile": item.risk_profile,
                "operator_username": actor.username,
                "operator_role": actor.role,
            }),
        },
    )
    .await;
    Ok(Json(serde_json::json!({"item": item})))
}

pub(crate) async fn revoke_whitelist_request(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(_body): Json<WhitelistActionBody>,
) -> Result<Json<serde_json::Value>, AppError> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_revoke_whitelist(&actor) {
        return Err(AppError::forbidden());
    }
    let operator_name = actor.display_name.clone();
    let item = whitelist_service::revoke_whitelist(&ctx.db, id, &operator_name)
        .await
        .map_err(AppError::bad_request)?;
    log_service::log_action(
        &ctx.db,
        &operator_name,
        "白名单管理",
        "删除白名单审核",
        &item.nickname,
        &extract_client_ip(&headers),
    )
    .await;
    write_whitelist_audit(
        &ctx,
        &headers,
        &actor,
        WhitelistAuditEvent {
            operation: "whitelist_revoke",
            item: &item,
            reason: None,
            message: format!("后台撤销白名单，ID: {}", item.id),
            details: serde_json::json!({
                "action": "revoke_whitelist",
                "whitelist_id": item.id,
                "steamid64": item.steamid64,
                "nickname": item.nickname,
                "status": item.status,
                "operator_username": actor.username,
                "operator_role": actor.role,
            }),
        },
    )
    .await;
    Ok(Json(serde_json::json!({"item": item})))
}

/// 刷新单条白名单记录的Steam名称
pub(crate) async fn refresh_single_steam_name(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_review_whitelist(&actor) {
        return Err(AppError::forbidden());
    }
    let resolver = &ctx.steam_resolver;
    let steam_name = whitelist_service::update_steam_persona_name(&ctx.db, id, resolver)
        .await
        .map_err(AppError::bad_request)?;
    Ok(Json(
        serde_json::json!({ "steam_persona_name": steam_name }),
    ))
}

/// 批量刷新白名单记录的Steam名称
pub(crate) async fn refresh_all_steam_names(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Json(body): Json<RefreshSteamNamesBody>,
) -> Result<Json<serde_json::Value>, AppError> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_review_whitelist(&actor) {
        return Err(AppError::forbidden());
    }
    let resolver = &ctx.steam_resolver;
    let updated_count = whitelist_service::refresh_all_steam_persona_names(
        &ctx.db,
        resolver,
        body.status.as_deref(),
    )
    .await
    .map_err(AppError::bad_request)?;
    let operator_name = actor.display_name.clone();
    log_service::log_action(
        &ctx.db,
        &operator_name,
        "白名单管理",
        "批量刷新Steam名称",
        &format!("更新了{}条记录", updated_count),
        &extract_client_ip(&headers),
    )
    .await;
    write_whitelist_batch_audit(
        &ctx,
        &headers,
        &actor,
        "whitelist_refresh_names",
        body.status.as_deref().unwrap_or("all"),
        format!("批量刷新白名单 Steam 名称，更新 {} 条", updated_count),
        serde_json::json!({
            "action": "refresh_all_steam_persona_names",
            "status_filter": body.status,
            "updated_count": updated_count,
            "operator_username": actor.username,
            "operator_role": actor.role,
        }),
    )
    .await;
    Ok(Json(serde_json::json!({ "updated_count": updated_count })))
}

/// 查询指定 Steam 的 QQ 绑定信息（管理后台玩家详情/审核弹窗使用）。
pub(crate) async fn get_qq_binding(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(steamid64): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let actor = current_operator(&ctx, &headers).await?;
    // 聊天记录含玩家私聊内容，仅 developer/admin 可读；其余审核角色只能看到绑定摘要。
    let can_view_chat = permission_service::can_manage_whitelist_manually(&actor);
    if !permission_service::can_review_whitelist(&actor) {
        return Err(AppError::forbidden());
    }
    let binding = whitelist_qq_service::find_binding_by_steamid64(&ctx.db, &steamid64)
        .await
        .map_err(AppError::internal)?;
    let qq_count = match binding.as_ref() {
        Some(b) => whitelist_qq_service::count_bindings_for_qq(&ctx.db, &b.qq_openid)
            .await
            .map_err(AppError::internal)?,
        None => 0,
    };
    // 同 QQ 绑定的其他 Steam，供管理员排查多开
    let sibling_bindings = match binding.as_ref() {
        Some(b) => whitelist_qq_service::find_bindings_by_openid(&ctx.db, &b.qq_openid)
            .await
            .map_err(AppError::internal)?
            .into_iter()
            .filter(|item| item.steamid64 != steamid64)
            .collect::<Vec<_>>(),
        None => Vec::new(),
    };
    let chat_messages = if can_view_chat {
        whitelist_qq_service::list_chat_messages(&ctx.db, &steamid64, 50)
            .await
            .map_err(AppError::internal)?
    } else {
        Vec::new()
    };
    Ok(Json(serde_json::json!({
        "binding": binding,
        "qq_binding_count": qq_count,
        "sibling_bindings": sibling_bindings,
        "chat_messages": chat_messages,
    })))
}

/// 管理员解绑 Steam↔QQ（换绑前必须先解绑）。
pub(crate) async fn delete_qq_binding(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(steamid64): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_manage_whitelist_manually(&actor) {
        return Err(AppError::forbidden());
    }
    let removed = whitelist_qq_service::delete_binding(&ctx.db, &steamid64)
        .await
        .map_err(AppError::internal)?;
    let operator_name = actor.display_name.clone();
    let Some(binding) = removed else {
        return Err(AppError::bad_request(anyhow::anyhow!(
            "该 Steam 账号没有 QQ 绑定"
        )));
    };
    log_service::log_action(
        &ctx.db,
        &operator_name,
        "白名单管理",
        "解绑QQ绑定",
        &steamid64,
        &extract_client_ip(&headers),
    )
    .await;
    if let Err(e) = audit_service::write_audit_log_with_context(
        &ctx.db,
        audit_service::AuditLogInput {
            operation: "whitelist_qq_unbind".to_string(),
            target: steamid64.clone(),
            target_type: "whitelist".to_string(),
            player_name: None,
            reason: None,
            duration_minutes: None,
            operator_id: Some(actor.id),
            operator_name: operator_name.clone(),
            operator_steamid: None,
            source: "web".to_string(),
            server_id: None,
            server_name: None,
            server_port: None,
            success: true,
            message: Some(format!("解绑 Steam↔QQ，原 openid: {}", binding.qq_openid)),
            idempotency_key: None,
        },
        None,
        Some(serde_json::json!({
            "action": "delete_qq_binding",
            "steamid64": steamid64,
            "qq_openid": binding.qq_openid,
            "operator_username": actor.username,
            "operator_role": actor.role,
        })),
    )
    .await
    {
        tracing::warn!(%e, "QQ 解绑审计写入失败");
    }
    Ok(Json(serde_json::json!({ "removed": binding })))
}

/// 查询与某 Steam 的 QQ 私聊聊天记录。
pub(crate) async fn list_qq_chat(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(steamid64): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let actor = current_operator(&ctx, &headers).await?;
    // 玩家私聊记录属敏感信息，仅 developer/admin 可读
    if !permission_service::can_manage_whitelist_manually(&actor) {
        return Err(AppError::forbidden());
    }
    let messages = whitelist_qq_service::list_chat_messages(&ctx.db, &steamid64, 100)
        .await
        .map_err(AppError::internal)?;
    Ok(Json(serde_json::json!({ "messages": messages })))
}

/// 管理员通过 LumiBot 私聊玩家（QQ C2C）。
pub(crate) async fn send_qq_chat(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(steamid64): Path<String>,
    Json(body): Json<QqChatBody>,
) -> Result<Json<serde_json::Value>, AppError> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_manage_whitelist_manually(&actor) {
        return Err(AppError::forbidden());
    }
    let binding = whitelist_qq_service::find_binding_by_steamid64(&ctx.db, &steamid64)
        .await
        .map_err(AppError::internal)?
        .ok_or_else(|| AppError::bad_request(anyhow::anyhow!("该 Steam 账号未绑定 QQ")))?;

    let content = body
        .content
        .as_deref()
        .map(str::trim)
        .filter(|c| !c.is_empty())
        .ok_or_else(|| AppError::bad_request(anyhow::anyhow!("请输入消息内容")))?
        .to_string();
    if content.chars().count() > 500 {
        return Err(AppError::bad_request(anyhow::anyhow!(
            "消息内容不能超过 500 字"
        )));
    }

    let operator_name = actor.display_name.clone();
    let result = lumi_bot_service::send_c2c_message(
        &ctx.config,
        &binding.qq_openid,
        &content,
        &operator_name,
    )
    .await;

    let (status, error) = match &result {
        Ok(_) => ("sent", None),
        Err(e) => ("failed", Some(e.to_string())),
    };
    if let Err(e) = whitelist_qq_service::record_chat_message(
        &ctx.db,
        Some(&steamid64),
        &binding.qq_openid,
        "admin_to_player",
        &content,
        status,
        error.as_deref(),
        Some(actor.id),
        Some(&operator_name),
    )
    .await
    {
        tracing::warn!(%e, "写入 QQ 聊天记录失败");
    }

    log_service::log_action(
        &ctx.db,
        &operator_name,
        "白名单管理",
        "QQ私聊玩家",
        &steamid64,
        &extract_client_ip(&headers),
    )
    .await;

    match result {
        Ok(response) => Ok(Json(serde_json::json!({
            "status": status,
            "response": response,
        }))),
        Err(e) => Err(AppError::bad_request(anyhow::anyhow!("发送失败：{e}"))),
    }
}

use axum::{
    extract::{Path, Query, State, WebSocketUpgrade, ws::{Message, WebSocket}},
    response::IntoResponse,
    Json,
};
use futures::{SinkExt, StreamExt};
use serde::Deserialize;
use std::collections::HashMap;
use uuid::Uuid;

use crate::routes::AppCtx;
use crate::services::notification_service;

#[derive(Deserialize)]
pub(crate) struct NotifQuery {
    page: Option<i64>,
    page_size: Option<i64>,
}

pub(crate) async fn list_notifications(
    State(ctx): State<AppCtx>,
    Query(query): Query<NotifQuery>,
    headers: axum::http::HeaderMap,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, Json<serde_json::Value>)> {
    let actor = crate::routes::current_operator(&ctx, &headers).await?;

    let page = query.page.unwrap_or(1);
    let page_size = query.page_size.unwrap_or(20);

    let (items, total) = notification_service::list_notifications(
        &ctx.db,
        actor.id,
        page,
        page_size,
    )
    .await
    .map_err(|_| {
        (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "查询失败"})),
        )
    })?;

    Ok(Json(serde_json::json!({
        "items": items,
        "total": total,
        "page": page,
        "page_size": page_size,
    })))
}

pub(crate) async fn unread_count(
    State(ctx): State<AppCtx>,
    headers: axum::http::HeaderMap,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, Json<serde_json::Value>)> {
    let actor = crate::routes::current_operator(&ctx, &headers).await?;

    let count = notification_service::get_unread_count(&ctx.db, actor.id)
        .await
        .map_err(|_| {
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "查询失败"})),
            )
        })?;

    Ok(Json(serde_json::json!({ "count": count })))
}

pub(crate) async fn mark_read(
    State(ctx): State<AppCtx>,
    Path(id): Path<Uuid>,
    headers: axum::http::HeaderMap,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, Json<serde_json::Value>)> {
    let actor = crate::routes::current_operator(&ctx, &headers).await?;

    notification_service::mark_read(&ctx.db, id, actor.id)
        .await
        .map_err(|_| {
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "操作失败"})),
            )
        })?;

    Ok(Json(serde_json::json!({ "ok": true })))
}

pub(crate) async fn mark_all_read(
    State(ctx): State<AppCtx>,
    headers: axum::http::HeaderMap,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, Json<serde_json::Value>)> {
    let actor = crate::routes::current_operator(&ctx, &headers).await?;

    let count = notification_service::mark_all_read(&ctx.db, actor.id)
        .await
        .map_err(|_| {
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "操作失败"})),
            )
        })?;

    Ok(Json(serde_json::json!({ "count": count })))
}

pub(crate) async fn ws_handler(
    State(ctx): State<AppCtx>,
    client_upgrade: WebSocketUpgrade,
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let token_str = params.get("token").cloned().unwrap_or_default();
    let token = Uuid::parse_str(&token_str).ok();
    let db = ctx.db.clone();
    let hub = ctx.notification_hub.clone();

    client_upgrade.on_upgrade(move |socket| handle_ws(socket, token, db, hub))
}

async fn handle_ws(
    socket: WebSocket,
    token: Option<Uuid>,
    db: crate::db::Database,
    hub: notification_service::NotificationHub,
) {
    let Some(token) = token else {
        let _ = socket.close().await;
        return;
    };

    let user_id = match crate::services::auth_service::current_session(&db, token).await {
        Ok(session) => session.user_id,
        Err(_) => {
            let _ = socket.close().await;
            return;
        }
    };

    let row: Option<(bool,)> = sqlx::query_as("SELECT enabled FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_optional(&db.pool)
        .await
        .ok()
        .flatten();

    let Some((true,)) = row else {
        let _ = socket.close().await;
        return;
    };

    let (mut sender, mut receiver) = socket.split();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<serde_json::Value>();

    notification_service::register_connection(&hub, user_id, tx.clone()).await;

    let send_hub = hub.clone();
    let send_tx = tx.clone();
    let send_user_id = user_id;
    let send_task = tokio::spawn(async move {
        let mut ping_interval = tokio::time::interval(std::time::Duration::from_secs(30));
        loop {
            tokio::select! {
                msg = rx.recv() => {
                    match msg {
                        Some(msg) => {
                            if let Ok(text) = serde_json::to_string(&msg) {
                                if sender.send(Message::Text(text)).await.is_err() {
                                    break;
                                }
                            }
                        }
                        None => break,
                    }
                }
                _ = ping_interval.tick() => {
                    if sender.send(Message::Ping(vec![])).await.is_err() {
                        break;
                    }
                }
            }
        }
        notification_service::unregister_connection(&send_hub, &send_user_id, &send_tx).await;
    });

    let recv_hub = hub.clone();
    let recv_user_id = user_id;
    let recv_tx = tx.clone();
    let recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            match msg {
                Message::Close(_) => break,
                Message::Ping(_) => {}
                _ => {}
            }
        }
        notification_service::unregister_connection(&recv_hub, &recv_user_id, &recv_tx).await;
    });

    // 发送任务已内置定时 Ping（见上方 send_task）
    tokio::select! {
        _ = send_task => {},
        _ = recv_task => {},
    }
}

use crate::db::Database;
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Data models
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Clone)]
pub struct NotificationItem {
    pub id: Uuid,
    #[serde(rename = "type")]
    pub notification_type: String,
    pub title: String,
    pub message: String,
    pub link: Option<String>,
    pub read: bool,
    pub created_at: String,
}

#[derive(sqlx::FromRow)]
struct NotificationRow {
    id: Uuid,
    user_id: Uuid,
    r#type: String,
    title: String,
    message: String,
    link: Option<String>,
    read: bool,
    created_at: DateTime<Utc>,
}

fn map_row(row: NotificationRow) -> NotificationItem {
    NotificationItem {
        id: row.id,
        notification_type: row.r#type,
        title: row.title,
        message: row.message,
        link: row.link,
        read: row.read,
        created_at: row.created_at.to_rfc3339(),
    }
}

// ---------------------------------------------------------------------------
// WebSocket Hub
// ---------------------------------------------------------------------------

pub type NotificationHub =
    Arc<RwLock<HashMap<Uuid, Vec<tokio::sync::mpsc::UnboundedSender<serde_json::Value>>>>>;

pub fn create_notification_hub() -> NotificationHub {
    Arc::new(RwLock::new(HashMap::new()))
}

pub async fn register_connection(
    hub: &NotificationHub,
    user_id: Uuid,
    tx: tokio::sync::mpsc::UnboundedSender<serde_json::Value>,
) {
    hub.write().await.entry(user_id).or_default().push(tx);
}

pub async fn unregister_connection(
    hub: &NotificationHub,
    user_id: &Uuid,
    tx: &tokio::sync::mpsc::UnboundedSender<serde_json::Value>,
) {
    let mut hub = hub.write().await;
    if let Some(senders) = hub.get_mut(user_id) {
        senders.retain(|s| !s.same_channel(tx));
        if senders.is_empty() {
            hub.remove(user_id);
        }
    }
}

async fn push_to_user(hub: &NotificationHub, user_id: &Uuid, item: &NotificationItem) {
    let msg = serde_json::json!({
        "type": "notification",
        "data": item,
    });
    let hub = hub.read().await;
    if let Some(senders) = hub.get(user_id) {
        for tx in senders {
            let _ = tx.send(msg.clone());
        }
    }
}

// ---------------------------------------------------------------------------
// CRUD
// ---------------------------------------------------------------------------

pub async fn list_notifications(
    db: &Database,
    user_id: Uuid,
    page: i64,
    page_size: i64,
) -> anyhow::Result<(Vec<NotificationItem>, i64)> {
    let page = page.max(1);
    let page_size = page_size.clamp(1, 100);
    let offset = (page - 1) * page_size;

    let total: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM notifications WHERE user_id = $1")
        .bind(user_id)
        .fetch_one(&db.pool)
        .await?;

    let rows: Vec<NotificationRow> = sqlx::query_as(
        r#"SELECT id, user_id, type, title, message, link, read, created_at
           FROM notifications WHERE user_id = $1
           ORDER BY created_at DESC LIMIT $2 OFFSET $3"#,
    )
    .bind(user_id)
    .bind(page_size)
    .bind(offset)
    .fetch_all(&db.pool)
    .await?;

    Ok((rows.into_iter().map(map_row).collect(), total.0))
}

pub async fn get_unread_count(db: &Database, user_id: Uuid) -> anyhow::Result<i64> {
    let count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM notifications WHERE user_id = $1 AND read = false")
            .bind(user_id)
            .fetch_one(&db.pool)
            .await?;
    Ok(count.0)
}

pub async fn mark_read(
    db: &Database,
    notification_id: Uuid,
    user_id: Uuid,
) -> anyhow::Result<bool> {
    let result = sqlx::query(
        "UPDATE notifications SET read = true WHERE id = $1 AND user_id = $2 AND read = false",
    )
    .bind(notification_id)
    .bind(user_id)
    .execute(&db.pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn mark_all_read(db: &Database, user_id: Uuid) -> anyhow::Result<u64> {
    let result =
        sqlx::query("UPDATE notifications SET read = true WHERE user_id = $1 AND read = false")
            .bind(user_id)
            .execute(&db.pool)
            .await?;
    Ok(result.rows_affected())
}

// ---------------------------------------------------------------------------
// Notification creation (with WebSocket push)
// ---------------------------------------------------------------------------

pub async fn notify_all_admins(
    db: &Database,
    hub: &NotificationHub,
    exclude_user_id: Option<&Uuid>,
    notification_type: &str,
    title: &str,
    message: &str,
    link: Option<&str>,
) -> anyhow::Result<()> {
    let admins: Vec<(Uuid,)> = sqlx::query_as(
        r#"SELECT id FROM users WHERE role IN ('developer', 'admin', 'normal') AND enabled = true"#,
    )
    .fetch_all(&db.pool)
    .await?;

    let user_ids: Vec<Uuid> = admins
        .into_iter()
        .map(|(uid,)| uid)
        .filter(|uid| exclude_user_id != Some(uid))
        .collect();

    if user_ids.is_empty() {
        return Ok(());
    }

    // 批量插入通知，避免 N+1 查询
    let rows: Vec<NotificationRow> = sqlx::query_as(
        r#"INSERT INTO notifications (id, user_id, type, title, message, link, read, created_at)
           SELECT gen_random_uuid(), u.id, $1, $2, $3, $4, false, now()
           FROM UNNEST($5::uuid[]) AS u(id)
           RETURNING id, user_id, type, title, message, link, read, created_at"#,
    )
    .bind(notification_type)
    .bind(title)
    .bind(message)
    .bind(link)
    .bind(&user_ids)
    .fetch_all(&db.pool)
    .await?;

    for row in rows {
        let user_id = row.user_id;
        let item = map_row(row);
        push_to_user(hub, &user_id, &item).await;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Public trigger functions
// ---------------------------------------------------------------------------

/// 白名单申请通知
pub async fn notify_whitelist_apply(
    db: &Database,
    hub: &NotificationHub,
    nickname: &str,
    steamid64: &str,
) -> anyhow::Result<()> {
    let message = format!("玩家 {}（{}）提交了白名单申请", nickname, steamid64);
    notify_all_admins(
        db,
        hub,
        None,
        "whitelist_apply",
        "新白名单申请",
        &message,
        Some("/whitelist?status=pending"),
    )
    .await
}

/// 手动封禁通知
pub async fn notify_ban_create(
    db: &Database,
    hub: &NotificationHub,
    operator_id: &Uuid,
    operator_name: &str,
    player: Option<&str>,
    steam_id: &str,
    reason: &str,
) -> anyhow::Result<()> {
    let player_display = player.unwrap_or(steam_id);
    let message = format!(
        "{} 封禁了玩家 {}（{}）：{}",
        operator_name, player_display, steam_id, reason
    );
    notify_all_admins(
        db,
        hub,
        Some(operator_id),
        "ban_create",
        "新封禁记录",
        &message,
        Some("/ban"),
    )
    .await
}

/// 插件封禁通知
pub async fn notify_plugin_ban(
    db: &Database,
    hub: &NotificationHub,
    server_name: &str,
    player: Option<&str>,
    steam_id: &str,
    reason: &str,
) -> anyhow::Result<()> {
    let player_display = player.unwrap_or(steam_id);
    let message = format!(
        "服务器 {} 提交了玩家 {}（{}）的封禁：{}",
        server_name, player_display, steam_id, reason
    );
    notify_all_admins(
        db,
        hub,
        None,
        "plugin_ban",
        "插件提交封禁",
        &message,
        Some("/ban"),
    )
    .await
}

// ---------------------------------------------------------------------------
// Cleanup task
// ---------------------------------------------------------------------------

pub fn start_cleanup_loop(db: Database, interval_secs: u64) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(interval_secs));
        loop {
            interval.tick().await;
            match sqlx::query(
                r#"DELETE FROM notifications WHERE read = true AND created_at < now() - interval '30 days'"#,
            )
            .execute(&db.pool)
            .await
            {
                Ok(result) => {
                    if result.rows_affected() > 0 {
                        tracing::info!(
                            count = result.rows_affected(),
                            "cleaned up old read notifications"
                        );
                    }
                }
                Err(e) => tracing::warn!(%e, "notification cleanup failed"),
            }
        }
    });
}

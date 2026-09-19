//! 授权事件同步服务（LumiAuth Control Plane）。
//!
//! 事件模型：
//! - `version` 全局递增（`auth_event_seq`），插件按 `last_applied_version` 续传；
//! - `server_id = NULL` 表示广播事件（白名单/封禁对所有服生效）；
//! - `server.config.update` 为单服事件，仅下发给对应服务器。
//!
//! 写入约定：业务表变更与 `auth_events` 插入必须在同一事务内完成
//! （Transactional Outbox），避免“DB 成功但事件丢失”。

use crate::db::Database;
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

/// 与插件约定的阈值（Phase2 定稿）：版本差 >500 或离线 >30min 走 Snapshot，其余增量。
pub const SNAPSHOT_VERSION_GAP: i64 = 500;
pub const SNAPSHOT_OFFLINE_SECS: i64 = 1800;
/// 事件 Long-Poll 最长挂起秒数（插件 timeout=25s，后端略小避免竞态）。
pub const EVENTS_POLL_HOLD_SECS: u64 = 20;
/// 事件保留：插件 30s 补偿 poll 足够覆盖，重放窗口保留 7 天便于排查。
pub const EVENTS_RETENTION_DAYS: i64 = 7;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct AuthEvent {
    pub event_id: Uuid,
    pub version: i64,
    pub server_id: Option<Uuid>,
    pub event_type: String,
    pub payload: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

/// 在业务事务内插入一条授权事件，返回分配到的 version。
pub async fn insert_event_tx(
    tx: &mut Transaction<'_, Postgres>,
    server_id: Option<Uuid>,
    event_type: &str,
    payload: serde_json::Value,
) -> anyhow::Result<i64> {
    let row: (i64,) = sqlx::query_as(
        r#"INSERT INTO auth_events (server_id, event_type, payload)
           VALUES ($1, $2, $3)
           RETURNING version"#,
    )
    .bind(server_id)
    .bind(event_type)
    .bind(&payload)
    .fetch_one(&mut **tx)
    .await?;
    Ok(row.0)
}

/// 非事务便捷入口（仅用于后台补偿/测试；业务写点必须用 insert_event_tx）。
pub async fn insert_event(
    db: &Database,
    server_id: Option<Uuid>,
    event_type: &str,
    payload: serde_json::Value,
) -> anyhow::Result<i64> {
    let mut tx = db.pool.begin().await?;
    let version = insert_event_tx(&mut tx, server_id, event_type, payload).await?;
    tx.commit().await?;
    Ok(version)
}

pub fn whitelist_add_payload(
    steamid64: &str,
    expires_at: Option<DateTime<Utc>>,
) -> serde_json::Value {
    serde_json::json!({
        "steamid64": steamid64,
        "expires_at": expires_at.map(|v| v.to_rfc3339()),
    })
}

pub fn ban_add_payload(
    ban_id: Uuid,
    steam_id: &str,
    ip_address: Option<&str>,
    reason: &str,
    expires_at: Option<DateTime<Utc>>,
) -> serde_json::Value {
    serde_json::json!({
        "ban_id": ban_id,
        "steam_id": steam_id,
        "ip_address": ip_address,
        "reason": reason,
        "expires_at": expires_at.map(|v| v.to_rfc3339()),
    })
}

pub fn ban_remove_payload(ban_id: Uuid, steam_id: &str) -> serde_json::Value {
    serde_json::json!({
        "ban_id": ban_id,
        "steam_id": steam_id,
    })
}

/// 当前全局最新 version（无事件时为 0）。
pub async fn latest_version(db: &Database) -> anyhow::Result<i64> {
    let row: (Option<i64>,) = sqlx::query_as("SELECT MAX(version) FROM auth_events")
        .fetch_one(&db.pool)
        .await?;
    Ok(row.0.unwrap_or(0))
}

/// 拉取 (after_version, +∞) 区间内对该服可见的事件（广播 + 本服），按 version 升序。
pub async fn events_after(
    db: &Database,
    server_id: Uuid,
    after_version: i64,
    limit: i64,
) -> anyhow::Result<Vec<AuthEvent>> {
    let rows = sqlx::query_as::<_, AuthEvent>(
        r#"SELECT event_id, version, server_id, event_type, payload, created_at
           FROM auth_events
           WHERE version > $1
             AND (server_id IS NULL OR server_id = $2)
           ORDER BY version ASC
           LIMIT $3"#,
    )
    .bind(after_version)
    .bind(server_id)
    .bind(limit.clamp(1, 500))
    .fetch_all(&db.pool)
    .await?;
    Ok(rows)
}

/// 插件 ACK：记录 last_acked_version + 连接状态（connected）。
pub async fn ack_version(
    db: &Database,
    server_id: Uuid,
    version: i64,
) -> anyhow::Result<(i64, i64)> {
    sqlx::query(
        r#"INSERT INTO auth_server_state (server_id, last_acked_version, connection_status, last_seen_at, updated_at)
           VALUES ($1, $2, 'connected', now(), now())
           ON CONFLICT (server_id) DO UPDATE SET
             last_acked_version = GREATEST(auth_server_state.last_acked_version, EXCLUDED.last_acked_version),
             connection_status = 'connected',
             last_seen_at = now(),
             updated_at = now()"#,
    )
    .bind(server_id)
    .bind(version)
    .execute(&db.pool)
    .await?;
    let latest = latest_version(db).await?;
    let acked = last_acked_version(db, server_id).await?;
    Ok((acked, latest))
}

pub async fn last_acked_version(db: &Database, server_id: Uuid) -> anyhow::Result<i64> {
    let row: (Option<i64>,) =
        sqlx::query_as("SELECT last_acked_version FROM auth_server_state WHERE server_id = $1")
            .bind(server_id)
            .fetch_optional(&db.pool)
            .await?
            .unwrap_or((None,));
    Ok(row.0.unwrap_or(0))
}

pub async fn mark_seen(db: &Database, server_id: Uuid, connected: bool) -> anyhow::Result<()> {
    sqlx::query(
        r#"INSERT INTO auth_server_state (server_id, connection_status, last_seen_at, updated_at)
           VALUES ($1, $2, now(), now())
           ON CONFLICT (server_id) DO UPDATE SET
             connection_status = EXCLUDED.connection_status,
             last_seen_at = now(),
             updated_at = now()"#,
    )
    .bind(server_id)
    .bind(if connected {
        "connected"
    } else {
        "disconnected"
    })
    .execute(&db.pool)
    .await?;
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
pub struct AuthServerSyncStatus {
    pub server_id: Uuid,
    pub connection_status: String,
    pub version: i64,
    pub latest_version: i64,
    pub pending_events: i64,
    pub last_sync_at: Option<DateTime<Utc>>,
}

/// 后台 UI 用：每服同步状态（Auth 4 列）。
pub async fn sync_status_for_servers(
    db: &Database,
    server_ids: &[Uuid],
) -> anyhow::Result<Vec<AuthServerSyncStatus>> {
    if server_ids.is_empty() {
        return Ok(Vec::new());
    }
    let latest = latest_version(db).await?;
    #[derive(sqlx::FromRow)]
    struct Row {
        server_id: Uuid,
        connection_status: String,
        last_acked_version: i64,
        last_seen_at: Option<DateTime<Utc>>,
    }
    let rows: Vec<Row> = sqlx::query_as(
        r#"SELECT server_id, connection_status, last_acked_version, last_seen_at
           FROM auth_server_state WHERE server_id = ANY($1)"#,
    )
    .bind(server_ids)
    .fetch_all(&db.pool)
    .await?;
    let mut out = Vec::with_capacity(server_ids.len());
    for id in server_ids {
        match rows.iter().find(|r| &r.server_id == id) {
            Some(r) => out.push(AuthServerSyncStatus {
                server_id: *id,
                connection_status: r.connection_status.clone(),
                version: r.last_acked_version,
                latest_version: latest,
                pending_events: (latest - r.last_acked_version).max(0),
                last_sync_at: r.last_seen_at,
            }),
            None => out.push(AuthServerSyncStatus {
                server_id: *id,
                connection_status: "disconnected".to_string(),
                version: 0,
                latest_version: latest,
                pending_events: latest.max(0),
                last_sync_at: None,
            }),
        }
    }
    Ok(out)
}

/// 是否需要 Snapshot（版本差 >500；离线时长由调用方结合 last_seen_at 判断）。
pub fn needs_snapshot(latest: i64, after_version: i64) -> bool {
    latest - after_version > SNAPSHOT_VERSION_GAP
}

/// 定期清理 7 天前的已投递事件（保留排查窗口）。
pub async fn cleanup_old_events(db: &Database) -> anyhow::Result<u64> {
    let result = sqlx::query(
        r#"DELETE FROM auth_events WHERE created_at < now() - make_interval(days => $1)"#,
    )
    .bind(EVENTS_RETENTION_DAYS)
    .execute(&db.pool)
    .await?;
    Ok(result.rows_affected())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_gap_threshold() {
        assert!(needs_snapshot(601, 100));
        assert!(!needs_snapshot(600, 100));
        assert!(!needs_snapshot(10, 10));
    }
}

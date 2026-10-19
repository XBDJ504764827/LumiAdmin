//! 授权事件同步服务（LumiAuth Control Plane）。
//!
//! 事件模型（按游戏节点同步）：
//! - `version` 全局递增（`auth_event_seq`），节点按 `last_applied_version` 续传；
//! - `node_id = NULL` 表示广播事件（白名单/封禁对所有节点生效）；
//! - `node_id = 某节点` 表示仅该节点可见的事件（服务器/社区配置变更），
//!   同一台机器上的所有服共享同一条事件，避免 N 台服重复下发与版本空洞。
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
/// 离线超过该秒数（30min）的服务器建议直接走 Snapshot 全量恢复。
/// 插件侧按“版本差 >500 或本地库缺失”触发；该常量供运维/文档引用。
pub const SNAPSHOT_OFFLINE_SECS: i64 = 1800;
/// 事件 Long-Poll 最长挂起秒数（插件 timeout=25s，后端略小避免竞态）。
pub const EVENTS_POLL_HOLD_SECS: u64 = 20;
/// 事件保留天数：插件 30s 补偿 poll 足够覆盖，重放窗口保留 7 天便于排查。
/// 由 [`cleanup_old_events`] 按此窗口清理，清理任务见 [`register_cleanup_task`]。
pub const EVENTS_RETENTION_DAYS: i64 = 7;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct AuthEvent {
    pub event_id: Uuid,
    pub version: i64,
    pub server_id: Option<Uuid>,
    pub node_id: Option<Uuid>,
    pub event_type: String,
    pub payload: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

/// 在业务事务内插入一条授权事件，返回分配到的 version。
pub async fn insert_event_tx(
    tx: &mut Transaction<'_, Postgres>,
    server_id: Option<Uuid>,
    node_id: Option<Uuid>,
    event_type: &str,
    payload: serde_json::Value,
) -> anyhow::Result<i64> {
    let row: (i64,) = sqlx::query_as(
        r#"INSERT INTO auth_events (server_id, node_id, event_type, payload)
           VALUES ($1, $2, $3, $4)
           RETURNING version"#,
    )
    .bind(server_id)
    .bind(node_id)
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
    node_id: Option<Uuid>,
    event_type: &str,
    payload: serde_json::Value,
) -> anyhow::Result<i64> {
    let mut tx = db.pool.begin().await?;
    let version = insert_event_tx(&mut tx, server_id, node_id, event_type, payload).await?;
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
        "expires_at_unix": expires_at.map(|v| v.timestamp()).unwrap_or(0),
    })
}

pub fn ban_remove_payload(ban_id: Uuid, steam_id: &str, ip_address: Option<&str>) -> serde_json::Value {
    serde_json::json!({
        "ban_id": ban_id,
        "steam_id": steam_id,
        "ip_address": ip_address,
    })
}

/// 节点最新可见 version：广播事件 ∪ 该节点事件（无事件时为 0）。
///
/// 关键：不能用全局 MAX(version)，否则其它节点的事件会把本节点版本顶高，
/// 造成「永远落后」与频繁误判版本跳跃、重复拉全量快照。
pub async fn latest_version_for_node(db: &Database, node_id: Uuid) -> anyhow::Result<i64> {
    let row: (Option<i64>,) = sqlx::query_as(
        r#"SELECT MAX(version) FROM auth_events
           WHERE node_id IS NULL OR node_id = $1"#,
    )
    .bind(node_id)
    .fetch_one(&db.pool)
    .await?;
    Ok(row.0.unwrap_or(0))
}

/// 拉取 (after_version, +∞) 区间内对该节点可见的事件（广播 + 本节点），按 version 升序。
pub async fn events_after(
    db: &Database,
    node_id: Uuid,
    after_version: i64,
    limit: i64,
) -> anyhow::Result<Vec<AuthEvent>> {
    let rows = sqlx::query_as::<_, AuthEvent>(
        r#"SELECT event_id, version, server_id, node_id, event_type, payload, created_at
           FROM auth_events
           WHERE version > $1
             AND (node_id IS NULL OR node_id = $2)
           ORDER BY version ASC
           LIMIT $3"#,
    )
    .bind(after_version)
    .bind(node_id)
    .bind(limit.clamp(1, 500))
    .fetch_all(&db.pool)
    .await?;
    Ok(rows)
}

/// 节点 ACK：记录 last_acked_version + 连接状态（connected）。
pub async fn ack_version(db: &Database, node_id: Uuid, version: i64) -> anyhow::Result<(i64, i64)> {
    sqlx::query(
        r#"INSERT INTO auth_node_state (node_id, last_acked_version, connection_status, last_seen_at, updated_at)
           VALUES ($1, $2, 'connected', now(), now())
           ON CONFLICT (node_id) DO UPDATE SET
             last_acked_version = GREATEST(auth_node_state.last_acked_version, EXCLUDED.last_acked_version),
             connection_status = 'connected',
             last_seen_at = now(),
             updated_at = now()"#,
    )
    .bind(node_id)
    .bind(version)
    .execute(&db.pool)
    .await?;
    let latest = latest_version_for_node(db, node_id).await?;
    let acked = last_acked_version(db, node_id).await?;
    Ok((acked, latest))
}

pub async fn last_seen_at(db: &Database, node_id: Uuid) -> anyhow::Result<Option<DateTime<Utc>>> {
    let row: Option<(Option<DateTime<Utc>>,)> =
        sqlx::query_as("SELECT last_seen_at FROM auth_node_state WHERE node_id = $1")
            .bind(node_id)
            .fetch_optional(&db.pool)
            .await?;
    Ok(row.and_then(|r| r.0))
}

/// 离线过久（> [`SNAPSHOT_OFFLINE_SECS`]）且落后时建议走 Snapshot 全量恢复。
///
/// 首访（无 `last_seen_at`）或已追平（`latest <= after`）时返回 false，
/// 避免无新事件时做无谓的全量替换。
pub async fn is_offline_snapshot_required(
    db: &Database,
    node_id: Uuid,
    after_version: i64,
    latest: i64,
) -> anyhow::Result<bool> {
    if latest <= after_version {
        return Ok(false);
    }
    let seen = last_seen_at(db, node_id).await?;
    let Some(seen) = seen else {
        return Ok(false);
    };
    Ok((Utc::now() - seen).num_seconds() > SNAPSHOT_OFFLINE_SECS)
}

pub async fn last_acked_version(db: &Database, node_id: Uuid) -> anyhow::Result<i64> {
    let row: (Option<i64>,) =
        sqlx::query_as("SELECT last_acked_version FROM auth_node_state WHERE node_id = $1")
            .bind(node_id)
            .fetch_optional(&db.pool)
            .await?
            .unwrap_or((None,));
    Ok(row.0.unwrap_or(0))
}

pub async fn mark_seen(db: &Database, node_id: Uuid, connected: bool) -> anyhow::Result<()> {
    sqlx::query(
        r#"INSERT INTO auth_node_state (node_id, connection_status, last_seen_at, updated_at)
           VALUES ($1, $2, now(), now())
           ON CONFLICT (node_id) DO UPDATE SET
             connection_status = EXCLUDED.connection_status,
             last_seen_at = now(),
             updated_at = now()"#,
    )
    .bind(node_id)
    .bind(if connected {
        "connected"
    } else {
        "disconnected"
    })
    .execute(&db.pool)
    .await?;
    Ok(())
}

/// 标记一次快照同步时间（UI 的 LastSync 来源之一）。
pub async fn mark_snapshot(db: &Database, node_id: Uuid) -> anyhow::Result<()> {
    sqlx::query(
        r#"INSERT INTO auth_node_state (node_id, connection_status, last_seen_at, last_snapshot_at, updated_at)
           VALUES ($1, 'connected', now(), now(), now())
           ON CONFLICT (node_id) DO UPDATE SET
             connection_status = 'connected', last_seen_at = now(),
             last_snapshot_at = now(), updated_at = now()"#,
    )
    .bind(node_id)
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

/// 后台 UI 用：每服同步状态（按服务器所属节点聚合，4 列）。
///
/// 同一节点上的所有服共享同一份同步状态，因此同一台机器上的服会显示一致的
/// 「同步进度」，不再出现「部分服落后、部分服追平」的假象。
pub async fn sync_status_for_servers(
    db: &Database,
    server_ids: &[Uuid],
) -> anyhow::Result<Vec<AuthServerSyncStatus>> {
    if server_ids.is_empty() {
        return Ok(Vec::new());
    }
    #[derive(sqlx::FromRow)]
    struct NodeRow {
        server_id: Uuid,
        node_id: Option<Uuid>,
        connection_status: Option<String>,
        last_acked_version: Option<i64>,
        last_seen_at: Option<DateTime<Utc>>,
    }
    let rows: Vec<NodeRow> = sqlx::query_as(
        r#"SELECT s.id AS server_id, s.node_id,
                  ns.connection_status, ns.last_acked_version, ns.last_seen_at
           FROM servers s
           LEFT JOIN auth_node_state ns ON ns.node_id = s.node_id
           WHERE s.id = ANY($1)"#,
    )
    .bind(server_ids)
    .fetch_all(&db.pool)
    .await?;

    // 每个节点只算一次最新版本，避免 N 次查询。
    let mut node_latest: std::collections::HashMap<Uuid, i64> = std::collections::HashMap::new();
    for row in &rows {
        if let Some(node_id) = row.node_id {
            if let std::collections::hash_map::Entry::Vacant(entry) = node_latest.entry(node_id) {
                entry.insert(latest_version_for_node(db, node_id).await?);
            }
        }
    }

    let mut out = Vec::with_capacity(server_ids.len());
    for id in server_ids {
        let row = rows.iter().find(|r| &r.server_id == id);
        match row.and_then(|r| r.node_id.map(|node_id| (r, node_id))) {
            Some((r, node_id)) => {
                let latest = node_latest.get(&node_id).copied().unwrap_or(0);
                let acked = r.last_acked_version.unwrap_or(0);
                out.push(AuthServerSyncStatus {
                    server_id: *id,
                    connection_status: r
                        .connection_status
                        .clone()
                        .unwrap_or_else(|| "disconnected".to_string()),
                    version: acked,
                    latest_version: latest,
                    pending_events: (latest - acked).max(0),
                    last_sync_at: r.last_seen_at,
                });
            }
            None => {
                out.push(AuthServerSyncStatus {
                    server_id: *id,
                    connection_status: "disconnected".to_string(),
                    version: 0,
                    latest_version: 0,
                    pending_events: 0,
                    last_sync_at: None,
                });
            }
        }
    }
    Ok(out)
}

/// 是否需要 Snapshot（版本差 >500；离线时长由调用方结合 last_seen_at 判断）。
pub fn needs_snapshot(latest: i64, after_version: i64) -> bool {
    latest - after_version > SNAPSHOT_VERSION_GAP
}

/// 定期清理保留窗口之外的已投递事件（保留排查窗口）。
pub async fn cleanup_old_events(db: &Database) -> anyhow::Result<u64> {
    let result = sqlx::query(
        r#"DELETE FROM auth_events WHERE created_at < now() - make_interval(days => $1::int)"#,
    )
    .bind(EVENTS_RETENTION_DAYS)
    .execute(&db.pool)
    .await?;
    Ok(result.rows_affected())
}

/// 注册事件保留清理后台任务（每天一次），避免 `auth_events` 无限增长。
///
/// 与 `log_retention_service` 的清理任务保持一致：失败仅告警，不影响主流程。
pub fn register_cleanup_task(db: Database) {
    crate::services::observability_service::register_task(
        "auth_events_cleanup",
        "授权事件保留清理",
        "清理",
        Some(86400),
        true,
    );
    crate::services::task_runtime::spawn_persistent("auth_events_cleanup", move || {
        let db = db.clone();
        async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(86400));
            loop {
                interval.tick().await;
                match crate::services::observability_service::observe_task(
                    "auth_events_cleanup",
                    cleanup_old_events(&db),
                    |count| format!("清理过期授权事件 {count} 条"),
                )
                .await
                {
                    Ok(_) => {}
                    Err(e) => tracing::warn!(%e, "授权事件保留清理失败"),
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::Config, db::Database};

    #[test]
    fn snapshot_gap_threshold() {
        assert!(needs_snapshot(601, 100));
        assert!(!needs_snapshot(600, 100));
        assert!(!needs_snapshot(10, 10));
    }

    async fn with_test_db(test: impl AsyncFnOnce(Database) -> anyhow::Result<()>) {
        let config = Config::from_env();
        let base_url = config.database_url.clone();
        let schema = format!("test_{}", Uuid::new_v4().simple());
        let scoped_url = crate::test_util::schema_url(&base_url, &schema);
        crate::test_util::create_schema(&base_url, &schema).await;

        let result = async {
            let db = Database::connect_for_test(&scoped_url).await?;
            db.migrate().await?;
            test(db).await
        }
        .await;

        crate::test_util::drop_schema(&base_url, &schema).await;
        result.unwrap();
    }

    async fn insert_server(db: &Database, community_id: Uuid, name: &str, ip: &str, port: i32) -> Uuid {
        let id = Uuid::new_v4();
        sqlx::query(
            r#"INSERT INTO servers (id, community_id, name, ip, port, rcon_password, report_token)
               VALUES ($1, $2, $3, $4, $5, 'test', $6)"#,
        )
        .bind(id)
        .bind(community_id)
        .bind(name)
        .bind(ip)
        .bind(port)
        .bind(Uuid::new_v4().to_string())
        .execute(&db.pool)
        .await
        .unwrap();
        id
    }

    /// 核心回归：一个节点的事件不能让另一个节点「永远落后」。
    #[tokio::test]
    async fn node_scoped_latest_ignores_other_nodes_events() {
        with_test_db(async |db| {
            let community_id = Uuid::new_v4();
            sqlx::query("INSERT INTO communities (id, name) VALUES ($1, 'c')")
                .bind(community_id)
                .execute(&db.pool)
                .await?;
            // 两个不同 IP = 两个不同游戏节点
            let server_a = insert_server(&db, community_id, "a", "10.0.0.1", 27015).await;
            let server_b = insert_server(&db, community_id, "b", "10.0.0.2", 27016).await;
            let node_a = crate::services::game_node_service::resolve_or_assign_node(&db, server_a)
                .await?
                .unwrap();
            let node_b = crate::services::game_node_service::resolve_or_assign_node(&db, server_b)
                .await?
                .unwrap();
            assert_ne!(node_a, node_b);

            // 广播事件：两个节点都可见
            let broadcast = insert_event(&db, None, None, "whitelist.add", serde_json::json!({}))
                .await?;
            // 仅节点 A 的事件
            let only_a = insert_event(&db, None, Some(node_a), "server.config.update", serde_json::json!({}))
                .await?;

            assert_eq!(latest_version_for_node(&db, node_a).await?, only_a);
            assert_eq!(latest_version_for_node(&db, node_b).await?, broadcast);
            assert!(latest_version_for_node(&db, node_b).await? < only_a);

            let events_b = events_after(&db, node_b, 0, 100).await?;
            assert_eq!(events_b.len(), 1);
            assert_eq!(events_b[0].version, broadcast);
            assert!(events_b.iter().all(|event| event.node_id.is_none()));

            // B 追平后 pending 必须为 0，而不是被 A 的事件顶出「落后」
            ack_version(&db, node_b, broadcast).await?;
            let status = sync_status_for_servers(&db, &[server_b]).await?;
            assert_eq!(status[0].pending_events, 0);
            assert_eq!(status[0].latest_version, broadcast);
            Ok(())
        })
        .await;
    }
}

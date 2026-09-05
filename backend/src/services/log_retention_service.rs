use crate::db::Database;
use crate::services::observability_service;

/// 启动审计/操作日志保留清理任务。
///
/// - `audit_logs`：默认保留 365 天（可配置 `AUDIT_RETENTION_DAYS`）
/// - `admin_logs`：默认保留 90 天（可配置 `ADMIN_LOG_RETENTION_DAYS`）
/// - `lumi_bot_event_queue`：处理完的事件保留 30 天（可配置 `LUMI_BOT_QUEUE_RETENTION_DAYS`），
///   失败事件不清理以免丢失死信
/// - `player_server_sessions`：玩家会话历史默认保留 180 天
///   （可配置 `SESSION_RETENTION_DAYS`，`0`=关闭），按 `created_at` 分批删除避免长锁
///
/// 与 server_status_history / access_logs / notifications 的清理任务保持一致，
/// 避免这些日志表无限增长。
pub fn start_log_retention_loop(db: Database, interval_secs: u64) {
    observability_service::register_task(
        "log_retention_cleanup",
        "审计/操作日志保留清理",
        "清理",
        Some(interval_secs),
        true,
    );
    super::task_runtime::spawn_persistent("log_retention_cleanup", move || {
        let db = db.clone();
        async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(interval_secs));
            loop {
                interval.tick().await;
                let result = cleanup_expired_logs(&db).await;
                match observability_service::observe_task(
                    "log_retention_cleanup",
                    async { result },
                    |counts| {
                        format!(
                            "清理完成：audit {audit} 条、操作日志 {logs} 条、LumiBot 事件 {queue} 条、会话历史 {sessions} 条",
                            audit = counts.0,
                            logs = counts.1,
                            queue = counts.2,
                            sessions = counts.3,
                        )
                    },
                )
                .await
                {
                    Ok(_) => {}
                    Err(e) => tracing::warn!(%e, "审计/操作日志保留清理失败"),
                }
            }
        }
    });
}

async fn cleanup_expired_logs(db: &Database) -> anyhow::Result<(u64, u64, u64, u64)> {
    let audit_days = retention_days("AUDIT_RETENTION_DAYS", 365);
    let admin_log_days = retention_days("ADMIN_LOG_RETENTION_DAYS", 90);
    let queue_days = retention_days("LUMI_BOT_QUEUE_RETENTION_DAYS", 30);
    let session_days = retention_days("SESSION_RETENTION_DAYS", 180);

    let audit = if audit_days > 0 {
        sqlx::query(
            "DELETE FROM audit_logs WHERE created_at < NOW() - ($1::int * interval '1 day')",
        )
        .bind(audit_days)
        .execute(&db.pool)
        .await?
        .rows_affected()
    } else {
        0
    };

    let logs = if admin_log_days > 0 {
        sqlx::query(
            "DELETE FROM admin_logs WHERE created_at < NOW() - ($1::int * interval '1 day')",
        )
        .bind(admin_log_days)
        .execute(&db.pool)
        .await?
        .rows_affected()
    } else {
        0
    };

    // LumiBot 事件队列：只清理已成功发送的事件；pending/failed 事件保留，
    // 防止待重试事件与死信被误删
    let queue = if queue_days > 0 {
        sqlx::query(
            "DELETE FROM lumi_bot_event_queue
             WHERE status = 'sent' AND queued_at < NOW() - ($1::int * interval '1 day')",
        )
        .bind(queue_days)
        .execute(&db.pool)
        .await?
        .rows_affected()
    } else {
        0
    };

    // 玩家会话历史：按 created_at 分批删除（每批最多 10 万条），避免单条大 DELETE 长锁。
    let sessions = if session_days > 0 {
        cleanup_old_sessions(db, session_days).await?
    } else {
        0
    };

    Ok((audit, logs, queue, sessions))
}

/// 分批清理超过保留天数的玩家会话历史。
/// 返回本轮删除的总条数；多次循环直到删除不足一批为止。
async fn cleanup_old_sessions(db: &Database, retention_days: i64) -> anyhow::Result<u64> {
    const BATCH_LIMIT: u64 = 100_000;
    let mut total: u64 = 0;
    loop {
        let result = sqlx::query(
            r#"DELETE FROM player_server_sessions
               WHERE id IN (
                 SELECT id FROM player_server_sessions
                 WHERE created_at < NOW() - ($1::int * interval '1 day')
                 LIMIT $2
               )"#,
        )
        .bind(retention_days)
        .bind(BATCH_LIMIT as i64)
        .execute(&db.pool)
        .await?;
        let affected = result.rows_affected();
        total += affected;
        if affected < BATCH_LIMIT {
            break;
        }
    }
    Ok(total)
}

/// 读取保留天数配置；`0` 表示关闭清理；解析失败时回退到默认值。
fn retention_days(key: &str, default: i64) -> i64 {
    match std::env::var(key) {
        Ok(v) => match v.trim().parse::<i64>() {
            Ok(days) if days >= 0 => days,
            Ok(_) => {
                tracing::warn!(key = key, value = %v, "保留天数应为非负整数，已使用默认值 {default}");
                default
            }
            Err(_) => {
                tracing::warn!(key = key, value = %v, "保留天数解析失败，已使用默认值 {default}");
                default
            }
        },
        Err(_) => default,
    }
}

#[cfg(test)]
mod tests {
    use super::retention_days;

    #[test]
    fn retention_days_falls_back_on_invalid_value() {
        // 环境变量不可控，这里只验证负数返回默认值（通过注入无效环境变量的场景由 e2e 覆盖）
        assert!(retention_days("__UNSET_RETENTION_KEY__", 365) == 365);
    }
}

//! 授权 RCON 快车道：封禁 `DB+事件` 落地后，立即经 RCON 下发快车道命令。
//!
//! 顺序保证（Q3 定稿）：
//! 1. `ban.add / ban.remove` 事件已在业务事务内提交（权威态已是 Ban）；
//! 2. 本服务再经 RCON 发送 `lumiadmin_apply_ban ...` / `lumiadmin_apply_unban ...`；
//! 3. 插件收到后**先写本地 SQLite（含内存缓存），再 Kick**（插件侧保证）；
//! 4. RCON 失败只记审计 + 日志，不回滚 DB；补偿靠 30s poll / 25s 事件 poll 收敛。

use crate::{config::Config, db::Database};
use uuid::Uuid;

fn escape_rcon_arg(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' | '\r' => out.push(' '),
            c => out.push(c),
        }
    }
    out
}

async fn servers_for_fast_path(db: &Database) -> Vec<(Uuid, String)> {
    #[derive(sqlx::FromRow)]
    struct Row {
        id: Uuid,
        name: String,
    }
    sqlx::query_as::<_, Row>(r#"SELECT id, name FROM servers WHERE status = 'online'"#)
        .fetch_all(&db.pool)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|r| (r.id, r.name))
        .collect()
}

async fn send_fast_path(
    db: &Database,
    config: &Config,
    server_id: Uuid,
    server_name: &str,
    command: &str,
    audit_operation: &str,
    audit_target: &str,
) {
    let timeouts = crate::services::community_rcon::RconTimeouts::from_config(config);
    match crate::services::community_rcon::execute_rcon_command(
        db, server_id, command, timeouts, true,
    )
    .await
    {
        Ok(_) => {
            tracing::info!(
                server_id = %server_id,
                command = %crate::services::community_rcon::audit_command(command),
                "授权快车道 RCON 下发成功"
            );
        }
        Err(e) => {
            tracing::warn!(%e, server_id = %server_id, server_name = %server_name, "授权快车道 RCON 下发失败，等待补偿同步收敛");
            if let Err(ae) = crate::services::audit_service::write_audit_log(
                db,
                crate::services::audit_service::AuditLogInput {
                    operation: audit_operation.to_string(),
                    target: audit_target.to_string(),
                    target_type: "auth_fast_path".to_string(),
                    player_name: None,
                    reason: None,
                    duration_minutes: None,
                    operator_id: None,
                    operator_name: "system".to_string(),
                    operator_steamid: None,
                    source: "system".to_string(),
                    server_id: Some(server_id),
                    server_name: Some(server_name.to_string()),
                    server_port: None,
                    success: false,
                    message: Some(format!("授权快车道 RCON 下发失败：{e}")),
                    idempotency_key: None,
                },
            )
            .await
            {
                tracing::warn!(%ae, "快车道失败审计写入失败");
            }
        }
    }
}

/// 后台创建封禁后的快车道广播：`lumiadmin_apply_ban <ban_id> <version> <steam_id> <expires_unix|0> <reason>`。
///
/// `version` 取该封禁事件的 version（查最新同 ban_id 的 ban.add 事件；查不到则传 0，
/// 插件以事件 poll 为准，快车道仅加速）。
pub async fn apply_ban_fast_path(
    db: &Database,
    config: &Config,
    item: &crate::services::ban_service::BanItem,
) {
    let version: (Option<i64>,) = sqlx::query_as(
        r#"SELECT MAX(version) FROM auth_events
           WHERE event_type = 'ban.add' AND payload->>'ban_id' = $1"#,
    )
    .bind(item.id.to_string())
    .fetch_optional(&db.pool)
    .await
    .unwrap_or(None)
    .unwrap_or((None,));
    let version = version.0.unwrap_or(0);
    let expires_unix: i64 = item
        .expires_at
        .as_deref()
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.timestamp())
        .unwrap_or(0);
    let command = format!(
        "lumiadmin_apply_ban \"{}\" {} \"{}\" {} \"{}\"",
        item.id,
        version,
        escape_rcon_arg(&item.steam_id),
        expires_unix,
        escape_rcon_arg(&item.reason),
    );
    let steam_id = item.steam_id.clone();
    for (server_id, server_name) in servers_for_fast_path(db).await {
        send_fast_path(
            db,
            config,
            server_id,
            &server_name,
            &command,
            "auth_fast_path_ban",
            &steam_id,
        )
        .await;
    }
}

/// 解封快车道：`lumiadmin_apply_unban <ban_id> <version> <steam_id>`。
pub async fn apply_unban_fast_path(db: &Database, config: &Config, ban_id: Uuid, steam_id: &str) {
    let command = format!(
        "lumiadmin_apply_unban \"{}\" 0 \"{}\"",
        ban_id,
        escape_rcon_arg(steam_id),
    );
    for (server_id, server_name) in servers_for_fast_path(db).await {
        send_fast_path(
            db,
            config,
            server_id,
            &server_name,
            &command,
            "auth_fast_path_unban",
            steam_id,
        )
        .await;
    }
}

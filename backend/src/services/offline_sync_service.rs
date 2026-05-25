use crate::{
    db::Database,
    services::{audit_service, plugin_ban_service},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Deserialize)]
pub struct OfflineOperationInput {
    pub operation: String,
    pub target: String,
    pub target_type: String,
    pub player_name: Option<String>,
    pub reason: Option<String>,
    pub duration_minutes: Option<i32>,
    pub operator_name: String,
    pub operator_steamid: Option<String>,
    pub created_at_unix: i64,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OfflineSyncInput {
    pub report_token: String,
    pub port: i32,
    pub operations: Vec<OfflineOperationInput>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OfflineSyncResult {
    pub received: i32,
    pub applied: i32,
    pub skipped: i32,
    pub errors: Vec<String>,
}

/// 接收并处理离线操作
pub async fn sync_offline_operations(
    db: &Database,
    input: OfflineSyncInput,
) -> anyhow::Result<OfflineSyncResult> {
    // 验证服务器身份
    let server = plugin_ban_service::ServerAuth::authenticate(db, input.port, &input.report_token).await?;

    let mut result = OfflineSyncResult {
        received: 0,
        applied: 0,
        skipped: 0,
        errors: Vec::new(),
    };

    for op in input.operations {
        result.received += 1;

        // 检查幂等键是否已处理
        let exists: (bool,) = sqlx::query_as(
            r#"SELECT COALESCE(
                (SELECT true FROM offline_operations WHERE idempotency_key = $1),
                (SELECT true FROM audit_logs WHERE idempotency_key = $1),
                false
               )"#,
        )
        .bind(&op.idempotency_key)
        .fetch_one(&db.pool)
        .await?;

        if exists.0 {
            result.skipped += 1;
            continue;
        }

        // 写入离线操作记录
        let created_at = DateTime::from_timestamp(op.created_at_unix, 0)
            .unwrap_or_else(Utc::now);

        let offline_id = Uuid::new_v4();
        sqlx::query(
            r#"INSERT INTO offline_operations (
                id, operation, target, target_type, player_name, reason, duration_minutes,
                operator_name, operator_steamid, server_id, server_port, created_at,
                synced_at, idempotency_key, applied
               )
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, now(), $13, false)"#,
        )
        .bind(offline_id)
        .bind(&op.operation)
        .bind(&op.target)
        .bind(&op.target_type)
        .bind(&op.player_name)
        .bind(&op.reason)
        .bind(op.duration_minutes)
        .bind(&op.operator_name)
        .bind(&op.operator_steamid)
        .bind(server.id)
        .bind(server.port)
        .bind(created_at)
        .bind(&op.idempotency_key)
        .execute(&db.pool)
        .await?;

        // 尝试应用操作
        match apply_offline_operation(db, &op, &server).await {
            Ok(_) => {
                sqlx::query(
                    r#"UPDATE offline_operations SET applied = true WHERE id = $1"#,
                )
                .bind(offline_id)
                .execute(&db.pool)
                .await?;
                result.applied += 1;
            }
            Err(error) => {
                let error_msg = error.to_string();
                sqlx::query(
                    r#"UPDATE offline_operations SET apply_error = $2 WHERE id = $1"#,
                )
                .bind(offline_id)
                .bind(&error_msg)
                .execute(&db.pool)
                .await?;
                result.errors.push(format!("{}: {}", op.idempotency_key, error_msg));
            }
        }
    }

    Ok(result)
}

async fn apply_offline_operation(
    db: &Database,
    op: &OfflineOperationInput,
    server: &plugin_ban_service::ServerAuth,
) -> anyhow::Result<()> {
    match op.operation.as_str() {
        "ban" => apply_offline_ban(db, op, server).await,
        "unban" => apply_offline_unban(db, op, server).await,
        "whitelist_add" => apply_offline_whitelist_add(db, op, server).await,
        "whitelist_remove" => apply_offline_whitelist_remove(db, op, server).await,
        _ => anyhow::bail!("未知操作类型: {}", op.operation),
    }
}

async fn apply_offline_ban(
    db: &Database,
    op: &OfflineOperationInput,
    server: &plugin_ban_service::ServerAuth,
) -> anyhow::Result<()> {
    let duration = op.duration_minutes.unwrap_or(0);
    let reason = op.reason.as_deref().unwrap_or("未填写");
    let ban_type = op.target_type.as_str();
    let normalized_target = if ban_type == "steam" {
        plugin_ban_service::normalize_steam_id(&op.target)
    } else {
        op.target.clone()
    };

    // 检查是否已存在有效封禁
    let existing: (i64,) = sqlx::query_as(
        r#"SELECT COUNT(*) FROM ban_records
           WHERE status = 'active'
             AND (expires_at IS NULL OR expires_at > now())
             AND (($1::TEXT IS NOT NULL AND steam_id = $1) OR ($2::TEXT IS NOT NULL AND ip_address = $2))"#,
    )
    .bind(if ban_type == "steam" { Some(&normalized_target) } else { None })
    .bind(if ban_type == "ip" { Some(&normalized_target) } else { None })
    .fetch_one(&db.pool)
    .await?;

    if existing.0 > 0 {
        // 写入审计日志（跳过）
        audit_service::write_audit_log(db, audit_service::AuditLogInput {
            operation: "ban".to_string(),
            target: op.target.clone(),
            target_type: op.target_type.clone(),
            player_name: op.player_name.clone(),
            reason: op.reason.clone(),
            duration_minutes: op.duration_minutes,
            operator_name: op.operator_name.clone(),
            operator_steamid: op.operator_steamid.clone(),
            source: "offline_sync".to_string(),
            server_id: Some(server.id),
            server_name: Some(server.name.clone()),
            server_port: Some(server.port),
            success: false,
            message: Some("目标已有有效封禁，跳过".to_string()),
            idempotency_key: Some(op.idempotency_key.clone()),
        }).await?;
        return Ok(());
    }

    // 创建封禁记录
    let expires_at = if duration == 0 {
        None
    } else {
        Some(Utc::now() + chrono::Duration::minutes(i64::from(duration)))
    };

    let ban_id: (Uuid,) = sqlx::query_as(
        r#"INSERT INTO ban_records (
               id, player, steam_id, ip_address, server_name, ban_type,
               duration_minutes, expires_at, reason, status, operator_name, source,
               server_id, server_port
           )
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, 'active', $10, 'offline_sync', $11, $12)
           RETURNING id"#,
    )
    .bind(Uuid::new_v4())
    .bind(&op.player_name)
    .bind(if ban_type == "steam" { &normalized_target } else { "" })
    .bind(if ban_type == "ip" { Some(&normalized_target) } else { None })
    .bind(&server.name)
    .bind(ban_type)
    .bind(duration)
    .bind(expires_at)
    .bind(reason)
    .bind(&op.operator_name)
    .bind(server.id)
    .bind(server.port)
    .fetch_one(&db.pool)
    .await?;

    // 写入审计日志
    audit_service::write_audit_log(db, audit_service::AuditLogInput {
        operation: "ban".to_string(),
        target: op.target.clone(),
        target_type: op.target_type.clone(),
        player_name: op.player_name.clone(),
        reason: op.reason.clone(),
        duration_minutes: op.duration_minutes,
        operator_name: op.operator_name.clone(),
        operator_steamid: op.operator_steamid.clone(),
        source: "offline_sync".to_string(),
        server_id: Some(server.id),
        server_name: Some(server.name.clone()),
        server_port: Some(server.port),
        success: true,
        message: Some(format!("离线封禁已同步，ID: {}", ban_id.0)),
        idempotency_key: Some(op.idempotency_key.clone()),
    }).await?;

    Ok(())
}

async fn apply_offline_unban(
    db: &Database,
    op: &OfflineOperationInput,
    server: &plugin_ban_service::ServerAuth,
) -> anyhow::Result<()> {
    let reason = op.reason.as_deref().unwrap_or("离线解封");

    // 查找有效封禁
    let ban_id: Option<Uuid> = sqlx::query_scalar(
        r#"SELECT id FROM ban_records
           WHERE status = 'active'
             AND (expires_at IS NULL OR expires_at > now())
             AND (steam_id = $1 OR ip_address = $1)
           ORDER BY created_at DESC
           LIMIT 1"#,
    )
    .bind(&op.target)
    .fetch_optional(&db.pool)
    .await?;

    let Some(id) = ban_id else {
        // 写入审计日志（跳过）
        audit_service::write_audit_log(db, audit_service::AuditLogInput {
            operation: "unban".to_string(),
            target: op.target.clone(),
            target_type: op.target_type.clone(),
            player_name: op.player_name.clone(),
            reason: op.reason.clone(),
            duration_minutes: None,
            operator_name: op.operator_name.clone(),
            operator_steamid: op.operator_steamid.clone(),
            source: "offline_sync".to_string(),
            server_id: Some(server.id),
            server_name: Some(server.name.clone()),
            server_port: Some(server.port),
            success: false,
            message: Some("未找到有效封禁，跳过".to_string()),
            idempotency_key: Some(op.idempotency_key.clone()),
        }).await?;
        return Ok(());
    };

    // 执行解封
    sqlx::query(
        r#"UPDATE ban_records
           SET status = 'inactive', removed_reason = $2, removed_by = $3, removed_at = now()
           WHERE id = $1"#,
    )
    .bind(id)
    .bind(reason)
    .bind(&op.operator_name)
    .execute(&db.pool)
    .await?;

    // 写入审计日志
    audit_service::write_audit_log(db, audit_service::AuditLogInput {
        operation: "unban".to_string(),
        target: op.target.clone(),
        target_type: op.target_type.clone(),
        player_name: op.player_name.clone(),
        reason: op.reason.clone(),
        duration_minutes: None,
        operator_name: op.operator_name.clone(),
        operator_steamid: op.operator_steamid.clone(),
        source: "offline_sync".to_string(),
        server_id: Some(server.id),
        server_name: Some(server.name.clone()),
        server_port: Some(server.port),
        success: true,
        message: Some(format!("离线解封已同步，原封禁ID: {}", id)),
        idempotency_key: Some(op.idempotency_key.clone()),
    }).await?;

    Ok(())
}

async fn apply_offline_whitelist_add(
    db: &Database,
    op: &OfflineOperationInput,
    server: &plugin_ban_service::ServerAuth,
) -> anyhow::Result<()> {
    // 检查是否已存在白名单记录
    let existing: Option<(String,)> = sqlx::query_as(
        r#"SELECT status FROM whitelist_requests WHERE steamid64 = $1 ORDER BY updated_at DESC LIMIT 1"#,
    )
    .bind(&op.target)
    .fetch_optional(&db.pool)
    .await?;

    match existing {
        Some((status,)) if status == "approved" => {
            // 已通过，跳过
            audit_service::write_audit_log(db, audit_service::AuditLogInput {
                operation: "whitelist_add".to_string(),
                target: op.target.clone(),
                target_type: "steam".to_string(),
                player_name: op.player_name.clone(),
                reason: None,
                duration_minutes: None,
                operator_name: op.operator_name.clone(),
                operator_steamid: op.operator_steamid.clone(),
                source: "offline_sync".to_string(),
                server_id: Some(server.id),
                server_name: Some(server.name.clone()),
                server_port: Some(server.port),
                success: false,
                message: Some("白名单已存在且已通过，跳过".to_string()),
                idempotency_key: Some(op.idempotency_key.clone()),
            }).await?;
        }
        Some((status,)) if status == "pending" => {
            // 待审核，自动通过
            sqlx::query(
                r#"UPDATE whitelist_requests
                   SET status = 'approved', approved_at = now(), approved_by = $2, updated_at = now()
                   WHERE steamid64 = $1 AND status = 'pending'"#,
            )
            .bind(&op.target)
            .bind(&op.operator_name)
            .execute(&db.pool)
            .await?;

            audit_service::write_audit_log(db, audit_service::AuditLogInput {
                operation: "whitelist_add".to_string(),
                target: op.target.clone(),
                target_type: "steam".to_string(),
                player_name: op.player_name.clone(),
                reason: None,
                duration_minutes: None,
                operator_name: op.operator_name.clone(),
                operator_steamid: op.operator_steamid.clone(),
                source: "offline_sync".to_string(),
                server_id: Some(server.id),
                server_name: Some(server.name.clone()),
                server_port: Some(server.port),
                success: true,
                message: Some("待审核白名单已自动通过".to_string()),
                idempotency_key: Some(op.idempotency_key.clone()),
            }).await?;
        }
        _ => {
            // 创建新的白名单记录
            let nickname = op.player_name.as_deref().unwrap_or("未知玩家");
            sqlx::query(
                r#"INSERT INTO whitelist_requests (
                    id, steam_id, steamid64, nickname, status,
                    applied_at, approved_at, approved_by, source, updated_at
                   )
                   VALUES ($1, $2, $3, $4, 'approved', now(), now(), $5, 'offline_sync', now())"#,
            )
            .bind(Uuid::new_v4())
            .bind(&op.target)
            .bind(&op.target)
            .bind(nickname)
            .bind(&op.operator_name)
            .execute(&db.pool)
            .await?;

            audit_service::write_audit_log(db, audit_service::AuditLogInput {
                operation: "whitelist_add".to_string(),
                target: op.target.clone(),
                target_type: "steam".to_string(),
                player_name: op.player_name.clone(),
                reason: None,
                duration_minutes: None,
                operator_name: op.operator_name.clone(),
                operator_steamid: op.operator_steamid.clone(),
                source: "offline_sync".to_string(),
                server_id: Some(server.id),
                server_name: Some(server.name.clone()),
                server_port: Some(server.port),
                success: true,
                message: Some("离线白名单已同步".to_string()),
                idempotency_key: Some(op.idempotency_key.clone()),
            }).await?;
        }
    }

    Ok(())
}

async fn apply_offline_whitelist_remove(
    db: &Database,
    op: &OfflineOperationInput,
    server: &plugin_ban_service::ServerAuth,
) -> anyhow::Result<()> {
    // 查找并撤销白名单
    let result = sqlx::query(
        r#"UPDATE whitelist_requests
           SET status = 'revoked', revoked_at = now(), revoked_by = $2, updated_at = now()
           WHERE steamid64 = $1 AND status = 'approved'
           RETURNING id"#,
    )
    .bind(&op.target)
    .bind(&op.operator_name)
    .execute(&db.pool)
    .await?;

    if result.rows_affected() == 0 {
        audit_service::write_audit_log(db, audit_service::AuditLogInput {
            operation: "whitelist_remove".to_string(),
            target: op.target.clone(),
            target_type: "steam".to_string(),
            player_name: op.player_name.clone(),
            reason: op.reason.clone(),
            duration_minutes: None,
            operator_name: op.operator_name.clone(),
            operator_steamid: op.operator_steamid.clone(),
            source: "offline_sync".to_string(),
            server_id: Some(server.id),
            server_name: Some(server.name.clone()),
            server_port: Some(server.port),
            success: false,
            message: Some("未找到已通过的白名单，跳过".to_string()),
            idempotency_key: Some(op.idempotency_key.clone()),
        }).await?;
    } else {
        audit_service::write_audit_log(db, audit_service::AuditLogInput {
            operation: "whitelist_remove".to_string(),
            target: op.target.clone(),
            target_type: "steam".to_string(),
            player_name: op.player_name.clone(),
            reason: op.reason.clone(),
            duration_minutes: None,
            operator_name: op.operator_name.clone(),
            operator_steamid: op.operator_steamid.clone(),
            source: "offline_sync".to_string(),
            server_id: Some(server.id),
            server_name: Some(server.name.clone()),
            server_port: Some(server.port),
            success: true,
            message: Some("离线白名单移除已同步".to_string()),
            idempotency_key: Some(op.idempotency_key.clone()),
        }).await?;
    }

    Ok(())
}
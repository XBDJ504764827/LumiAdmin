use crate::db::Database;
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct AuditLogInput {
    pub operation: String,
    pub target: String,
    pub target_type: String,
    pub player_name: Option<String>,
    pub reason: Option<String>,
    pub duration_minutes: Option<i32>,
    /// 稳定关联操作管理员；插件/离线同步等外部来源可以为空。
    pub operator_id: Option<Uuid>,
    /// 操作时的名称快照，避免管理员改名后历史记录显示改变。
    pub operator_name: String,
    pub operator_steamid: Option<String>,
    pub source: String,
    pub server_id: Option<Uuid>,
    pub server_name: Option<String>,
    pub server_port: Option<i32>,
    pub success: bool,
    pub message: Option<String>,
    pub idempotency_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct AuditLogItem {
    pub id: Uuid,
    pub operation: String,
    pub target: String,
    pub target_type: String,
    pub player_name: Option<String>,
    pub reason: Option<String>,
    pub duration_minutes: Option<i32>,
    pub operator_id: Option<Uuid>,
    pub operator_name: String,
    pub operator_steamid: Option<String>,
    pub source: String,
    pub server_id: Option<Uuid>,
    pub server_name: Option<String>,
    pub server_port: Option<i32>,
    pub success: bool,
    pub message: Option<String>,
    pub client_ip: Option<String>,
    pub details: Option<Value>,
    pub idempotency_key: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct AuditLogQuery {
    pub server_id: Option<Uuid>,
    pub operation: Option<String>,
    pub operator_name: Option<String>,
    pub target: Option<String>,
    pub search: Option<String>,
    pub source: Option<String>,
    pub success: Option<bool>,
    pub page: i64,
    pub page_size: i64,
}

/// 写入审计日志
pub async fn write_audit_log(db: &Database, input: AuditLogInput) -> anyhow::Result<AuditLogItem> {
    write_audit_log_with_context(db, input, None, None).await
}

/// 写入带客户端上下文和结构化详情的审计日志。
pub async fn write_audit_log_with_context(
    db: &Database,
    input: AuditLogInput,
    client_ip: Option<String>,
    details: Option<Value>,
) -> anyhow::Result<AuditLogItem> {
    let id = Uuid::new_v4();
    let row = sqlx::query_as::<_, AuditLogItem>(
        r#"INSERT INTO audit_logs (
            id, operation, target, target_type, player_name, reason, duration_minutes,
            operator_id, operator_name, operator_steamid, source, server_id, server_name, server_port,
            success, message, client_ip, details, idempotency_key, created_at
           )
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, now())
           RETURNING id, operation, target, target_type, player_name, reason, duration_minutes,
                     operator_id, operator_name, operator_steamid, source, server_id, server_name, server_port,
                     success, message, client_ip, details, idempotency_key, created_at"#,
    )
    .bind(id)
    .bind(&input.operation)
    .bind(&input.target)
    .bind(&input.target_type)
    .bind(&input.player_name)
    .bind(&input.reason)
    .bind(input.duration_minutes)
    .bind(input.operator_id)
    .bind(&input.operator_name)
    .bind(&input.operator_steamid)
    .bind(&input.source)
    .bind(input.server_id)
    .bind(&input.server_name)
    .bind(input.server_port)
    .bind(input.success)
    .bind(&input.message)
    .bind(&client_ip)
    .bind(&details)
    .bind(&input.idempotency_key)
    .fetch_one(&db.pool)
    .await?;
    Ok(row)
}

/// 在调用方事务中写入审计日志，用于业务状态与审计记录原子提交。
pub async fn write_audit_log_in_transaction(
    tx: &mut Transaction<'_, Postgres>,
    input: AuditLogInput,
    client_ip: Option<String>,
    details: Option<Value>,
) -> anyhow::Result<AuditLogItem> {
    let id = Uuid::new_v4();
    let row = sqlx::query_as::<_, AuditLogItem>(
        r#"INSERT INTO audit_logs (
            id, operation, target, target_type, player_name, reason, duration_minutes,
            operator_id, operator_name, operator_steamid, source, server_id, server_name, server_port,
            success, message, client_ip, details, idempotency_key, created_at
           )
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, now())
           RETURNING id, operation, target, target_type, player_name, reason, duration_minutes,
                     operator_id, operator_name, operator_steamid, source, server_id, server_name, server_port,
                     success, message, client_ip, details, idempotency_key, created_at"#,
    )
    .bind(id)
    .bind(&input.operation)
    .bind(&input.target)
    .bind(&input.target_type)
    .bind(&input.player_name)
    .bind(&input.reason)
    .bind(input.duration_minutes)
    .bind(input.operator_id)
    .bind(&input.operator_name)
    .bind(&input.operator_steamid)
    .bind(&input.source)
    .bind(input.server_id)
    .bind(&input.server_name)
    .bind(input.server_port)
    .bind(input.success)
    .bind(&input.message)
    .bind(&client_ip)
    .bind(&details)
    .bind(&input.idempotency_key)
    .fetch_one(&mut **tx)
    .await?;
    Ok(row)
}

/// 查询审计日志
pub async fn list_audit_logs(
    db: &Database,
    query: &AuditLogQuery,
) -> anyhow::Result<(Vec<AuditLogItem>, i64)> {
    let mut conditions = Vec::new();
    let mut param_idx = 1u32;

    if let Some(ref _server_id) = query.server_id {
        conditions.push(format!("al.server_id = ${}", param_idx));
        param_idx += 1;
    }
    if let Some(ref operation) = query.operation {
        if !operation.trim().is_empty() {
            conditions.push(format!("al.operation = ${}", param_idx));
            param_idx += 1;
        }
    }
    if let Some(ref operator_name) = query.operator_name {
        if !operator_name.trim().is_empty() {
            conditions.push(format!(
                "(al.operator_name ILIKE ${} OR operator_user.display_name ILIKE ${})",
                param_idx, param_idx
            ));
            param_idx += 1;
        }
    }
    if let Some(ref target) = query.target {
        if !target.trim().is_empty() {
            conditions.push(format!("al.target ILIKE ${}", param_idx));
            param_idx += 1;
        }
    }
    if let Some(ref search) = query.search {
        if !search.trim().is_empty() {
            conditions.push(format!(
                "(al.target ILIKE ${0} OR al.player_name ILIKE ${0} OR al.operator_name ILIKE ${0} OR al.reason ILIKE ${0} OR al.message ILIKE ${0} OR al.client_ip ILIKE ${0} OR al.server_name ILIKE ${0} OR al.details::TEXT ILIKE ${0})",
                param_idx
            ));
            param_idx += 1;
        }
    }
    if let Some(ref source) = query.source {
        if !source.trim().is_empty() {
            conditions.push(format!("al.source = ${}", param_idx));
            param_idx += 1;
        }
    }
    if let Some(_success) = query.success {
        conditions.push(format!("al.success = ${}", param_idx));
        param_idx += 1;
    }

    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };

    let operator_join = r#"LEFT JOIN users operator_user ON operator_user.id = al.operator_id"#;

    let count_sql = format!("SELECT COUNT(*) FROM audit_logs al {operator_join} {where_clause}");
    let data_sql = format!(
        r#"SELECT al.id, al.operation, al.target, al.target_type, al.player_name, al.reason, al.duration_minutes,
                  al.operator_id, al.operator_name, al.operator_steamid, al.source, al.server_id, al.server_name, al.server_port,
                  al.success, al.message, al.client_ip, al.details, al.idempotency_key, al.created_at
           FROM audit_logs al {operator_join} {}
           ORDER BY al.created_at DESC
           LIMIT ${} OFFSET ${}"#,
        where_clause,
        param_idx,
        param_idx + 1
    );

    let mut count_query = sqlx::query_scalar::<_, i64>(&count_sql);
    let mut data_query = sqlx::query_as::<_, AuditLogItem>(&data_sql);

    if let Some(ref server_id) = query.server_id {
        count_query = count_query.bind(server_id);
        data_query = data_query.bind(server_id);
    }
    if let Some(ref operation) = query.operation {
        if !operation.trim().is_empty() {
            count_query = count_query.bind(operation.trim());
            data_query = data_query.bind(operation.trim());
        }
    }
    if let Some(ref operator_name) = query.operator_name {
        if !operator_name.trim().is_empty() {
            count_query = count_query.bind(format!("%{}%", operator_name.trim()));
            data_query = data_query.bind(format!("%{}%", operator_name.trim()));
        }
    }
    if let Some(ref target) = query.target {
        if !target.trim().is_empty() {
            count_query = count_query.bind(format!("%{}%", target.trim()));
            data_query = data_query.bind(format!("%{}%", target.trim()));
        }
    }
    if let Some(ref search) = query.search {
        if !search.trim().is_empty() {
            let pattern = format!("%{}%", search.trim());
            count_query = count_query.bind(pattern.clone());
            data_query = data_query.bind(pattern);
        }
    }
    if let Some(ref source) = query.source {
        if !source.trim().is_empty() {
            count_query = count_query.bind(source.trim());
            data_query = data_query.bind(source.trim());
        }
    }
    if let Some(success) = query.success {
        count_query = count_query.bind(success);
        data_query = data_query.bind(success);
    }

    data_query = data_query
        .bind(query.page_size)
        .bind((query.page - 1) * query.page_size);

    let total = count_query.fetch_one(&db.pool).await?;
    let items = data_query.fetch_all(&db.pool).await?;

    Ok((items, total))
}

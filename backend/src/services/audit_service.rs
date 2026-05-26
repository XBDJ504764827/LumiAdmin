use crate::db::Database;
use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct AuditLogInput {
    pub operation: String,
    pub target: String,
    pub target_type: String,
    pub player_name: Option<String>,
    pub reason: Option<String>,
    pub duration_minutes: Option<i32>,
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
    pub operator_name: String,
    pub operator_steamid: Option<String>,
    pub source: String,
    pub server_id: Option<Uuid>,
    pub server_name: Option<String>,
    pub server_port: Option<i32>,
    pub success: bool,
    pub message: Option<String>,
    pub idempotency_key: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct AuditLogQuery {
    pub server_id: Option<Uuid>,
    pub operation: Option<String>,
    pub operator_name: Option<String>,
    pub target: Option<String>,
    pub source: Option<String>,
    pub success: Option<bool>,
    pub page: i64,
    pub page_size: i64,
}

/// 写入审计日志
pub async fn write_audit_log(db: &Database, input: AuditLogInput) -> anyhow::Result<AuditLogItem> {
    let id = Uuid::new_v4();
    let row = sqlx::query_as::<_, AuditLogItem>(
        r#"INSERT INTO audit_logs (
            id, operation, target, target_type, player_name, reason, duration_minutes,
            operator_name, operator_steamid, source, server_id, server_name, server_port,
            success, message, idempotency_key, created_at
           )
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, now())
           RETURNING id, operation, target, target_type, player_name, reason, duration_minutes,
                     operator_name, operator_steamid, source, server_id, server_name, server_port,
                     success, message, idempotency_key, created_at"#,
    )
    .bind(id)
    .bind(&input.operation)
    .bind(&input.target)
    .bind(&input.target_type)
    .bind(&input.player_name)
    .bind(&input.reason)
    .bind(input.duration_minutes)
    .bind(&input.operator_name)
    .bind(&input.operator_steamid)
    .bind(&input.source)
    .bind(input.server_id)
    .bind(&input.server_name)
    .bind(input.server_port)
    .bind(input.success)
    .bind(&input.message)
    .bind(&input.idempotency_key)
    .fetch_one(&db.pool)
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
        conditions.push(format!("server_id = ${}", param_idx));
        param_idx += 1;
    }
    if let Some(ref operation) = query.operation {
        if !operation.trim().is_empty() {
            conditions.push(format!("operation = ${}", param_idx));
            param_idx += 1;
        }
    }
    if let Some(ref operator_name) = query.operator_name {
        if !operator_name.trim().is_empty() {
            conditions.push(format!("operator_name ILIKE ${}", param_idx));
            param_idx += 1;
        }
    }
    if let Some(ref target) = query.target {
        if !target.trim().is_empty() {
            conditions.push(format!("target ILIKE ${}", param_idx));
            param_idx += 1;
        }
    }
    if let Some(ref source) = query.source {
        if !source.trim().is_empty() {
            conditions.push(format!("source = ${}", param_idx));
            param_idx += 1;
        }
    }
    if let Some(_success) = query.success {
        conditions.push(format!("success = ${}", param_idx));
        param_idx += 1;
    }

    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };

    let count_sql = format!("SELECT COUNT(*) FROM audit_logs {}", where_clause);
    let data_sql = format!(
        r#"SELECT id, operation, target, target_type, player_name, reason, duration_minutes,
                  operator_name, operator_steamid, source, server_id, server_name, server_port,
                  success, message, idempotency_key, created_at
           FROM audit_logs {}
           ORDER BY created_at DESC
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

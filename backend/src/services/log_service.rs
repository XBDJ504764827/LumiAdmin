use crate::{db::Database, routes::ListQuery};
use serde::Serialize;
use uuid::Uuid;

#[derive(Serialize)]
pub struct LogItem {
    pub operator_name: String,
    pub module: String,
    pub action: String,
    pub target_detail: String,
    pub ip_address: String,
    pub created_at: String,
}

pub async fn list_logs(
    db: &Database,
    query: &ListQuery,
) -> anyhow::Result<crate::routes::PaginatedResponse<LogItem>> {
    let mut conditions = vec!["COALESCE(u.role, '') <> 'developer'".to_string()];
    let mut param_idx = 1u32;
    let search_pattern = query.search_pattern();

    if search_pattern.is_some() {
        conditions.push(format!("(l.operator_name ILIKE ${param_idx} OR l.module ILIKE ${param_idx} OR l.action ILIKE ${param_idx})"));
        param_idx += 1;
    }

    let where_clause = format!("WHERE {}", conditions.join(" AND "));

    let count_sql = format!("SELECT COUNT(*) FROM admin_logs l LEFT JOIN users u ON u.display_name = l.operator_name {where_clause}");
    let data_sql = format!(
        r#"SELECT l.operator_name, l.module, l.action, l.target_detail, l.ip_address, l.created_at
           FROM admin_logs l LEFT JOIN users u ON u.display_name = l.operator_name
           {where_clause} ORDER BY l.created_at DESC LIMIT ${param_idx} OFFSET ${}"#,
        param_idx + 1
    );

    let mut count_query = sqlx::query_scalar::<_, i64>(&count_sql);
    let mut data_query = sqlx::query_as::<
        _,
        (
            String,
            String,
            String,
            String,
            String,
            chrono::DateTime<chrono::Utc>,
        ),
    >(&data_sql);

    if let Some(ref pattern) = search_pattern {
        count_query = count_query.bind(pattern);
        data_query = data_query.bind(pattern);
    }
    data_query = data_query.bind(query.page_size()).bind(query.offset());

    let total = count_query.fetch_one(&db.pool).await?;
    let items = data_query
        .fetch_all(&db.pool)
        .await?
        .into_iter()
        .map(
            |(operator_name, module, action, target_detail, ip_address, created_at)| LogItem {
                operator_name,
                module,
                action,
                target_detail,
                ip_address,
                created_at: created_at.to_rfc3339(),
            },
        )
        .collect();

    Ok(crate::routes::PaginatedResponse {
        items,
        total,
        page: query.page(),
        page_size: query.page_size(),
    })
}

pub async fn create_log(
    db: &Database,
    operator_name: &str,
    module: &str,
    action: &str,
    target_detail: &str,
    ip_address: &str,
) -> anyhow::Result<()> {
    sqlx::query(
        r#"INSERT INTO admin_logs (id, operator_name, module, action, target_detail, ip_address)
           VALUES ($1, $2, $3, $4, $5, $6)"#,
    )
    .bind(Uuid::new_v4())
    .bind(operator_name)
    .bind(module)
    .bind(action)
    .bind(target_detail)
    .bind(ip_address)
    .execute(&db.pool)
    .await?;
    Ok(())
}

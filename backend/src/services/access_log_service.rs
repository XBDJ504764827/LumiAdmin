// 进服记录监控服务
use crate::db::Database;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// 进服方式
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AccessMethod {
    /// 无限制放行
    Unrestricted,
    /// 白名单通过
    Whitelist,
    /// Rating/Steam 等级限制通过
    Restriction,
    /// 自定义权限规则通过
    CustomRule,
    /// 自定义权限规则拒绝（不应出现在进服记录中）
    CustomRuleBlocked,
}

impl AccessMethod {
    pub fn as_str(&self) -> &'static str {
        match self {
            AccessMethod::Unrestricted => "unrestricted",
            AccessMethod::Whitelist => "whitelist",
            AccessMethod::Restriction => "restriction",
            AccessMethod::CustomRule => "custom_rule",
            AccessMethod::CustomRuleBlocked => "custom_rule_blocked",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "unrestricted" => AccessMethod::Unrestricted,
            "whitelist" => AccessMethod::Whitelist,
            "restriction" => AccessMethod::Restriction,
            "custom_rule" => AccessMethod::CustomRule,
            "custom_rule_blocked" => AccessMethod::CustomRuleBlocked,
            _ => AccessMethod::Unrestricted,
        }
    }
}

/// 进服记录
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct PlayerAccessLog {
    pub id: Uuid,
    pub steam_id64: String,
    pub player_name: Option<String>,
    pub ip_address: Option<String>,
    pub server_id: Uuid,
    pub server_name: String,
    pub server_port: i32,
    pub community_id: Uuid,
    pub community_name: Option<String>,
    pub access_method: String,
    pub rating: Option<i32>,
    pub steam_level: Option<i32>,
    pub created_at: DateTime<Utc>,
}

/// 创建进服记录
pub async fn create_access_log(
    db: &Database,
    steam_id64: &str,
    player_name: Option<&str>,
    ip_address: Option<&str>,
    server_id: Uuid,
    server_name: &str,
    server_port: i32,
    community_id: Uuid,
    community_name: Option<&str>,
    access_method: &AccessMethod,
    rating: Option<i32>,
    steam_level: Option<i32>,
) -> anyhow::Result<PlayerAccessLog> {
    let row = sqlx::query_as::<_, PlayerAccessLog>(
        r#"INSERT INTO player_access_logs (
            id, steam_id64, player_name, ip_address, server_id, server_name, server_port,
            community_id, community_name, access_method, rating, steam_level, created_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, now())
        RETURNING *"#,
    )
    .bind(Uuid::new_v4())
    .bind(steam_id64)
    .bind(player_name)
    .bind(ip_address)
    .bind(server_id)
    .bind(server_name)
    .bind(server_port)
    .bind(community_id)
    .bind(community_name)
    .bind(access_method.as_str())
    .bind(rating)
    .bind(steam_level)
    .fetch_one(&db.pool)
    .await?;

    Ok(row)
}

/// 查询进服记录（分页）
pub async fn query_access_logs(
    db: &Database,
    params: &AccessLogQueryParams,
) -> anyhow::Result<(Vec<PlayerAccessLog>, i64)> {
    let mut conditions = Vec::new();
    let mut param_idx = 1;
    let mut params_list: Vec<Box<dyn sqlx::Encode<'_, sqlx::Postgres>>> = Vec::new();

    if let Some(steam_id64) = &params.steam_id64 {
        conditions.push(format!("steam_id64 = ${}", param_idx));
        params_list.push(Box::new(steam_id64.clone()));
        param_idx += 1;
    }

    if let Some(server_id) = &params.server_id {
        conditions.push(format!("server_id = ${}", param_idx));
        params_list.push(Box::new(*server_id));
        param_idx += 1;
    }

    if let Some(community_id) = &params.community_id {
        conditions.push(format!("community_id = ${}", param_idx));
        params_list.push(Box::new(*community_id));
        param_idx += 1;
    }

    if let Some(access_method) = &params.access_method {
        conditions.push(format!("access_method = ${}", param_idx));
        params_list.push(Box::new(access_method.to_string()));
        param_idx += 1;
    }

    if let Some(search) = &params.search {
        conditions.push(format!(
            "(steam_id64 ILIKE ${} OR player_name ILIKE ${} OR server_name ILIKE ${})",
            param_idx, param_idx, param_idx
        ));
        let search_pattern = format!("%{}%", search);
        params_list.push(Box::new(search_pattern.clone()));
        params_list.push(Box::new(search_pattern.clone()));
        params_list.push(Box::new(search_pattern));
        param_idx += 3;
    }

    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };

    // 查询总数
    let count_sql = format!(
        "SELECT COUNT(*) FROM player_access_logs {}",
        where_clause
    );

    let count: i64 = sqlx::query_as(&count_sql)
        .fetch_one(&db.pool)
        .await
        .map(|(c,)| c)?;

    // 查询列表
    let offset = (params.page.unwrap_or(1) - 1) * params.page_size.unwrap_or(20);
    let limit = params.page_size.unwrap_or(20);

    let list_sql = format!(
        r#"SELECT * FROM player_access_logs {}
           ORDER BY created_at DESC
           LIMIT {} OFFSET {}"#,
        where_clause, limit, offset
    );

    let rows: Vec<PlayerAccessLog> = sqlx::query_as(&list_sql)
        .fetch_all(&db.pool)
        .await?;

    Ok((rows, count))
}

/// 查询参数
#[derive(Debug, Clone, Default)]
pub struct AccessLogQueryParams {
    pub steam_id64: Option<String>,
    pub server_id: Option<Uuid>,
    pub community_id: Option<Uuid>,
    pub access_method: Option<String>,
    pub search: Option<String>,
    pub page: Option<i64>,
    pub page_size: Option<i64>,
}

/// 分页查询进服记录（简化版，避免泛型导致的 Handler trait 问题）
pub async fn list_logs_simple(
    db: &Database,
    page: i64,
    page_size: i64,
    steam_id64: Option<String>,
    server_id: Option<Uuid>,
) -> anyhow::Result<(Vec<PlayerAccessLog>, i64)> {
    let offset = (page - 1) * page_size;

    let (items, total) = if let Some(ref sid) = steam_id64 {
        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM player_access_logs WHERE steam_id64 = $1"
        ).bind(sid).fetch_one(&db.pool).await?;
        let rows = sqlx::query_as::<_, PlayerAccessLog>(
            "SELECT * FROM player_access_logs WHERE steam_id64 = $1 ORDER BY created_at DESC LIMIT $2 OFFSET $3"
        ).bind(sid).bind(page_size).bind(offset).fetch_all(&db.pool).await?;
        (rows, count.0)
    } else if let Some(ref sid) = server_id {
        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM player_access_logs WHERE server_id = $1"
        ).bind(sid).fetch_one(&db.pool).await?;
        let rows = sqlx::query_as::<_, PlayerAccessLog>(
            "SELECT * FROM player_access_logs WHERE server_id = $1 ORDER BY created_at DESC LIMIT $2 OFFSET $3"
        ).bind(sid).bind(page_size).bind(offset).fetch_all(&db.pool).await?;
        (rows, count.0)
    } else {
        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM player_access_logs"
        ).fetch_one(&db.pool).await?;
        let rows = sqlx::query_as::<_, PlayerAccessLog>(
            "SELECT * FROM player_access_logs ORDER BY created_at DESC LIMIT $1 OFFSET $2"
        ).bind(page_size).bind(offset).fetch_all(&db.pool).await?;
        (rows, count.0)
    };

    Ok((items, total))
}
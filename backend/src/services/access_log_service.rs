// 进服记录监控服务
use crate::db::Database;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// 进服方式
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AccessMethod {
    // ---- 进服成功 ----
    /// 无限制放行
    Unrestricted,
    /// 白名单通过
    Whitelist,
    /// Rating/Steam 等级限制通过
    Restriction,
    /// 自定义权限规则通过
    CustomRule,
    // ---- 进服失败 ----
    /// 被封禁
    Banned,
    /// 白名单未通过
    WhitelistRejected,
    /// Rating/Steam 等级不足
    RestrictionRejected,
    /// 自定义权限规则拒绝
    CustomRuleRejected,
    /// 快照回退（服务降级）
    SnapshotFallback,
}

impl AccessMethod {
    pub fn as_str(&self) -> &'static str {
        match self {
            AccessMethod::Unrestricted => "unrestricted",
            AccessMethod::Whitelist => "whitelist",
            AccessMethod::Restriction => "restriction",
            AccessMethod::CustomRule => "custom_rule",
            AccessMethod::Banned => "banned",
            AccessMethod::WhitelistRejected => "whitelist_rejected",
            AccessMethod::RestrictionRejected => "restriction_rejected",
            AccessMethod::CustomRuleRejected => "custom_rule_rejected",
            AccessMethod::SnapshotFallback => "snapshot_fallback",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "unrestricted" => AccessMethod::Unrestricted,
            "whitelist" => AccessMethod::Whitelist,
            "restriction" => AccessMethod::Restriction,
            "custom_rule" => AccessMethod::CustomRule,
            "banned" => AccessMethod::Banned,
            "whitelist_rejected" => AccessMethod::WhitelistRejected,
            "restriction_rejected" => AccessMethod::RestrictionRejected,
            "custom_rule_rejected" => AccessMethod::CustomRuleRejected,
            "snapshot_fallback" => AccessMethod::SnapshotFallback,
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
    pub allowed: bool,
    pub access_method: String,
    pub reject_reason: Option<String>,
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
    allowed: bool,
    access_method: &AccessMethod,
    reject_reason: Option<&str>,
    rating: Option<i32>,
    steam_level: Option<i32>,
) -> anyhow::Result<PlayerAccessLog> {
    let row = sqlx::query_as::<_, PlayerAccessLog>(
        r#"INSERT INTO player_access_logs (
            id, steam_id64, player_name, ip_address, server_id, server_name, server_port,
            community_id, community_name, allowed, access_method, reject_reason, rating, steam_level, created_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, now())
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
    .bind(allowed)
    .bind(access_method.as_str())
    .bind(reject_reason)
    .bind(rating)
    .bind(steam_level)
    .fetch_one(&db.pool)
    .await?;

    Ok(row)
}

/// 查询参数
#[derive(Debug, Clone, Default)]
pub struct AccessLogQueryParams {
    pub steam_id64: Option<String>,
    pub server_id: Option<Uuid>,
    pub community_id: Option<Uuid>,
    pub access_method: Option<String>,
    pub allowed: Option<bool>,
    pub search: Option<String>,
    pub page: Option<i64>,
    pub page_size: Option<i64>,
}

/// 分页查询进服记录
///
/// 由于 Rust/sqlx 的泛型推断限制，动态 SQL 使用逐分支 bind 而非动态参数列表。
/// 每种筛选组合独立实现，保证类型安全的参数绑定。
pub async fn query_access_logs(
    db: &Database,
    params: &AccessLogQueryParams,
) -> anyhow::Result<(Vec<PlayerAccessLog>, i64)> {
    let page = params.page.unwrap_or(1).max(1);
    let page_size = params.page_size.unwrap_or(20).clamp(1, 100);
    let offset = (page - 1) * page_size;

    // 根据参数组合选择查询分支
    match (&params.steam_id64, &params.server_id, &params.community_id, &params.access_method, &params.allowed, &params.search) {
        // 无筛选 — 全量分页
        (None, None, None, None, None, None) => {
            let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM player_access_logs")
                .fetch_one(&db.pool).await?;
            let rows = sqlx::query_as::<_, PlayerAccessLog>(
                "SELECT * FROM player_access_logs ORDER BY created_at DESC LIMIT $1 OFFSET $2"
            ).bind(page_size).bind(offset).fetch_all(&db.pool).await?;
            Ok((rows, count.0))
        }

        // 仅 steam_id64
        (Some(sid), None, None, None, None, None) => {
            let count: (i64,) = sqlx::query_as(
                "SELECT COUNT(*) FROM player_access_logs WHERE steam_id64 = $1"
            ).bind(sid).fetch_one(&db.pool).await?;
            let rows = sqlx::query_as::<_, PlayerAccessLog>(
                "SELECT * FROM player_access_logs WHERE steam_id64 = $1 ORDER BY created_at DESC LIMIT $2 OFFSET $3"
            ).bind(sid).bind(page_size).bind(offset).fetch_all(&db.pool).await?;
            Ok((rows, count.0))
        }

        // 仅 server_id
        (None, Some(sid), None, None, None, None) => {
            let count: (i64,) = sqlx::query_as(
                "SELECT COUNT(*) FROM player_access_logs WHERE server_id = $1"
            ).bind(sid).fetch_one(&db.pool).await?;
            let rows = sqlx::query_as::<_, PlayerAccessLog>(
                "SELECT * FROM player_access_logs WHERE server_id = $1 ORDER BY created_at DESC LIMIT $2 OFFSET $3"
            ).bind(sid).bind(page_size).bind(offset).fetch_all(&db.pool).await?;
            Ok((rows, count.0))
        }

        // 仅 community_id
        (None, None, Some(cid), None, None, None) => {
            let count: (i64,) = sqlx::query_as(
                "SELECT COUNT(*) FROM player_access_logs WHERE community_id = $1"
            ).bind(cid).fetch_one(&db.pool).await?;
            let rows = sqlx::query_as::<_, PlayerAccessLog>(
                "SELECT * FROM player_access_logs WHERE community_id = $1 ORDER BY created_at DESC LIMIT $2 OFFSET $3"
            ).bind(cid).bind(page_size).bind(offset).fetch_all(&db.pool).await?;
            Ok((rows, count.0))
        }

        // 仅 access_method
        (None, None, None, Some(am), None, None) => {
            let count: (i64,) = sqlx::query_as(
                "SELECT COUNT(*) FROM player_access_logs WHERE access_method = $1"
            ).bind(am).fetch_one(&db.pool).await?;
            let rows = sqlx::query_as::<_, PlayerAccessLog>(
                "SELECT * FROM player_access_logs WHERE access_method = $1 ORDER BY created_at DESC LIMIT $2 OFFSET $3"
            ).bind(am).bind(page_size).bind(offset).fetch_all(&db.pool).await?;
            Ok((rows, count.0))
        }

        // 仅 allowed
        (None, None, None, None, Some(a), None) => {
            let count: (i64,) = sqlx::query_as(
                "SELECT COUNT(*) FROM player_access_logs WHERE allowed = $1"
            ).bind(a).fetch_one(&db.pool).await?;
            let rows = sqlx::query_as::<_, PlayerAccessLog>(
                "SELECT * FROM player_access_logs WHERE allowed = $1 ORDER BY created_at DESC LIMIT $2 OFFSET $3"
            ).bind(a).bind(page_size).bind(offset).fetch_all(&db.pool).await?;
            Ok((rows, count.0))
        }

        // 仅 search
        (None, None, None, None, None, Some(q)) => {
            let pattern = format!("%{q}%");
            let count: (i64,) = sqlx::query_as(
                "SELECT COUNT(*) FROM player_access_logs WHERE steam_id64 ILIKE $1 OR player_name ILIKE $1 OR server_name ILIKE $1"
            ).bind(&pattern).fetch_one(&db.pool).await?;
            let rows = sqlx::query_as::<_, PlayerAccessLog>(
                "SELECT * FROM player_access_logs WHERE steam_id64 ILIKE $1 OR player_name ILIKE $1 OR server_name ILIKE $1 ORDER BY created_at DESC LIMIT $2 OFFSET $3"
            ).bind(&pattern).bind(page_size).bind(offset).fetch_all(&db.pool).await?;
            Ok((rows, count.0))
        }

        // allowed + access_method
        (None, None, None, Some(am), Some(a), None) => {
            let count: (i64,) = sqlx::query_as(
                "SELECT COUNT(*) FROM player_access_logs WHERE access_method = $1 AND allowed = $2"
            ).bind(am).bind(a).fetch_one(&db.pool).await?;
            let rows = sqlx::query_as::<_, PlayerAccessLog>(
                "SELECT * FROM player_access_logs WHERE access_method = $1 AND allowed = $2 ORDER BY created_at DESC LIMIT $3 OFFSET $4"
            ).bind(am).bind(a).bind(page_size).bind(offset).fetch_all(&db.pool).await?;
            Ok((rows, count.0))
        }

        // steam_id64 + allowed
        (Some(sid), None, None, None, Some(a), None) => {
            let count: (i64,) = sqlx::query_as(
                "SELECT COUNT(*) FROM player_access_logs WHERE steam_id64 = $1 AND allowed = $2"
            ).bind(sid).bind(a).fetch_one(&db.pool).await?;
            let rows = sqlx::query_as::<_, PlayerAccessLog>(
                "SELECT * FROM player_access_logs WHERE steam_id64 = $1 AND allowed = $2 ORDER BY created_at DESC LIMIT $3 OFFSET $4"
            ).bind(sid).bind(a).bind(page_size).bind(offset).fetch_all(&db.pool).await?;
            Ok((rows, count.0))
        }

        // steam_id64 + access_method
        (Some(sid), None, None, Some(am), None, None) => {
            let count: (i64,) = sqlx::query_as(
                "SELECT COUNT(*) FROM player_access_logs WHERE steam_id64 = $1 AND access_method = $2"
            ).bind(sid).bind(am).fetch_one(&db.pool).await?;
            let rows = sqlx::query_as::<_, PlayerAccessLog>(
                "SELECT * FROM player_access_logs WHERE steam_id64 = $1 AND access_method = $2 ORDER BY created_at DESC LIMIT $3 OFFSET $4"
            ).bind(sid).bind(am).bind(page_size).bind(offset).fetch_all(&db.pool).await?;
            Ok((rows, count.0))
        }

        // 通用 fallback：构建动态 SQL
        _ => {
            let mut conditions = Vec::new();
            let has_steam = params.steam_id64.is_some();
            let has_server = params.server_id.is_some();
            let has_community = params.community_id.is_some();
            let has_method = params.access_method.is_some();
            let has_allowed = params.allowed.is_some();
            let has_search = params.search.is_some();

            let idx_steam = if has_steam { 1 } else { 0 };
            let idx_server = idx_steam + if has_server { 1 } else { 0 };
            let idx_community = idx_server + if has_community { 1 } else { 0 };
            let idx_method = idx_community + if has_method { 1 } else { 0 };
            let idx_allowed = idx_method + if has_allowed { 1 } else { 0 };
            let idx_search = idx_allowed + if has_search { 1 } else { 0 };

            if has_steam { conditions.push(format!("steam_id64 = ${idx_steam}")); }
            if has_server { conditions.push(format!("server_id = ${idx_server}")); }
            if has_community { conditions.push(format!("community_id = ${idx_community}")); }
            if has_method { conditions.push(format!("access_method = ${idx_method}")); }
            if has_allowed { conditions.push(format!("allowed = ${idx_allowed}")); }
            if has_search {
                conditions.push(format!(
                    "(steam_id64 ILIKE ${idx_search} OR player_name ILIKE ${idx_search} OR server_name ILIKE ${idx_search})"
                ));
            }

            let where_clause = if conditions.is_empty() {
                String::new()
            } else {
                format!("WHERE {}", conditions.join(" AND "))
            };

            use sqlx::Row;
            let count_sql = format!("SELECT COUNT(*) FROM player_access_logs {where_clause}");
            let list_sql = format!(
                "SELECT * FROM player_access_logs {where_clause} ORDER BY created_at DESC LIMIT {page_size} OFFSET {offset}"
            );

            let mut count_query = sqlx::query(&count_sql);
            if let Some(ref v) = params.steam_id64 { count_query = count_query.bind(v); }
            if let Some(ref v) = params.server_id { count_query = count_query.bind(v); }
            if let Some(ref v) = params.community_id { count_query = count_query.bind(v); }
            if let Some(ref v) = params.access_method { count_query = count_query.bind(v); }
            if let Some(v) = params.allowed { count_query = count_query.bind(v); }
            if let Some(ref v) = params.search { count_query = count_query.bind(format!("%{}%", v)); }

            let count_row = count_query.fetch_one(&db.pool).await?;
            let total: i64 = count_row.get(0);

            let mut list_query = sqlx::query(&list_sql);
            if let Some(ref v) = params.steam_id64 { list_query = list_query.bind(v); }
            if let Some(ref v) = params.server_id { list_query = list_query.bind(v); }
            if let Some(ref v) = params.community_id { list_query = list_query.bind(v); }
            if let Some(ref v) = params.access_method { list_query = list_query.bind(v); }
            if let Some(v) = params.allowed { list_query = list_query.bind(v); }
            if let Some(ref v) = params.search { list_query = list_query.bind(format!("%{}%", v)); }

            let rows = list_query.fetch_all(&db.pool).await?;
            let items: Vec<PlayerAccessLog> = rows.iter().map(|row| PlayerAccessLog {
                id: row.get("id"),
                steam_id64: row.get("steam_id64"),
                player_name: row.get("player_name"),
                ip_address: row.get("ip_address"),
                server_id: row.get("server_id"),
                server_name: row.get("server_name"),
                server_port: row.get("server_port"),
                community_id: row.get("community_id"),
                community_name: row.get("community_name"),
                allowed: row.get("allowed"),
                access_method: row.get("access_method"),
                reject_reason: row.get("reject_reason"),
                rating: row.get("rating"),
                steam_level: row.get("steam_level"),
                created_at: row.get("created_at"),
            }).collect();

            Ok((items, total))
        }
    }
}

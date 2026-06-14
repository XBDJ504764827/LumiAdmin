// 进服记录监控服务
use crate::db::Database;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, QueryBuilder};
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
    pub failure_code: Option<String>,
    pub reject_reason: Option<String>,
    pub rating: Option<i32>,
    pub steam_level: Option<i32>,
    pub created_at: DateTime<Utc>,
}

/// 创建进服记录
#[allow(clippy::too_many_arguments)]
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
    failure_code: Option<&str>,
    reject_reason: Option<&str>,
    rating: Option<i32>,
    steam_level: Option<i32>,
) -> anyhow::Result<PlayerAccessLog> {
    let row = sqlx::query_as::<_, PlayerAccessLog>(
        r#"INSERT INTO player_access_logs (
            id, steam_id64, player_name, ip_address, server_id, server_name, server_port,
            community_id, community_name, allowed, access_method, failure_code, reject_reason, rating, steam_level, created_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, now())
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
    .bind(failure_code)
    .bind(reject_reason)
    .bind(rating)
    .bind(steam_level)
    .fetch_one(&db.pool)
    .await?;

    Ok(row)
}

/// 查询参数
#[derive(Debug, Clone, Default, Deserialize)]
pub struct AccessLogQueryParams {
    pub steam_id64: Option<String>,
    pub server_id: Option<Uuid>,
    pub community_id: Option<Uuid>,
    pub access_method: Option<String>,
    pub allowed: Option<bool>,
    pub failure_code: Option<String>,
    pub ip_address: Option<String>,
    pub search: Option<String>,
    pub page: Option<i64>,
    pub page_size: Option<i64>,
}

impl AccessLogQueryParams {
    pub fn page(&self) -> i64 {
        self.page.unwrap_or(1).max(1)
    }

    pub fn page_size(&self) -> i64 {
        self.page_size.unwrap_or(20).clamp(1, 100)
    }

    fn offset(&self) -> i64 {
        (self.page() - 1) * self.page_size()
    }
}

fn push_filter_prefix(builder: &mut QueryBuilder<'_, Postgres>, has_where: &mut bool) {
    if *has_where {
        builder.push(" AND ");
    } else {
        builder.push(" WHERE ");
        *has_where = true;
    }
}

fn push_filters(builder: &mut QueryBuilder<'_, Postgres>, params: &AccessLogQueryParams) {
    let mut has_where = false;

    if let Some(v) = params.steam_id64.as_ref().filter(|v| !v.trim().is_empty()) {
        push_filter_prefix(builder, &mut has_where);
        builder.push("steam_id64 = ").push_bind(v.trim().to_string());
    }
    if let Some(v) = params.server_id {
        push_filter_prefix(builder, &mut has_where);
        builder.push("server_id = ").push_bind(v);
    }
    if let Some(v) = params.community_id {
        push_filter_prefix(builder, &mut has_where);
        builder.push("community_id = ").push_bind(v);
    }
    if let Some(v) = params.access_method.as_ref().filter(|v| !v.trim().is_empty()) {
        push_filter_prefix(builder, &mut has_where);
        builder.push("access_method = ").push_bind(v.trim().to_string());
    }
    if let Some(v) = params.allowed {
        push_filter_prefix(builder, &mut has_where);
        builder.push("allowed = ").push_bind(v);
    }
    if let Some(v) = params.failure_code.as_ref().filter(|v| !v.trim().is_empty()) {
        push_filter_prefix(builder, &mut has_where);
        builder.push("failure_code = ").push_bind(v.trim().to_string());
    }
    if let Some(v) = params.ip_address.as_ref().map(|s| s.trim()).filter(|s| !s.is_empty()) {
        let pattern = format!("%{}%", v.replace('%', "\\%").replace('_', "\\_"));
        push_filter_prefix(builder, &mut has_where);
        builder
            .push("ip_address ILIKE ")
            .push_bind(pattern)
            .push(" ESCAPE '\\'");
    }
    if let Some(v) = params.search.as_ref().map(|s| s.trim()).filter(|s| !s.is_empty()) {
        let pattern = format!("%{}%", v.replace('%', "\\%").replace('_', "\\_"));
        push_filter_prefix(builder, &mut has_where);
        builder
            .push("(steam_id64 ILIKE ")
            .push_bind(pattern.clone())
            .push(" ESCAPE '\\' OR player_name ILIKE ")
            .push_bind(pattern.clone())
            .push(" ESCAPE '\\' OR server_name ILIKE ")
            .push_bind(pattern)
            .push(" ESCAPE '\\')");
    }
}

/// 分页查询进服记录
pub async fn query_access_logs(
    db: &Database,
    params: &AccessLogQueryParams,
) -> anyhow::Result<(Vec<PlayerAccessLog>, i64)> {
    let mut count_builder = QueryBuilder::<Postgres>::new("SELECT COUNT(*) FROM player_access_logs");
    push_filters(&mut count_builder, params);
    let (total,): (i64,) = count_builder.build_query_as().fetch_one(&db.pool).await?;

    let mut list_builder = QueryBuilder::<Postgres>::new("SELECT * FROM player_access_logs");
    push_filters(&mut list_builder, params);
    list_builder
        .push(" ORDER BY created_at DESC LIMIT ")
        .push_bind(params.page_size())
        .push(" OFFSET ")
        .push_bind(params.offset());

    let rows = list_builder
        .build_query_as::<PlayerAccessLog>()
        .fetch_all(&db.pool)
        .await?;

    Ok((rows, total))
}

/// 清理超过保留天数的进服记录
pub async fn cleanup_old_access_logs(db: &Database, retention_days: i64) -> anyhow::Result<u64> {
    if retention_days <= 0 {
        return Ok(0);
    }
    let result = sqlx::query(
        r#"DELETE FROM player_access_logs
           WHERE created_at < now() - ($1::TEXT || ' days')::INTERVAL"#,
    )
    .bind(retention_days.to_string())
    .execute(&db.pool)
    .await?;
    Ok(result.rows_affected())
}

/// 启动进服记录定时清理任务
pub fn start_access_log_cleanup_loop(db: Database, interval_secs: u64, retention_days: i64) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(interval_secs));
        loop {
            interval.tick().await;
            match cleanup_old_access_logs(&db, retention_days).await {
                Ok(0) => {}
                Ok(count) => tracing::info!(count, retention_days, "进服记录清理完成"),
                Err(e) => tracing::warn!(%e, "进服记录清理失败"),
            }
        }
    });
}

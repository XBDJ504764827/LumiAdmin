use crate::{
    db::Database,
    routes::PaginatedResponse,
    services::{ban_service, plugin_ban_service::ServerAuth, r2_storage, r2_storage::R2Storage},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const RECORD_FIELDS: &str = r#"id, idempotency_key, server_id, server_name, server_port,
    steam_id64, steam_id2, player_name, map_name, map_id, course, mode, time_type,
    teleports, run_time_seconds, threshold_seconds, replay_storage_key, replay_file_name,
    replay_file_size, replay_content_type, replay_category, status, reviewed_by, review_note,
    reviewed_at, global_submit_status, global_record_id, global_submit_error,
    global_submitted_at, ban_id, created_at, updated_at"#;

const RULE_FIELDS: &str = r#"id, map_name, course, mode, time_type, threshold_seconds,
    enabled, note, created_by, updated_by, created_at, updated_at"#;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct AbnormalRecordItem {
    pub id: Uuid,
    pub idempotency_key: String,
    pub server_id: Option<Uuid>,
    pub server_name: Option<String>,
    pub server_port: Option<i32>,
    pub steam_id64: String,
    pub steam_id2: Option<String>,
    pub player_name: Option<String>,
    pub map_name: String,
    pub map_id: Option<i32>,
    pub course: i32,
    pub mode: String,
    pub time_type: String,
    pub teleports: i32,
    pub run_time_seconds: f32,
    pub threshold_seconds: f32,
    pub replay_storage_key: Option<String>,
    pub replay_file_name: Option<String>,
    pub replay_file_size: Option<i64>,
    pub replay_content_type: Option<String>,
    pub replay_category: String,
    pub status: String,
    pub reviewed_by: Option<String>,
    pub review_note: Option<String>,
    pub reviewed_at: Option<DateTime<Utc>>,
    pub global_submit_status: String,
    pub global_record_id: Option<i32>,
    pub global_submit_error: Option<String>,
    pub global_submitted_at: Option<DateTime<Utc>>,
    pub ban_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct AbnormalRecordRuleItem {
    pub id: Uuid,
    pub map_name: String,
    pub course: i32,
    pub mode: Option<String>,
    pub time_type: Option<String>,
    pub threshold_seconds: f32,
    pub enabled: bool,
    pub note: Option<String>,
    pub created_by: Option<String>,
    pub updated_by: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct AbnormalRecordListQuery {
    pub search: Option<String>,
    pub status: Option<String>,
    pub global_submit_status: Option<String>,
    pub page: Option<i64>,
    pub page_size: Option<i64>,
}

impl AbnormalRecordListQuery {
    fn page(&self) -> i64 {
        self.page.unwrap_or(1).max(1)
    }

    fn page_size(&self) -> i64 {
        self.page_size.unwrap_or(20).clamp(1, 100)
    }

    fn offset(&self) -> i64 {
        (self.page() - 1) * self.page_size()
    }

    fn search_pattern(&self) -> Option<String> {
        self.search.as_ref().and_then(|value| {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(format!(
                    "%{}%",
                    trimmed.replace('%', "\\%").replace('_', "\\_")
                ))
            }
        })
    }
}

#[derive(Debug, Deserialize)]
pub struct RuleListQuery {
    pub search: Option<String>,
    pub enabled: Option<bool>,
    pub page: Option<i64>,
    pub page_size: Option<i64>,
}

impl RuleListQuery {
    fn page(&self) -> i64 {
        self.page.unwrap_or(1).max(1)
    }

    fn page_size(&self) -> i64 {
        self.page_size.unwrap_or(50).clamp(1, 100)
    }

    fn offset(&self) -> i64 {
        (self.page() - 1) * self.page_size()
    }

    fn search_pattern(&self) -> Option<String> {
        self.search.as_ref().and_then(|value| {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(format!(
                    "%{}%",
                    trimmed.replace('%', "\\%").replace('_', "\\_")
                ))
            }
        })
    }
}

#[derive(Debug, Deserialize)]
pub struct RuleInput {
    pub map_name: String,
    pub course: Option<i32>,
    pub mode: Option<String>,
    pub time_type: Option<String>,
    pub threshold_seconds: f32,
    pub enabled: Option<bool>,
    pub note: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreatePluginAbnormalRecordInput {
    pub report_token: String,
    pub port: i32,
    pub idempotency_key: String,
    pub steam_id64: String,
    pub steam_id2: Option<String>,
    pub player_name: Option<String>,
    pub map_name: String,
    pub map_id: Option<i32>,
    pub course: Option<i32>,
    pub mode: String,
    pub time_type: String,
    pub teleports: Option<i32>,
    pub run_time_seconds: f32,
    pub threshold_seconds: Option<f32>,
}

#[derive(Debug, Deserialize)]
pub struct ReviewInput {
    pub review_note: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RejectAndBanInput {
    pub review_note: Option<String>,
    pub reason: String,
    pub duration_minutes: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct SubmitResultInput {
    pub report_token: String,
    pub port: i32,
    pub status: String,
    pub global_record_id: Option<i32>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct PollApprovedResponse {
    pub items: Vec<AbnormalRecordItem>,
}

pub async fn list_records(
    db: &Database,
    query: &AbnormalRecordListQuery,
) -> anyhow::Result<PaginatedResponse<AbnormalRecordItem>> {
    let mut conditions = Vec::new();
    let mut param_idx = 1u32;
    let search_pattern = query.search_pattern();

    if search_pattern.is_some() {
        conditions.push(format!(
            "(steam_id64 ILIKE ${0} OR player_name ILIKE ${0} OR map_name ILIKE ${0} OR server_name ILIKE ${0})",
            param_idx
        ));
        param_idx += 1;
    }
    if query
        .status
        .as_ref()
        .is_some_and(|value| !value.trim().is_empty())
    {
        conditions.push(format!("status = ${param_idx}"));
        param_idx += 1;
    }
    if query
        .global_submit_status
        .as_ref()
        .is_some_and(|value| !value.trim().is_empty())
    {
        conditions.push(format!("global_submit_status = ${param_idx}"));
        param_idx += 1;
    }

    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };
    let count_sql = format!("SELECT COUNT(*) FROM abnormal_records {where_clause}");
    let data_sql = format!(
        r#"SELECT {RECORD_FIELDS} FROM abnormal_records {where_clause}
           ORDER BY created_at DESC LIMIT ${param_idx} OFFSET ${}"#,
        param_idx + 1
    );

    let mut count_query = sqlx::query_scalar::<_, i64>(&count_sql);
    let mut data_query = sqlx::query_as::<_, AbnormalRecordItem>(&data_sql);

    if let Some(ref pattern) = search_pattern {
        count_query = count_query.bind(pattern);
        data_query = data_query.bind(pattern);
    }
    if let Some(ref status) = query.status {
        if !status.trim().is_empty() {
            count_query = count_query.bind(status.trim());
            data_query = data_query.bind(status.trim());
        }
    }
    if let Some(ref status) = query.global_submit_status {
        if !status.trim().is_empty() {
            count_query = count_query.bind(status.trim());
            data_query = data_query.bind(status.trim());
        }
    }

    let total = count_query.fetch_one(&db.pool).await?;
    let items = data_query
        .bind(query.page_size())
        .bind(query.offset())
        .fetch_all(&db.pool)
        .await?;

    Ok(PaginatedResponse {
        items,
        total,
        page: query.page(),
        page_size: query.page_size(),
    })
}

pub async fn get_record(db: &Database, id: Uuid) -> anyhow::Result<AbnormalRecordItem> {
    sqlx::query_as::<_, AbnormalRecordItem>(&format!(
        "SELECT {RECORD_FIELDS} FROM abnormal_records WHERE id = $1"
    ))
    .bind(id)
    .fetch_one(&db.pool)
    .await
    .map_err(Into::into)
}

pub async fn list_rules(
    db: &Database,
    query: &RuleListQuery,
) -> anyhow::Result<PaginatedResponse<AbnormalRecordRuleItem>> {
    let mut conditions = Vec::new();
    let mut param_idx = 1u32;
    let search_pattern = query.search_pattern();

    if search_pattern.is_some() {
        conditions.push(format!("map_name ILIKE ${param_idx}"));
        param_idx += 1;
    }
    if query.enabled.is_some() {
        conditions.push(format!("enabled = ${param_idx}"));
        param_idx += 1;
    }

    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };
    let count_sql = format!("SELECT COUNT(*) FROM abnormal_record_rules {where_clause}");
    let data_sql = format!(
        r#"SELECT {RULE_FIELDS} FROM abnormal_record_rules {where_clause}
           ORDER BY lower(map_name), course, mode NULLS FIRST, time_type NULLS FIRST
           LIMIT ${param_idx} OFFSET ${}"#,
        param_idx + 1
    );

    let mut count_query = sqlx::query_scalar::<_, i64>(&count_sql);
    let mut data_query = sqlx::query_as::<_, AbnormalRecordRuleItem>(&data_sql);
    if let Some(ref pattern) = search_pattern {
        count_query = count_query.bind(pattern);
        data_query = data_query.bind(pattern);
    }
    if let Some(enabled) = query.enabled {
        count_query = count_query.bind(enabled);
        data_query = data_query.bind(enabled);
    }

    let total = count_query.fetch_one(&db.pool).await?;
    let items = data_query
        .bind(query.page_size())
        .bind(query.offset())
        .fetch_all(&db.pool)
        .await?;

    Ok(PaginatedResponse {
        items,
        total,
        page: query.page(),
        page_size: query.page_size(),
    })
}

pub async fn list_enabled_rules(db: &Database) -> anyhow::Result<Vec<AbnormalRecordRuleItem>> {
    sqlx::query_as::<_, AbnormalRecordRuleItem>(&format!(
        r#"SELECT {RULE_FIELDS} FROM abnormal_record_rules
           WHERE enabled = true
           ORDER BY lower(map_name), course, mode NULLS FIRST, time_type NULLS FIRST"#
    ))
    .fetch_all(&db.pool)
    .await
    .map_err(Into::into)
}

pub async fn create_rule(
    db: &Database,
    actor_name: &str,
    input: RuleInput,
) -> anyhow::Result<AbnormalRecordRuleItem> {
    let map_name = normalize_required(&input.map_name, "地图名不能为空")?;
    let mode = normalize_mode(input.mode);
    let time_type = normalize_time_type(input.time_type);
    validate_threshold(input.threshold_seconds)?;

    sqlx::query_as::<_, AbnormalRecordRuleItem>(&format!(
        r#"INSERT INTO abnormal_record_rules (
              id, map_name, course, mode, time_type, threshold_seconds, enabled, note, created_by, updated_by
           )
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $9)
           RETURNING {RULE_FIELDS}"#
    ))
    .bind(Uuid::new_v4())
    .bind(map_name)
    .bind(input.course.unwrap_or(0).max(0))
    .bind(mode)
    .bind(time_type)
    .bind(input.threshold_seconds)
    .bind(input.enabled.unwrap_or(true))
    .bind(crate::services::normalize_optional_string(input.note))
    .bind(actor_name)
    .fetch_one(&db.pool)
    .await
    .map_err(Into::into)
}

pub async fn update_rule(
    db: &Database,
    id: Uuid,
    actor_name: &str,
    input: RuleInput,
) -> anyhow::Result<AbnormalRecordRuleItem> {
    let map_name = normalize_required(&input.map_name, "地图名不能为空")?;
    let mode = normalize_mode(input.mode);
    let time_type = normalize_time_type(input.time_type);
    validate_threshold(input.threshold_seconds)?;

    let row = sqlx::query_as::<_, AbnormalRecordRuleItem>(&format!(
        r#"UPDATE abnormal_record_rules
           SET map_name = $2, course = $3, mode = $4, time_type = $5,
               threshold_seconds = $6, enabled = $7, note = $8, updated_by = $9, updated_at = now()
           WHERE id = $1
           RETURNING {RULE_FIELDS}"#
    ))
    .bind(id)
    .bind(map_name)
    .bind(input.course.unwrap_or(0).max(0))
    .bind(mode)
    .bind(time_type)
    .bind(input.threshold_seconds)
    .bind(input.enabled.unwrap_or(true))
    .bind(crate::services::normalize_optional_string(input.note))
    .bind(actor_name)
    .fetch_optional(&db.pool)
    .await?;

    row.ok_or_else(|| anyhow::anyhow!("记录不存在"))
}

pub async fn delete_rule(db: &Database, id: Uuid) -> anyhow::Result<bool> {
    let result = sqlx::query("DELETE FROM abnormal_record_rules WHERE id = $1")
        .bind(id)
        .execute(&db.pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn create_plugin_record(
    db: &Database,
    input: CreatePluginAbnormalRecordInput,
) -> anyhow::Result<AbnormalRecordItem> {
    let server = ServerAuth::authenticate(db, input.port, &input.report_token).await?;
    let idempotency_key = normalize_required(&input.idempotency_key, "幂等键不能为空")?;
    let steam_id64 = normalize_required(&input.steam_id64, "SteamID64 不能为空")?;
    let map_name = normalize_required(&input.map_name, "地图名不能为空")?;
    let mode = normalize_required(&input.mode, "模式不能为空")?.to_lowercase();
    let time_type = normalize_required(&input.time_type, "计时类型不能为空")?.to_lowercase();
    let course = input.course.unwrap_or(0).max(0);
    anyhow::ensure!(input.run_time_seconds > 0.0, "完图时间必须大于 0");

    let threshold = match input.threshold_seconds {
        Some(value) => value,
        None => match find_matching_rule(db, &map_name, course, &mode, &time_type).await? {
            Some(rule) => rule.threshold_seconds,
            None => anyhow::bail!("没有找到该地图的异常时间规则"),
        },
    };
    validate_threshold(threshold)?;

    let existing: Option<AbnormalRecordItem> = sqlx::query_as::<_, AbnormalRecordItem>(&format!(
        "SELECT {RECORD_FIELDS} FROM abnormal_records WHERE idempotency_key = $1"
    ))
    .bind(&idempotency_key)
    .fetch_optional(&db.pool)
    .await?;
    if let Some(item) = existing {
        return Ok(item);
    }

    sqlx::query_as::<_, AbnormalRecordItem>(&format!(
        r#"INSERT INTO abnormal_records (
              id, idempotency_key, server_id, server_name, server_port,
              steam_id64, steam_id2, player_name, map_name, map_id, course, mode, time_type,
              teleports, run_time_seconds, threshold_seconds
           )
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)
           RETURNING {RECORD_FIELDS}"#
    ))
    .bind(Uuid::new_v4())
    .bind(idempotency_key)
    .bind(server.id)
    .bind(server.name)
    .bind(server.port)
    .bind(steam_id64)
    .bind(crate::services::normalize_optional_string(input.steam_id2))
    .bind(crate::services::normalize_optional_string(
        input.player_name,
    ))
    .bind(map_name)
    .bind(input.map_id)
    .bind(course)
    .bind(mode)
    .bind(time_type)
    .bind(input.teleports.unwrap_or(0).max(0))
    .bind(input.run_time_seconds)
    .bind(threshold)
    .fetch_one(&db.pool)
    .await
    .map_err(Into::into)
}

pub async fn upload_replay(
    db: &Database,
    storage: &R2Storage,
    record_id: Uuid,
    report_token: &str,
    port: i32,
    file_name: &str,
    content_type: Option<&str>,
    data: Vec<u8>,
) -> anyhow::Result<AbnormalRecordItem> {
    let server = ServerAuth::authenticate(db, port, report_token).await?;
    let mut item = get_record(db, record_id).await?;
    anyhow::ensure!(item.server_id == Some(server.id), "记录不属于当前服务器");
    anyhow::ensure!(item.status == "pending", "该记录已被处理，无法上传录像");
    anyhow::ensure!(!data.is_empty(), "录像文件为空");
    anyhow::ensure!(
        r2_storage::is_allowed_file_type(file_name),
        "不支持的录像文件类型"
    );
    let file_size = data.len() as i64;

    let content_type = content_type
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| r2_storage::guess_content_type(file_name));
    let key = storage
        .upload_with_prefix("abnormal-records", record_id, file_name, content_type, data)
        .await?;

    item = sqlx::query_as::<_, AbnormalRecordItem>(&format!(
        r#"UPDATE abnormal_records
           SET replay_storage_key = $2, replay_file_name = $3, replay_file_size = $4,
               replay_content_type = $5, replay_category = $6, updated_at = now()
           WHERE id = $1
           RETURNING {RECORD_FIELDS}"#
    ))
    .bind(record_id)
    .bind(key)
    .bind(file_name)
    .bind(file_size)
    .bind(content_type)
    .bind(r2_storage::file_category(file_name))
    .fetch_one(&db.pool)
    .await?;

    Ok(item)
}

pub async fn update_replay_metadata_size(
    db: &Database,
    record_id: Uuid,
    file_size: i64,
) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE abnormal_records SET replay_file_size = $2, updated_at = now() WHERE id = $1",
    )
    .bind(record_id)
    .bind(file_size)
    .execute(&db.pool)
    .await?;
    Ok(())
}

pub async fn approve_record(
    db: &Database,
    id: Uuid,
    reviewer_name: &str,
    input: ReviewInput,
) -> anyhow::Result<AbnormalRecordItem> {
    let row = sqlx::query_as::<_, AbnormalRecordItem>(&format!(
        r#"UPDATE abnormal_records
           SET status = 'approved',
               reviewed_by = $2,
               review_note = $3,
               reviewed_at = now(),
               global_submit_status = CASE
                 WHEN global_submit_status IN ('not_submitted', 'failed', 'discarded') THEN 'queued'
                 ELSE global_submit_status
               END,
               updated_at = now()
           WHERE id = $1 AND status = 'pending'
           RETURNING {RECORD_FIELDS}"#
    ))
    .bind(id)
    .bind(reviewer_name)
    .bind(crate::services::normalize_optional_string(
        input.review_note,
    ))
    .fetch_optional(&db.pool)
    .await?;

    row.ok_or_else(|| anyhow::anyhow!("记录不存在或已处理"))
}

pub async fn reject_record(
    db: &Database,
    id: Uuid,
    reviewer_name: &str,
    input: ReviewInput,
) -> anyhow::Result<AbnormalRecordItem> {
    let row = sqlx::query_as::<_, AbnormalRecordItem>(&format!(
        r#"UPDATE abnormal_records
           SET status = 'rejected',
               reviewed_by = $2,
               review_note = $3,
               reviewed_at = now(),
               global_submit_status = 'discarded',
               updated_at = now()
           WHERE id = $1 AND status = 'pending'
           RETURNING {RECORD_FIELDS}"#
    ))
    .bind(id)
    .bind(reviewer_name)
    .bind(crate::services::normalize_optional_string(
        input.review_note,
    ))
    .fetch_optional(&db.pool)
    .await?;

    row.ok_or_else(|| anyhow::anyhow!("记录不存在或已处理"))
}

pub async fn reject_and_ban(
    db: &Database,
    config: &crate::config::Config,
    id: Uuid,
    reviewer_name: &str,
    input: RejectAndBanInput,
) -> anyhow::Result<(AbnormalRecordItem, ban_service::BanItem)> {
    let item = get_record(db, id).await?;
    anyhow::ensure!(item.status == "pending", "记录不存在或已处理");

    let mut ban = ban_service::create_ban(
        db,
        config,
        ban_service::CreateBanInput {
            player: item.player_name.clone(),
            steam_id: item.steam_id64.clone(),
            ip_address: None,
            ban_type: "steam".to_string(),
            reason: input.reason,
            duration_minutes: input.duration_minutes.unwrap_or(0),
            expires_at: None,
            operator_name: reviewer_name.to_string(),
        },
    )
    .await?;
    sqlx::query("UPDATE ban_records SET source = 'abnormal_record' WHERE id = $1")
        .bind(ban.id)
        .execute(&db.pool)
        .await?;
    ban.source = "abnormal_record".to_string();

    let row = sqlx::query_as::<_, AbnormalRecordItem>(&format!(
        r#"UPDATE abnormal_records
           SET status = 'rejected',
               reviewed_by = $2,
               review_note = $3,
               reviewed_at = now(),
               global_submit_status = 'discarded',
               ban_id = $4,
               updated_at = now()
           WHERE id = $1 AND status = 'pending'
           RETURNING {RECORD_FIELDS}"#
    ))
    .bind(id)
    .bind(reviewer_name)
    .bind(crate::services::normalize_optional_string(
        input.review_note,
    ))
    .bind(ban.id)
    .fetch_optional(&db.pool)
    .await?;

    let item = row.ok_or_else(|| anyhow::anyhow!("记录不存在或已处理"))?;
    Ok((item, ban))
}

pub async fn retry_submit(
    db: &Database,
    id: Uuid,
    reviewer_name: &str,
) -> anyhow::Result<AbnormalRecordItem> {
    let row = sqlx::query_as::<_, AbnormalRecordItem>(&format!(
        r#"UPDATE abnormal_records
           SET status = CASE WHEN status = 'approved_but_submit_failed' THEN 'approved' ELSE status END,
               global_submit_status = 'queued',
               global_submit_error = NULL,
               reviewed_by = COALESCE(reviewed_by, $2),
               updated_at = now()
           WHERE id = $1 AND status IN ('approved', 'approved_but_submit_failed')
           RETURNING {RECORD_FIELDS}"#
    ))
    .bind(id)
    .bind(reviewer_name)
    .fetch_optional(&db.pool)
    .await?;

    row.ok_or_else(|| anyhow::anyhow!("该记录当前状态不可重试"))
}

pub async fn poll_approved(
    db: &Database,
    report_token: &str,
    port: i32,
    limit: i64,
) -> anyhow::Result<PollApprovedResponse> {
    let server = ServerAuth::authenticate(db, port, report_token).await?;
    let limit = limit.clamp(1, 20);
    let items = sqlx::query_as::<_, AbnormalRecordItem>(&format!(
        r#"UPDATE abnormal_records
           SET global_submit_status = 'submitting', updated_at = now()
           WHERE id IN (
             SELECT id FROM abnormal_records
             WHERE server_id = $1
               AND status = 'approved'
               AND global_submit_status IN ('queued', 'failed')
             ORDER BY reviewed_at ASC NULLS LAST, created_at ASC
             LIMIT $2
             FOR UPDATE SKIP LOCKED
           )
           RETURNING {RECORD_FIELDS}"#
    ))
    .bind(server.id)
    .bind(limit)
    .fetch_all(&db.pool)
    .await?;

    Ok(PollApprovedResponse { items })
}

pub async fn submit_result(
    db: &Database,
    id: Uuid,
    input: SubmitResultInput,
) -> anyhow::Result<AbnormalRecordItem> {
    let server = ServerAuth::authenticate(db, input.port, &input.report_token).await?;
    let status = input.status.trim();
    anyhow::ensure!(matches!(status, "submitted" | "failed"), "提交状态无效");
    let item = get_record(db, id).await?;
    anyhow::ensure!(item.server_id == Some(server.id), "记录不属于当前服务器");

    let row = if status == "submitted" {
        sqlx::query_as::<_, AbnormalRecordItem>(&format!(
            r#"UPDATE abnormal_records
               SET global_submit_status = 'submitted',
                   global_record_id = $2,
                   global_submit_error = NULL,
                   global_submitted_at = now(),
                   updated_at = now()
               WHERE id = $1
               RETURNING {RECORD_FIELDS}"#
        ))
        .bind(id)
        .bind(input.global_record_id)
        .fetch_one(&db.pool)
        .await?
    } else {
        sqlx::query_as::<_, AbnormalRecordItem>(&format!(
            r#"UPDATE abnormal_records
               SET status = 'approved_but_submit_failed',
                   global_submit_status = 'failed',
                   global_submit_error = $2,
                   updated_at = now()
               WHERE id = $1
               RETURNING {RECORD_FIELDS}"#
        ))
        .bind(id)
        .bind(crate::services::normalize_optional_string(input.error))
        .fetch_one(&db.pool)
        .await?
    };

    Ok(row)
}

pub async fn replay_url(
    db: &Database,
    storage: Option<&R2Storage>,
    id: Uuid,
) -> anyhow::Result<Option<String>> {
    let item = get_record(db, id).await?;
    let Some(key) = item.replay_storage_key else {
        return Ok(None);
    };
    Ok(storage.map(|r2| r2.presigned_url(&key, 3600)))
}

async fn find_matching_rule(
    db: &Database,
    map_name: &str,
    course: i32,
    mode: &str,
    time_type: &str,
) -> anyhow::Result<Option<AbnormalRecordRuleItem>> {
    sqlx::query_as::<_, AbnormalRecordRuleItem>(&format!(
        r#"SELECT {RULE_FIELDS}
           FROM abnormal_record_rules
           WHERE enabled = true
             AND lower(map_name) = lower($1)
             AND (course = $2 OR course = 0)
             AND (mode IS NULL OR lower(mode) = lower($3))
             AND (time_type IS NULL OR lower(time_type) = lower($4))
           ORDER BY
             CASE WHEN course = $2 THEN 0 ELSE 1 END,
             CASE WHEN mode IS NOT NULL THEN 0 ELSE 1 END,
             CASE WHEN time_type IS NOT NULL THEN 0 ELSE 1 END
           LIMIT 1"#
    ))
    .bind(map_name)
    .bind(course)
    .bind(mode)
    .bind(time_type)
    .fetch_optional(&db.pool)
    .await
    .map_err(Into::into)
}

fn normalize_required(value: &str, message: &str) -> anyhow::Result<String> {
    let trimmed = value.trim();
    anyhow::ensure!(!trimmed.is_empty(), "{message}");
    Ok(trimmed.to_string())
}

fn normalize_mode(value: Option<String>) -> Option<String> {
    crate::services::normalize_optional_string(value).map(|value| value.to_lowercase())
}

fn normalize_time_type(value: Option<String>) -> Option<String> {
    crate::services::normalize_optional_string(value).map(|value| value.to_lowercase())
}

fn validate_threshold(value: f32) -> anyhow::Result<()> {
    anyhow::ensure!(value.is_finite(), "异常阈值必须是有效数字");
    anyhow::ensure!(value > 0.0, "异常阈值必须大于 0 秒");
    anyhow::ensure!(value <= 86_400.0, "异常阈值不能超过 24 小时");
    Ok(())
}

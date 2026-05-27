use crate::{
    config::Config, db::Database, routes::ListQuery, services::steam_service::SteamResolver,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

#[derive(Serialize)]
pub struct PlayerReportItem {
    pub id: Uuid,
    pub target_steam_id: String,
    pub target_player_name: Option<String>,
    pub reporter_contact: Option<String>,
    pub report_reason: String,
    pub status: String,
    pub reviewed_by: Option<String>,
    pub review_note: Option<String>,
    pub reviewed_at: Option<String>,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upload_token: Option<String>,
}

#[derive(sqlx::FromRow)]
struct PlayerReportRow {
    id: Uuid,
    target_steam_id: String,
    target_player_name: Option<String>,
    reporter_contact: Option<String>,
    report_reason: String,
    status: String,
    reviewed_by: Option<String>,
    review_note: Option<String>,
    reviewed_at: Option<chrono::DateTime<chrono::Utc>>,
    created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Deserialize)]
pub struct CreatePlayerReportInput {
    pub steam_input: String,
    pub target_player_name: Option<String>,
    pub reporter_contact: Option<String>,
    pub report_reason: String,
}

#[derive(Deserialize)]
pub struct ReviewPlayerReportInput {
    pub status: String,
    pub review_note: Option<String>,
}

const REPORT_FIELDS: &str = "id, target_steam_id, target_player_name, reporter_contact, report_reason, status, reviewed_by, review_note, reviewed_at, created_at";

fn row_to_item(row: PlayerReportRow, upload_token: Option<String>) -> PlayerReportItem {
    PlayerReportItem {
        id: row.id,
        target_steam_id: row.target_steam_id,
        target_player_name: row.target_player_name,
        reporter_contact: row.reporter_contact,
        report_reason: row.report_reason,
        status: row.status,
        reviewed_by: row.reviewed_by,
        review_note: row.review_note,
        reviewed_at: row.reviewed_at.map(|value| value.to_rfc3339()),
        created_at: row.created_at.to_rfc3339(),
        upload_token,
    }
}

pub async fn create_report(
    db: &Database,
    config: &Config,
    input: CreatePlayerReportInput,
) -> anyhow::Result<PlayerReportItem> {
    let steam_input = input.steam_input.trim();
    let reason = input.report_reason.trim();
    anyhow::ensure!(!steam_input.is_empty(), "Steam 标识符不能为空");
    anyhow::ensure!(!reason.is_empty(), "举报理由不能为空");

    let target_steam_id = SteamResolver::new(config)
        .resolve(steam_input)
        .await?
        .steamid64;
    let upload_token = new_upload_token();
    let upload_token_hash = hash_upload_token(&upload_token);

    let row = sqlx::query_as::<_, PlayerReportRow>(&format!(
        r#"INSERT INTO player_reports (
               id, target_steam_id, target_player_name, reporter_contact, report_reason, upload_token_hash
           )
           VALUES ($1, $2, $3, $4, $5, $6)
           RETURNING {REPORT_FIELDS}"#
    ))
    .bind(Uuid::new_v4())
    .bind(target_steam_id)
    .bind(super::normalize_optional_string(input.target_player_name))
    .bind(super::normalize_optional_string(input.reporter_contact))
    .bind(reason)
    .bind(upload_token_hash)
    .fetch_one(&db.pool)
    .await?;

    Ok(row_to_item(row, Some(upload_token)))
}

pub async fn list_reports(
    db: &Database,
    query: &ListQuery,
) -> anyhow::Result<crate::routes::PaginatedResponse<PlayerReportItem>> {
    let mut conditions = Vec::new();
    let mut param_idx = 1u32;
    let search_pattern = query.search_pattern();

    if search_pattern.is_some() {
        conditions.push(format!(
            "(target_steam_id ILIKE ${param_idx} OR target_player_name ILIKE ${param_idx})"
        ));
        param_idx += 1;
    }
    if let Some(ref status) = query.status {
        if !status.trim().is_empty() {
            conditions.push(format!("status = ${param_idx}"));
            param_idx += 1;
        }
    }

    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };

    let count_sql = format!("SELECT COUNT(*) FROM player_reports {where_clause}");
    let data_sql = format!(
        "SELECT {REPORT_FIELDS} FROM player_reports {where_clause} ORDER BY created_at DESC LIMIT ${param_idx} OFFSET ${}",
        param_idx + 1
    );

    let mut count_query = sqlx::query_scalar::<_, i64>(&count_sql);
    let mut data_query = sqlx::query_as::<_, PlayerReportRow>(&data_sql);

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
    data_query = data_query.bind(query.page_size()).bind(query.offset());

    let total = count_query.fetch_one(&db.pool).await?;
    let items = data_query
        .fetch_all(&db.pool)
        .await?
        .into_iter()
        .map(|row| row_to_item(row, None))
        .collect();

    Ok(crate::routes::PaginatedResponse {
        items,
        total,
        page: query.page(),
        page_size: query.page_size(),
    })
}

pub async fn review_report(
    db: &Database,
    id: Uuid,
    reviewer_name: &str,
    input: ReviewPlayerReportInput,
) -> anyhow::Result<PlayerReportItem> {
    let status = input.status.trim();
    anyhow::ensure!(matches!(status, "approved" | "rejected"), "审核状态无效");

    let row = sqlx::query_as::<_, PlayerReportRow>(&format!(
        r#"UPDATE player_reports
           SET status = $2, reviewed_by = $3, review_note = $4, reviewed_at = now()
           WHERE id = $1
           RETURNING {REPORT_FIELDS}"#
    ))
    .bind(id)
    .bind(status)
    .bind(reviewer_name)
    .bind(super::normalize_optional_string(input.review_note))
    .fetch_one(&db.pool)
    .await?;

    Ok(row_to_item(row, None))
}

pub async fn find_upload_token_hash(db: &Database, id: Uuid) -> anyhow::Result<(String, Option<String>)> {
    let row = sqlx::query_as("SELECT status, upload_token_hash FROM player_reports WHERE id = $1")
        .bind(id)
        .fetch_one(&db.pool)
        .await?;
    Ok(row)
}

pub fn verify_upload_token(stored_hash: Option<&str>, token: &str) -> bool {
    let Some(stored_hash) = stored_hash else {
        return false;
    };
    !token.trim().is_empty() && hash_upload_token(token.trim()) == stored_hash
}

fn new_upload_token() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

fn hash_upload_token(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

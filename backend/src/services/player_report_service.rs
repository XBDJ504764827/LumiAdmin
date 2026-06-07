use crate::{
    config::Config,
    db::Database,
    routes::ListQuery,
    services::{
        ban_service::{self, BanItem, BanRow},
        steam_service::SteamResolver,
    },
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

#[derive(Deserialize)]
pub struct BanPlayerReportInput {
    pub player: Option<String>,
    pub steam_id: Option<String>,
    pub ip_address: Option<String>,
    pub ban_type: String,
    pub reason: String,
}

#[derive(Serialize)]
pub struct BanPlayerReportResult {
    pub ban: BanItem,
    pub report: PlayerReportItem,
    pub copied_files: i64,
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
        r#"SELECT pr.id, pr.target_steam_id, pr.target_player_name, pr.reporter_contact,
                  pr.report_reason, pr.status, COALESCE(reviewer_user.display_name, pr.reviewed_by) AS reviewed_by,
                  pr.review_note, pr.reviewed_at, pr.created_at
           FROM player_reports pr
           LEFT JOIN LATERAL (
             SELECT COALESCE(NULLIF(u.remark, ''), u.username) AS display_name
             FROM users u
             WHERE u.username = pr.reviewed_by
                OR u.display_name = pr.reviewed_by
                OR NULLIF(u.remark, '') = pr.reviewed_by
             ORDER BY CASE WHEN u.username = pr.reviewed_by THEN 0 WHEN u.display_name = pr.reviewed_by THEN 1 ELSE 2 END
             LIMIT 1
           ) reviewer_user ON true
           {where_clause} ORDER BY pr.created_at DESC LIMIT ${param_idx} OFFSET ${}"#,
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
           WHERE id = $1 AND status = 'pending'
           RETURNING {REPORT_FIELDS}"#
    ))
    .bind(id)
    .bind(status)
    .bind(reviewer_name)
    .bind(super::normalize_optional_string(input.review_note))
    .fetch_optional(&db.pool)
    .await?;

    let Some(row) = row else {
        let existing: Option<(String,)> =
            sqlx::query_as("SELECT status FROM player_reports WHERE id = $1")
                .bind(id)
                .fetch_optional(&db.pool)
                .await?;
        if existing.is_some() {
            anyhow::bail!("该举报已被处理");
        }
        anyhow::bail!("记录不存在");
    };

    Ok(row_to_item(row, None))
}

pub async fn ban_report(
    db: &Database,
    config: &Config,
    report_id: Uuid,
    reviewer_name: &str,
    input: BanPlayerReportInput,
) -> anyhow::Result<BanPlayerReportResult> {
    let ban_type = input.ban_type.trim();
    let reason = input.reason.trim();
    anyhow::ensure!(matches!(ban_type, "steam" | "ip"), "封禁属性无效");
    anyhow::ensure!(!reason.is_empty(), "封禁理由不能为空");
    let resolved_input_steam_id = match input
        .steam_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(value) => Some(SteamResolver::new(config).resolve(value).await?.steamid64),
        None => None,
    };

    let mut tx = db.pool.begin().await?;

    let report_row = sqlx::query_as::<_, PlayerReportRow>(&format!(
        "SELECT {REPORT_FIELDS} FROM player_reports WHERE id = $1 FOR UPDATE"
    ))
    .bind(report_id)
    .fetch_one(&mut *tx)
    .await?;

    anyhow::ensure!(report_row.status == "pending", "该举报已被处理");

    let steam_id = resolved_input_steam_id.unwrap_or_else(|| report_row.target_steam_id.clone());

    let existing: Option<(Uuid,)> = sqlx::query_as(
        r#"SELECT id FROM ban_records WHERE steam_id = $1 AND status = 'active' FOR UPDATE"#,
    )
    .bind(&steam_id)
    .fetch_optional(&mut *tx)
    .await?;
    anyhow::ensure!(
        existing.is_none(),
        "该玩家已有活跃封禁记录，请先解封后再重新封禁"
    );

    let player = super::normalize_optional_string(input.player)
        .or_else(|| report_row.target_player_name.clone());
    let row = sqlx::query_as::<_, BanRow>(
        r#"INSERT INTO ban_records (
               id, player, steam_id, ip_address, server_name, ban_type,
               duration_minutes, expires_at, reason, status, operator_name, source,
               server_id, server_port
           )
           VALUES ($1, $2, $3, $4, NULL, $5, 0, NULL, $6, 'active', $7, 'manual', NULL, NULL)
           RETURNING id, player, steam_id, ip_address, server_name, ban_type,
                     duration_minutes, expires_at, reason, status, operator_name, source,
                     server_id, server_port, removed_reason, removed_by, removed_at, created_at"#,
    )
    .bind(Uuid::new_v4())
    .bind(player)
    .bind(&steam_id)
    .bind(super::normalize_optional_string(input.ip_address))
    .bind(ban_type)
    .bind(reason)
    .bind(reviewer_name)
    .fetch_one(&mut *tx)
    .await?;
    let ban = ban_service::row_to_item(row);

    let copied_files = sqlx::query(
        r#"INSERT INTO ban_files (
               id, ban_id, file_name, file_size, content_type, storage_key, category, uploaded_by, uploaded_at
           )
           SELECT gen_random_uuid(), $1, file_name, file_size, content_type, storage_key, category, $2, uploaded_at
           FROM player_report_files
           WHERE report_id = $3"#,
    )
    .bind(ban.id)
    .bind(reviewer_name)
    .bind(report_id)
    .execute(&mut *tx)
    .await?
    .rows_affected() as i64;

    let reviewed_row = sqlx::query_as::<_, PlayerReportRow>(&format!(
        r#"UPDATE player_reports
           SET status = 'approved', reviewed_by = $2, review_note = $3, reviewed_at = now(), upload_token_hash = NULL
           WHERE id = $1 AND status = 'pending'
           RETURNING {REPORT_FIELDS}"#
    ))
    .bind(report_id)
    .bind(reviewer_name)
    .bind(format!("已根据玩家举报创建封禁记录：{}", ban.id))
    .fetch_one(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(BanPlayerReportResult {
        ban,
        report: row_to_item(reviewed_row, None),
        copied_files,
    })
}

pub async fn find_upload_token_hash(
    db: &Database,
    id: Uuid,
) -> anyhow::Result<(String, Option<String>)> {
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

/// 按 SteamID 查询玩家的举报记录（公开接口，供玩家查看举报状态和管理员审核结果）
pub async fn query_reports_by_target_steam_id(
    db: &Database,
    steam_id: &str,
) -> anyhow::Result<Vec<PlayerReportItem>> {
    let steam_id = steam_id.trim();
    anyhow::ensure!(!steam_id.is_empty(), "SteamID 不能为空");

    let rows = sqlx::query_as::<_, PlayerReportRow>(
        r#"SELECT id, target_steam_id, target_player_name, reporter_contact, report_reason,
                  status, reviewed_by, review_note, reviewed_at, created_at
           FROM player_reports
           WHERE target_steam_id = $1
           ORDER BY created_at DESC"#,
    )
    .bind(steam_id)
    .fetch_all(&db.pool)
    .await?;

    // upload_token is deliberately None — public queries must not expose the token
    Ok(rows
        .into_iter()
        .map(|row| row_to_item(row, None))
        .collect())
}

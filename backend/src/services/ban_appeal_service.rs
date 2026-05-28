use crate::db::Database;
use crate::routes::{ListQuery, PaginatedResponse};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

const APPEAL_FIELDS: &str = "id, ban_id, steam_id, player_name, appeal_reason, status, reviewed_by, review_note, reviewed_at, created_at";

#[derive(Serialize)]
pub struct BanAppealItem {
    pub id: Uuid,
    pub ban_id: Uuid,
    pub steam_id: String,
    pub player_name: String,
    pub appeal_reason: String,
    pub status: String,
    pub reviewed_by: Option<String>,
    pub review_note: Option<String>,
    pub reviewed_at: Option<String>,
    pub created_at: String,
    // 关联的封禁信息
    pub ban_reason: Option<String>,
    pub ban_type: Option<String>,
    pub ban_operator_name: Option<String>,
    pub ban_server_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upload_token: Option<String>,
}

#[derive(sqlx::FromRow)]
struct AppealRow {
    id: Uuid,
    ban_id: Uuid,
    steam_id: String,
    player_name: String,
    appeal_reason: String,
    status: String,
    reviewed_by: Option<String>,
    review_note: Option<String>,
    reviewed_at: Option<chrono::DateTime<chrono::Utc>>,
    created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(sqlx::FromRow)]
struct AppealWithBanRow {
    id: Uuid,
    ban_id: Uuid,
    steam_id: String,
    player_name: String,
    appeal_reason: String,
    status: String,
    reviewed_by: Option<String>,
    review_note: Option<String>,
    reviewed_at: Option<chrono::DateTime<chrono::Utc>>,
    created_at: chrono::DateTime<chrono::Utc>,
    ban_reason: Option<String>,
    ban_type: Option<String>,
    ban_operator_name: Option<String>,
    ban_server_name: Option<String>,
}

#[derive(Deserialize)]
pub struct CreateAppealInput {
    pub ban_id: Uuid,
    pub steam_id: String,
    pub player_name: String,
    pub appeal_reason: String,
}

pub async fn create_appeal(
    db: &Database,
    input: CreateAppealInput,
) -> anyhow::Result<BanAppealItem> {
    let steam_id = input.steam_id.trim();
    let player_name = input.player_name.trim();
    let reason = input.appeal_reason.trim();
    let upload_token = new_upload_token();
    let upload_token_hash = hash_upload_token(&upload_token);

    anyhow::ensure!(!steam_id.is_empty(), "SteamID 不能为空");
    anyhow::ensure!(!player_name.is_empty(), "玩家名称不能为空");
    anyhow::ensure!(!reason.is_empty(), "申诉理由不能为空");

    // 检查封禁记录是否存在、仍活跃，并且确实属于提交申诉的 SteamID。
    let ban_record: Option<(String, String)> =
        sqlx::query_as("SELECT status, steam_id FROM ban_records WHERE id = $1")
            .bind(input.ban_id)
            .fetch_optional(&db.pool)
            .await?;

    let (status, banned_steam_id) = ban_record.ok_or_else(|| anyhow::anyhow!("封禁记录不存在"))?;
    anyhow::ensure!(status == "active", "该封禁记录已非活跃状态，无需申诉");
    anyhow::ensure!(
        banned_steam_id.trim() == steam_id,
        "该封禁记录不属于当前 SteamID，无法提交申诉"
    );

    // 检查该封禁是否已有待审核的申诉
    let existing: Option<(Uuid,)> =
        sqlx::query_as("SELECT id FROM ban_appeals WHERE ban_id = $1 AND status = 'pending'")
            .bind(input.ban_id)
            .fetch_optional(&db.pool)
            .await?;
    anyhow::ensure!(existing.is_none(), "该封禁记录已有待审核的申诉");

    let row = sqlx::query_as::<_, AppealRow>(&format!(
        r#"INSERT INTO ban_appeals (id, ban_id, steam_id, player_name, appeal_reason, upload_token_hash)
               VALUES ($1, $2, $3, $4, $5, $6)
               RETURNING {APPEAL_FIELDS}"#
    ))
    .bind(Uuid::new_v4())
    .bind(input.ban_id)
    .bind(steam_id)
    .bind(player_name)
    .bind(reason)
    .bind(upload_token_hash)
    .fetch_one(&db.pool)
    .await?;

    let ban_info: Option<(String, String, String, Option<String>)> = sqlx::query_as(
        r#"SELECT br.reason, br.ban_type, COALESCE(operator_user.display_name, br.operator_name) AS operator_name, br.server_name
           FROM ban_records br
           LEFT JOIN LATERAL (
             SELECT COALESCE(NULLIF(u.remark, ''), u.username) AS display_name
             FROM users u
             WHERE u.username = br.operator_name
                OR u.display_name = br.operator_name
                OR NULLIF(u.remark, '') = br.operator_name
             ORDER BY CASE WHEN u.username = br.operator_name THEN 0 WHEN u.display_name = br.operator_name THEN 1 ELSE 2 END
             LIMIT 1
           ) operator_user ON true
           WHERE br.id = $1"#,
    )
    .bind(input.ban_id)
    .fetch_optional(&db.pool)
    .await?;

    Ok(BanAppealItem {
        id: row.id,
        ban_id: row.ban_id,
        steam_id: row.steam_id,
        player_name: row.player_name,
        appeal_reason: row.appeal_reason,
        status: row.status,
        reviewed_by: row.reviewed_by,
        review_note: row.review_note,
        reviewed_at: row.reviewed_at.map(|v| v.to_rfc3339()),
        created_at: row.created_at.to_rfc3339(),
        ban_reason: ban_info.as_ref().map(|b| b.0.clone()),
        ban_type: ban_info.as_ref().map(|b| b.1.clone()),
        ban_operator_name: ban_info.as_ref().map(|b| b.2.clone()),
        ban_server_name: ban_info.as_ref().and_then(|b| b.3.clone()),
        upload_token: Some(upload_token),
    })
}

pub async fn list_appeals(
    db: &Database,
    query: &ListQuery,
) -> anyhow::Result<PaginatedResponse<BanAppealItem>> {
    let mut conditions = Vec::new();
    let mut param_idx = 1u32;
    let search_pattern = query.search_pattern();

    if search_pattern.is_some() {
        conditions.push(format!(
            "(ba.steam_id ILIKE ${param_idx} OR ba.player_name ILIKE ${param_idx})"
        ));
        param_idx += 1;
    }
    if let Some(ref status) = query.status {
        if !status.trim().is_empty() {
            conditions.push(format!("ba.status = ${param_idx}"));
            param_idx += 1;
        }
    }

    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };

    let count_sql = format!("SELECT COUNT(*) FROM ban_appeals ba {where_clause}");
    let data_sql = format!(
        r#"SELECT ba.id, ba.ban_id, ba.steam_id, ba.player_name, ba.appeal_reason,
                  ba.status, COALESCE(reviewer_user.display_name, ba.reviewed_by) AS reviewed_by, ba.review_note, ba.reviewed_at, ba.created_at,
                  br.reason AS ban_reason, br.ban_type, COALESCE(ban_operator_user.display_name, br.operator_name) AS ban_operator_name, br.server_name AS ban_server_name
           FROM ban_appeals ba
           LEFT JOIN ban_records br ON ba.ban_id = br.id
           LEFT JOIN LATERAL (
             SELECT COALESCE(NULLIF(u.remark, ''), u.username) AS display_name
             FROM users u
             WHERE u.username = ba.reviewed_by
                OR u.display_name = ba.reviewed_by
                OR NULLIF(u.remark, '') = ba.reviewed_by
             ORDER BY CASE WHEN u.username = ba.reviewed_by THEN 0 WHEN u.display_name = ba.reviewed_by THEN 1 ELSE 2 END
             LIMIT 1
           ) reviewer_user ON true
           LEFT JOIN LATERAL (
             SELECT COALESCE(NULLIF(u.remark, ''), u.username) AS display_name
             FROM users u
             WHERE u.username = br.operator_name
                OR u.display_name = br.operator_name
                OR NULLIF(u.remark, '') = br.operator_name
             ORDER BY CASE WHEN u.username = br.operator_name THEN 0 WHEN u.display_name = br.operator_name THEN 1 ELSE 2 END
             LIMIT 1
           ) ban_operator_user ON true
           {where_clause}
           ORDER BY ba.created_at DESC
           LIMIT ${param_idx} OFFSET ${}"#,
        param_idx + 1
    );

    let mut count_query = sqlx::query_scalar::<_, i64>(&count_sql);
    let mut data_query = sqlx::query_as::<_, AppealWithBanRow>(&data_sql);

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
    let rows = data_query.fetch_all(&db.pool).await?;

    let items = rows
        .into_iter()
        .map(|row| BanAppealItem {
            id: row.id,
            ban_id: row.ban_id,
            steam_id: row.steam_id,
            player_name: row.player_name,
            appeal_reason: row.appeal_reason,
            status: row.status,
            reviewed_by: row.reviewed_by,
            review_note: row.review_note,
            reviewed_at: row.reviewed_at.map(|v| v.to_rfc3339()),
            created_at: row.created_at.to_rfc3339(),
            ban_reason: row.ban_reason,
            ban_type: row.ban_type,
            ban_operator_name: row.ban_operator_name,
            ban_server_name: row.ban_server_name,
            upload_token: None,
        })
        .collect();

    Ok(PaginatedResponse {
        items,
        total,
        page: query.page(),
        page_size: query.page_size(),
    })
}

pub async fn approve_appeal(
    db: &Database,
    appeal_id: Uuid,
    reviewer_name: &str,
    review_note: Option<String>,
) -> anyhow::Result<BanAppealItem> {
    // 获取申诉记录
    let row = sqlx::query_as::<_, AppealRow>(&format!(
        "SELECT {APPEAL_FIELDS} FROM ban_appeals WHERE id = $1"
    ))
    .bind(appeal_id)
    .fetch_optional(&db.pool)
    .await?
    .ok_or_else(|| anyhow::anyhow!("申诉记录不存在"))?;

    anyhow::ensure!(row.status == "pending", "该申诉已被处理");

    // 更新申诉状态
    sqlx::query(
        r#"UPDATE ban_appeals SET status = 'approved', reviewed_by = $2, review_note = $3, reviewed_at = now()
           WHERE id = $1"#,
    )
    .bind(appeal_id)
    .bind(reviewer_name)
    .bind(review_note)
    .execute(&db.pool)
    .await?;

    // 解除封禁
    super::ban_service::unban(db, row.ban_id, reviewer_name).await?;

    // 返回更新后的记录
    let ban_info: Option<(String, String, String, Option<String>)> = sqlx::query_as(
        r#"SELECT br.reason, br.ban_type, COALESCE(operator_user.display_name, br.operator_name) AS operator_name, br.server_name
           FROM ban_records br
           LEFT JOIN LATERAL (
             SELECT COALESCE(NULLIF(u.remark, ''), u.username) AS display_name
             FROM users u
             WHERE u.username = br.operator_name
                OR u.display_name = br.operator_name
                OR NULLIF(u.remark, '') = br.operator_name
             ORDER BY CASE WHEN u.username = br.operator_name THEN 0 WHEN u.display_name = br.operator_name THEN 1 ELSE 2 END
             LIMIT 1
           ) operator_user ON true
           WHERE br.id = $1"#,
    )
    .bind(row.ban_id)
    .fetch_optional(&db.pool)
    .await?;

    let updated: AppealRow = sqlx::query_as(&format!(
        "SELECT {APPEAL_FIELDS} FROM ban_appeals WHERE id = $1"
    ))
    .bind(appeal_id)
    .fetch_one(&db.pool)
    .await?;

    Ok(BanAppealItem {
        id: updated.id,
        ban_id: updated.ban_id,
        steam_id: updated.steam_id,
        player_name: updated.player_name,
        appeal_reason: updated.appeal_reason,
        status: updated.status,
        reviewed_by: updated.reviewed_by,
        review_note: updated.review_note,
        reviewed_at: updated.reviewed_at.map(|v| v.to_rfc3339()),
        created_at: updated.created_at.to_rfc3339(),
        ban_reason: ban_info.as_ref().map(|b| b.0.clone()),
        ban_type: ban_info.as_ref().map(|b| b.1.clone()),
        ban_operator_name: ban_info.as_ref().map(|b| b.2.clone()),
        ban_server_name: ban_info.as_ref().and_then(|b| b.3.clone()),
        upload_token: None,
    })
}

pub async fn reject_appeal(
    db: &Database,
    appeal_id: Uuid,
    reviewer_name: &str,
    review_note: Option<String>,
) -> anyhow::Result<BanAppealItem> {
    let row = sqlx::query_as::<_, AppealRow>(&format!(
        "SELECT {APPEAL_FIELDS} FROM ban_appeals WHERE id = $1"
    ))
    .bind(appeal_id)
    .fetch_optional(&db.pool)
    .await?
    .ok_or_else(|| anyhow::anyhow!("申诉记录不存在"))?;

    anyhow::ensure!(row.status == "pending", "该申诉已被处理");

    sqlx::query(
        r#"UPDATE ban_appeals SET status = 'rejected', reviewed_by = $2, review_note = $3, reviewed_at = now()
           WHERE id = $1"#,
    )
    .bind(appeal_id)
    .bind(reviewer_name)
    .bind(review_note)
    .execute(&db.pool)
    .await?;

    let ban_info: Option<(String, String, String, Option<String>)> = sqlx::query_as(
        r#"SELECT br.reason, br.ban_type, COALESCE(operator_user.display_name, br.operator_name) AS operator_name, br.server_name
           FROM ban_records br
           LEFT JOIN LATERAL (
             SELECT COALESCE(NULLIF(u.remark, ''), u.username) AS display_name
             FROM users u
             WHERE u.username = br.operator_name
                OR u.display_name = br.operator_name
                OR NULLIF(u.remark, '') = br.operator_name
             ORDER BY CASE WHEN u.username = br.operator_name THEN 0 WHEN u.display_name = br.operator_name THEN 1 ELSE 2 END
             LIMIT 1
           ) operator_user ON true
           WHERE br.id = $1"#,
    )
    .bind(row.ban_id)
    .fetch_optional(&db.pool)
    .await?;

    let updated: AppealRow = sqlx::query_as(&format!(
        "SELECT {APPEAL_FIELDS} FROM ban_appeals WHERE id = $1"
    ))
    .bind(appeal_id)
    .fetch_one(&db.pool)
    .await?;

    Ok(BanAppealItem {
        id: updated.id,
        ban_id: updated.ban_id,
        steam_id: updated.steam_id,
        player_name: updated.player_name,
        appeal_reason: updated.appeal_reason,
        status: updated.status,
        reviewed_by: updated.reviewed_by,
        review_note: updated.review_note,
        reviewed_at: updated.reviewed_at.map(|v| v.to_rfc3339()),
        created_at: updated.created_at.to_rfc3339(),
        ban_reason: ban_info.as_ref().map(|b| b.0.clone()),
        ban_type: ban_info.as_ref().map(|b| b.1.clone()),
        ban_operator_name: ban_info.as_ref().map(|b| b.2.clone()),
        ban_server_name: ban_info.as_ref().and_then(|b| b.3.clone()),
        upload_token: None,
    })
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

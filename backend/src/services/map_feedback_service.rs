use crate::{
    config::Config,
    db::Database,
    routes::ListQuery,
    services::steam_service::SteamResolver,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Serialize)]
pub struct MapFeedbackItem {
    pub id: Uuid,
    pub feedback_type: String,
    pub steam_id: Option<String>,
    pub steam_persona_name: Option<String>,
    pub contact: Option<String>,
    pub detail: String,
    pub status: String,
    pub reviewed_by: Option<String>,
    pub review_note: Option<String>,
    pub reviewed_at: Option<String>,
    pub created_at: String,
}

#[derive(sqlx::FromRow)]
struct MapFeedbackRow {
    id: Uuid,
    feedback_type: String,
    steam_id: Option<String>,
    steam_persona_name: Option<String>,
    contact: Option<String>,
    detail: String,
    status: String,
    reviewed_by: Option<String>,
    review_note: Option<String>,
    reviewed_at: Option<chrono::DateTime<chrono::Utc>>,
    created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Deserialize)]
pub struct CreateMapFeedbackInput {
    pub feedback_type: String,
    pub steam_input: Option<String>,
    pub contact: Option<String>,
    pub detail: String,
}

#[derive(Deserialize)]
pub struct ReviewMapFeedbackInput {
    pub status: String,
    pub review_note: Option<String>,
}

const FEEDBACK_FIELDS: &str = "id, feedback_type, steam_id, steam_persona_name, contact, detail, status, reviewed_by, review_note, reviewed_at, created_at";

fn row_to_item(row: MapFeedbackRow) -> MapFeedbackItem {
    MapFeedbackItem {
        id: row.id,
        feedback_type: row.feedback_type,
        steam_id: row.steam_id,
        steam_persona_name: row.steam_persona_name,
        contact: row.contact,
        detail: row.detail,
        status: row.status,
        reviewed_by: row.reviewed_by,
        review_note: row.review_note,
        reviewed_at: row.reviewed_at.map(|value| value.to_rfc3339()),
        created_at: row.created_at.to_rfc3339(),
    }
}

pub async fn create_feedback(
    db: &Database,
    config: &Config,
    input: CreateMapFeedbackInput,
) -> anyhow::Result<MapFeedbackItem> {
    let feedback_type = input.feedback_type.trim();
    let detail = input.detail.trim();
    anyhow::ensure!(
        matches!(feedback_type, "missing" | "broken" | "request"),
        "反馈类型无效"
    );
    anyhow::ensure!(!detail.is_empty(), "详细内容不能为空");

    // Steam 标识符可选，填了则解析并自动获取 Steam 名称
    let (steam_id, persona_name) = if let Some(steam_input) = input.steam_input.as_deref() {
        let trimmed = steam_input.trim();
        if trimmed.is_empty() {
            (None, None)
        } else {
            let parsed = SteamResolver::new(config).resolve(trimmed).await?;
            let name = match tokio::time::timeout(
                std::time::Duration::from_secs(5),
                SteamResolver::new(config).fetch_profile(&parsed.steamid64),
            )
            .await
            {
                Ok(Ok(Some(profile))) => Some(profile.persona_name),
                _ => None,
            };
            (Some(parsed.steamid64), name)
        }
    } else {
        (None, None)
    };

    let row = sqlx::query_as::<_, MapFeedbackRow>(&format!(
        r#"INSERT INTO map_feedback (
               id, feedback_type, steam_id, steam_persona_name, contact, detail
           )
           VALUES ($1, $2, $3, $4, $5, $6)
           RETURNING {FEEDBACK_FIELDS}"#
    ))
    .bind(Uuid::new_v4())
    .bind(feedback_type)
    .bind(steam_id)
    .bind(persona_name)
    .bind(super::normalize_optional_string(input.contact))
    .bind(detail)
    .fetch_one(&db.pool)
    .await?;

    Ok(row_to_item(row))
}

pub async fn list_feedback(
    db: &Database,
    query: &ListQuery,
) -> anyhow::Result<crate::routes::PaginatedResponse<MapFeedbackItem>> {
    let mut conditions = Vec::new();
    let mut param_idx = 1u32;
    let search_pattern = query.search_pattern();

    if search_pattern.is_some() {
        conditions.push(format!(
            "(detail ILIKE ${param_idx} OR steam_id ILIKE ${param_idx} OR steam_persona_name ILIKE ${param_idx} OR contact ILIKE ${param_idx})"
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

    let count_sql = format!("SELECT COUNT(*) FROM map_feedback {where_clause}");
    let data_sql = format!(
        r#"SELECT id, feedback_type, steam_id, steam_persona_name, contact, detail,
                  status, COALESCE(reviewer_user.display_name, reviewed_by) AS reviewed_by,
                  review_note, reviewed_at, created_at
           FROM map_feedback
           LEFT JOIN LATERAL (
             SELECT COALESCE(NULLIF(u.remark, ''), u.username) AS display_name
             FROM users u
             WHERE u.username = map_feedback.reviewed_by
                OR NULLIF(u.remark, '') = map_feedback.reviewed_by
             LIMIT 1
           ) reviewer_user ON true
           {where_clause} ORDER BY created_at DESC LIMIT ${param_idx} OFFSET ${}"#,
        param_idx + 1
    );

    let mut count_query = sqlx::query_scalar::<_, i64>(&count_sql);
    let mut data_query = sqlx::query_as::<_, MapFeedbackRow>(&data_sql);

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
        .map(row_to_item)
        .collect();

    Ok(crate::routes::PaginatedResponse {
        items,
        total,
        page: query.page(),
        page_size: query.page_size(),
    })
}

pub async fn review_feedback(
    db: &Database,
    id: Uuid,
    reviewer_name: &str,
    input: ReviewMapFeedbackInput,
) -> anyhow::Result<MapFeedbackItem> {
    let status = input.status.trim();
    anyhow::ensure!(
        matches!(status, "resolved" | "rejected"),
        "审核状态无效"
    );

    let row = sqlx::query_as::<_, MapFeedbackRow>(&format!(
        r#"UPDATE map_feedback
           SET status = $2, reviewed_by = $3, review_note = $4, reviewed_at = now()
           WHERE id = $1 AND status = 'pending'
           RETURNING {FEEDBACK_FIELDS}"#
    ))
    .bind(id)
    .bind(status)
    .bind(reviewer_name)
    .bind(super::normalize_optional_string(input.review_note))
    .fetch_optional(&db.pool)
    .await?;

    let Some(row) = row else {
        let existing: Option<(String,)> =
            sqlx::query_as("SELECT status FROM map_feedback WHERE id = $1")
                .bind(id)
                .fetch_optional(&db.pool)
                .await?;
        if existing.is_some() {
            anyhow::bail!("该反馈已被处理");
        }
        anyhow::bail!("记录不存在");
    };

    Ok(row_to_item(row))
}

/// 按 SteamID 查询玩家的地图反馈记录（公开接口，供玩家查看反馈状态）
pub async fn query_feedback_by_steam_id(
    db: &Database,
    steam_id: &str,
) -> anyhow::Result<Vec<MapFeedbackItem>> {
    let steam_id = steam_id.trim();
    anyhow::ensure!(!steam_id.is_empty(), "SteamID 不能为空");

    let rows = sqlx::query_as::<_, MapFeedbackRow>(&format!(
        r#"SELECT {FEEDBACK_FIELDS} FROM map_feedback WHERE steam_id = $1 ORDER BY created_at DESC"#
    ))
    .bind(steam_id)
    .fetch_all(&db.pool)
    .await?;

    Ok(rows.into_iter().map(row_to_item).collect())
}

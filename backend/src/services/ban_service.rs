use crate::{
    config::Config, db::Database, routes::ListQuery, services::steam_service::SteamResolver,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Serialize)]
pub struct BanItem {
    pub id: Uuid,
    pub player: Option<String>,
    pub steam_id: String,
    pub ip_address: Option<String>,
    pub server_name: Option<String>,
    pub ban_type: String,
    pub duration_minutes: i32,
    pub expires_at: Option<String>,
    pub reason: String,
    pub status: String,
    pub operator_name: String,
    pub source: String,
    pub server_id: Option<Uuid>,
    pub server_port: Option<i32>,
    pub removed_reason: Option<String>,
    pub removed_by: Option<String>,
    pub removed_at: Option<String>,
    pub created_at: String,
}

#[derive(Serialize)]
pub struct BanListItem {
    pub id: Uuid,
    pub player: Option<String>,
    pub steam_id: String,
    pub ip_address: Option<String>,
    pub ban_type: String,
    pub duration_minutes: i32,
    pub expires_at: Option<String>,
    pub reason: String,
    pub status: String,
    pub operator_name: String,
    pub created_at: String,
}

#[derive(Clone, Debug, sqlx::FromRow)]
pub struct BanRecord {
    pub player: Option<String>,
    pub steam_id: String,
    pub operator_name: String,
}

#[derive(sqlx::FromRow)]
pub(crate) struct BanRow {
    pub id: Uuid,
    pub player: Option<String>,
    pub steam_id: String,
    pub ip_address: Option<String>,
    pub server_name: Option<String>,
    pub ban_type: String,
    pub duration_minutes: i32,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    pub reason: String,
    pub status: String,
    pub operator_name: String,
    pub source: String,
    pub server_id: Option<Uuid>,
    pub server_port: Option<i32>,
    pub removed_reason: Option<String>,
    pub removed_by: Option<String>,
    pub removed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(sqlx::FromRow)]
struct BanListRow {
    pub id: Uuid,
    pub player: Option<String>,
    pub steam_id: String,
    pub ip_address: Option<String>,
    pub ban_type: String,
    pub duration_minutes: i32,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    pub reason: String,
    pub status: String,
    pub operator_name: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Deserialize)]
pub struct CreateBanInput {
    pub player: Option<String>,
    pub steam_id: String,
    pub ip_address: Option<String>,
    pub ban_type: String,
    pub reason: String,
    pub operator_name: String,
}

#[derive(Deserialize)]
pub struct UpdateBanInput {
    pub player: Option<String>,
    pub steam_id: String,
    pub ip_address: Option<String>,
    pub ban_type: String,
    pub reason: String,
}

const BAN_FIELDS: &str = "id, player, steam_id, ip_address, server_name, ban_type, duration_minutes, expires_at, reason, status, operator_name, source, server_id, server_port, removed_reason, removed_by, removed_at, created_at";
const BAN_LIST_FIELDS: &str = "id, player, steam_id, ip_address, ban_type, duration_minutes, expires_at, reason, status, operator_name, created_at";

pub(crate) fn row_to_item(row: BanRow) -> BanItem {
    BanItem {
        id: row.id,
        player: row.player,
        steam_id: row.steam_id,
        ip_address: row.ip_address,
        server_name: row.server_name,
        ban_type: row.ban_type,
        duration_minutes: row.duration_minutes,
        expires_at: row.expires_at.map(|value| value.to_rfc3339()),
        reason: row.reason,
        status: row.status,
        operator_name: row.operator_name,
        source: row.source,
        server_id: row.server_id,
        server_port: row.server_port,
        removed_reason: row.removed_reason,
        removed_by: row.removed_by,
        removed_at: row.removed_at.map(|value| value.to_rfc3339()),
        created_at: row.created_at.to_rfc3339(),
    }
}

fn list_row_to_item(row: BanListRow) -> BanListItem {
    BanListItem {
        id: row.id,
        player: row.player,
        steam_id: row.steam_id,
        ip_address: row.ip_address,
        ban_type: row.ban_type,
        duration_minutes: row.duration_minutes,
        expires_at: row.expires_at.map(|value| value.to_rfc3339()),
        reason: row.reason,
        status: row.status,
        operator_name: row.operator_name,
        created_at: row.created_at.to_rfc3339(),
    }
}

pub async fn list_bans(
    db: &Database,
    query: &ListQuery,
) -> anyhow::Result<crate::routes::PaginatedResponse<BanListItem>> {
    let mut conditions = Vec::new();
    let mut param_idx = 1u32;
    let search_pattern = query.search_pattern();

    if search_pattern.is_some() {
        conditions.push(format!(
            "(steam_id ILIKE ${param_idx} OR player ILIKE ${param_idx})"
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

    let count_sql = format!("SELECT COUNT(*) FROM ban_records {where_clause}");
    let data_sql = format!("SELECT {BAN_LIST_FIELDS} FROM ban_records {where_clause} ORDER BY created_at DESC LIMIT ${param_idx} OFFSET ${}", param_idx + 1);

    let mut count_query = sqlx::query_scalar::<_, i64>(&count_sql);
    let mut data_query = sqlx::query_as::<_, BanListRow>(&data_sql);

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
        .map(list_row_to_item)
        .collect();

    Ok(crate::routes::PaginatedResponse {
        items,
        total,
        page: query.page(),
        page_size: query.page_size(),
    })
}

pub async fn get_ban(db: &Database, id: Uuid) -> anyhow::Result<BanItem> {
    let row = sqlx::query_as::<_, BanRow>(&format!(
        "SELECT {BAN_FIELDS} FROM ban_records WHERE id = $1"
    ))
    .bind(id)
    .fetch_one(&db.pool)
    .await?;
    Ok(row_to_item(row))
}

pub async fn create_ban(
    db: &Database,
    config: &Config,
    input: CreateBanInput,
) -> anyhow::Result<BanItem> {
    let steam_id_input = input.steam_id.trim();
    let ban_type = input.ban_type.trim();
    let reason = input.reason.trim();

    anyhow::ensure!(!steam_id_input.is_empty(), "SteamID64 不能为空");
    anyhow::ensure!(matches!(ban_type, "steam" | "ip"), "封禁属性无效");
    anyhow::ensure!(!reason.is_empty(), "封禁理由不能为空");

    let steam_id = SteamResolver::new(config)
        .resolve(steam_id_input)
        .await?
        .steamid64;

    // 检查是否已有活跃封禁
    let existing: Option<(Uuid,)> =
        sqlx::query_as(r#"SELECT id FROM ban_records WHERE steam_id = $1 AND status = 'active'"#)
            .bind(&steam_id)
            .fetch_optional(&db.pool)
            .await?;
    if existing.is_some() {
        anyhow::bail!("该玩家已有活跃封禁记录，请先解封后再重新封禁");
    }

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
    .bind(super::normalize_optional_string(input.player))
    .bind(&steam_id)
    .bind(super::normalize_optional_string(input.ip_address))
    .bind(ban_type)
    .bind(reason)
    .bind(input.operator_name)
    .fetch_one(&db.pool)
    .await?;

    Ok(row_to_item(row))
}

pub async fn update_ban(
    db: &Database,
    config: &Config,
    id: Uuid,
    input: UpdateBanInput,
) -> anyhow::Result<BanItem> {
    let steam_id_input = input.steam_id.trim();
    let ban_type = input.ban_type.trim();
    let reason = input.reason.trim();

    anyhow::ensure!(!steam_id_input.is_empty(), "SteamID64 不能为空");
    anyhow::ensure!(matches!(ban_type, "steam" | "ip"), "封禁属性无效");
    anyhow::ensure!(!reason.is_empty(), "封禁理由不能为空");

    let steam_id = SteamResolver::new(config)
        .resolve(steam_id_input)
        .await?
        .steamid64;
    let row = sqlx::query_as::<_, BanRow>(
        r#"UPDATE ban_records
           SET player = $2,
               steam_id = $3,
               ip_address = $4,
               ban_type = $5,
               reason = $6
           WHERE id = $1
           RETURNING id, player, steam_id, ip_address, server_name, ban_type,
                     duration_minutes, expires_at, reason, status, operator_name, source,
                     server_id, server_port, removed_reason, removed_by, removed_at, created_at"#,
    )
    .bind(id)
    .bind(super::normalize_optional_string(input.player))
    .bind(&steam_id)
    .bind(super::normalize_optional_string(input.ip_address))
    .bind(ban_type)
    .bind(reason)
    .fetch_one(&db.pool)
    .await?;

    Ok(row_to_item(row))
}

pub async fn delete_ban(db: &Database, id: Uuid) -> anyhow::Result<()> {
    sqlx::query(r#"DELETE FROM ban_records WHERE id = $1"#)
        .bind(id)
        .execute(&db.pool)
        .await?;
    Ok(())
}

pub async fn find_ban(db: &Database, id: Uuid) -> anyhow::Result<BanRecord> {
    sqlx::query_as::<_, BanRecord>(
        "SELECT player, steam_id, operator_name FROM ban_records WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&db.pool)
    .await
    .map_err(Into::into)
}

/// 按 SteamID64 查询该玩家的所有活跃封禁记录（供公开申诉页使用）
pub async fn find_active_bans_by_steamid(
    db: &Database,
    steamid64: &str,
) -> anyhow::Result<Vec<BanItem>> {
    let rows = sqlx::query_as::<_, BanRow>(
        &format!("SELECT {BAN_FIELDS} FROM ban_records WHERE steam_id = $1 AND status = 'active' ORDER BY created_at DESC"),
    )
    .bind(steamid64)
    .fetch_all(&db.pool)
    .await?;
    Ok(rows.into_iter().map(row_to_item).collect())
}

pub async fn unban(db: &Database, id: Uuid, removed_by: &str) -> anyhow::Result<BanItem> {
    let row = sqlx::query_as::<_, BanRow>(
        r#"UPDATE ban_records
           SET status = 'inactive', removed_by = $2, removed_at = now()
           WHERE id = $1
           RETURNING id, player, steam_id, ip_address, server_name, ban_type,
                     duration_minutes, expires_at, reason, status, operator_name, source,
                     server_id, server_port, removed_reason, removed_by, removed_at, created_at"#,
    )
    .bind(id)
    .bind(removed_by)
    .fetch_one(&db.pool)
    .await?;

    Ok(row_to_item(row))
}

use crate::{
    db::Database,
    http_client::http_client,
    services::ban_service::{self, BanItem},
};
use chrono::{DateTime, Utc};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use uuid::Uuid;

const VALID_BAN_TYPES: &[&str] = &[
    "ban_evasion",
    "bhop_hack",
    "bhop_macro",
    "exploiting",
    "strafe_hack",
    "strafe_macro",
    "other",
];

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ExternalBanApiTargetPublic {
    pub id: Uuid,
    pub name: String,
    pub enabled: bool,
    pub base_url: String,
    pub has_token: bool,
    pub default_ban_type: String,
    pub auto_sync: bool,
    pub notes_template: String,
    pub stats_template: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub(crate) struct ExternalBanApiTargetRow {
    pub id: Uuid,
    pub name: String,
    pub enabled: bool,
    pub base_url: String,
    pub bearer_token: Option<String>,
    pub default_ban_type: String,
    pub auto_sync: bool,
    pub notes_template: String,
    pub stats_template: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ExternalBanApiTargetInput {
    pub name: String,
    pub enabled: bool,
    pub base_url: String,
    pub bearer_token: Option<String>,
    pub default_ban_type: String,
    pub auto_sync: bool,
    pub notes_template: String,
    pub stats_template: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExternalBanApiTestResult {
    pub ok: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExternalBanSyncItem {
    pub target_id: Uuid,
    pub target_name: String,
    pub ok: bool,
    pub message: String,
    pub external_uuid: Option<String>,
    pub external_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExternalBanSyncSummary {
    pub ok: bool,
    pub message: String,
    pub items: Vec<ExternalBanSyncItem>,
}

#[derive(Debug, Clone, Serialize)]
struct ExternalBanCreatePayload {
    steamid64: String,
    ban_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    expires_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    notes: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stats: Option<String>,
}

// ---- Sync record listing (for sync history UI) ----

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ExternalBanSyncRecord {
    pub local_ban_id: Uuid,
    pub target_id: Uuid,
    pub target_name: String,
    pub external_uuid: Option<String>,
    pub external_id: Option<i64>,
    pub status: String,
    pub last_error: Option<String>,
    pub synced_at: Option<String>,
    pub updated_at: String,
    pub ban_player: Option<String>,
    pub ban_steam_id: Option<String>,
    pub ban_reason: Option<String>,
    pub ban_status: Option<String>,
    #[sqlx(default)]
    pub total: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SyncRecordQuery {
    pub target_id: Option<Uuid>,
    pub status: Option<String>,
    pub search: Option<String>,
    pub page: Option<i64>,
    pub page_size: Option<i64>,
}

fn public_target(row: ExternalBanApiTargetRow) -> ExternalBanApiTargetPublic {
    ExternalBanApiTargetPublic {
        id: row.id,
        name: row.name,
        enabled: row.enabled,
        base_url: row.base_url,
        has_token: row
            .bearer_token
            .as_deref()
            .map(|value| !value.trim().is_empty())
            .unwrap_or(false),
        default_ban_type: row.default_ban_type,
        auto_sync: row.auto_sync,
        notes_template: row.notes_template,
        stats_template: row.stats_template,
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}

fn normalize_name(value: &str) -> anyhow::Result<String> {
    let name = value.trim();
    anyhow::ensure!(!name.is_empty(), "名称不能为空");
    Ok(name.to_string())
}

fn normalize_base_url(value: &str) -> anyhow::Result<String> {
    let trimmed = value.trim().trim_end_matches('/');
    anyhow::ensure!(!trimmed.is_empty(), "API 地址不能为空");
    anyhow::ensure!(
        trimmed.starts_with("https://") || trimmed.starts_with("http://"),
        "API 地址必须以 http:// 或 https:// 开头"
    );
    Ok(trimmed.to_string())
}

fn normalize_ban_type(value: &str) -> anyhow::Result<String> {
    let ban_type = value.trim();
    anyhow::ensure!(VALID_BAN_TYPES.contains(&ban_type), "外部封禁类型无效");
    Ok(ban_type.to_string())
}

fn normalize_optional(value: Option<String>) -> Option<String> {
    value
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
}

fn validate_input(input: &ExternalBanApiTargetInput) -> anyhow::Result<(String, String, String)> {
    let name = normalize_name(&input.name)?;
    let base_url = normalize_base_url(&input.base_url)?;
    let default_ban_type = normalize_ban_type(&input.default_ban_type)?;
    anyhow::ensure!(!input.notes_template.trim().is_empty(), "备注模板不能为空");
    Ok((name, base_url, default_ban_type))
}

async fn get_target_row(db: &Database, id: Uuid) -> anyhow::Result<ExternalBanApiTargetRow> {
    sqlx::query_as::<_, ExternalBanApiTargetRow>(
        r#"SELECT id, name, enabled, base_url, bearer_token, default_ban_type, auto_sync,
                  notes_template, stats_template, created_at, updated_at
           FROM external_ban_api_targets
           WHERE id = $1"#,
    )
    .bind(id)
    .fetch_one(&db.pool)
    .await
    .map_err(Into::into)
}

pub async fn list_targets(db: &Database) -> anyhow::Result<Vec<ExternalBanApiTargetPublic>> {
    let rows = sqlx::query_as::<_, ExternalBanApiTargetRow>(
        r#"SELECT id, name, enabled, base_url, bearer_token, default_ban_type, auto_sync,
                  notes_template, stats_template, created_at, updated_at
           FROM external_ban_api_targets
           ORDER BY created_at ASC"#,
    )
    .fetch_all(&db.pool)
    .await?;
    Ok(rows.into_iter().map(public_target).collect())
}

pub async fn create_target(
    db: &Database,
    input: ExternalBanApiTargetInput,
) -> anyhow::Result<ExternalBanApiTargetPublic> {
    let (name, base_url, default_ban_type) = validate_input(&input)?;
    let row = sqlx::query_as::<_, ExternalBanApiTargetRow>(
        r#"INSERT INTO external_ban_api_targets (
               id, name, enabled, base_url, bearer_token, default_ban_type,
               auto_sync, notes_template, stats_template
           )
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
           RETURNING id, name, enabled, base_url, bearer_token, default_ban_type,
                     auto_sync, notes_template, stats_template, created_at, updated_at"#,
    )
    .bind(Uuid::new_v4())
    .bind(name)
    .bind(input.enabled)
    .bind(base_url)
    .bind(normalize_optional(input.bearer_token))
    .bind(default_ban_type)
    .bind(input.auto_sync)
    .bind(input.notes_template.trim())
    .bind(normalize_optional(input.stats_template))
    .fetch_one(&db.pool)
    .await?;
    Ok(public_target(row))
}

pub async fn update_target(
    db: &Database,
    id: Uuid,
    input: ExternalBanApiTargetInput,
) -> anyhow::Result<ExternalBanApiTargetPublic> {
    let (name, base_url, default_ban_type) = validate_input(&input)?;
    let row = sqlx::query_as::<_, ExternalBanApiTargetRow>(
        r#"UPDATE external_ban_api_targets
           SET name = $2,
               enabled = $3,
               base_url = $4,
               bearer_token = COALESCE($5, bearer_token),
               default_ban_type = $6,
               auto_sync = $7,
               notes_template = $8,
               stats_template = $9,
               updated_at = now()
           WHERE id = $1
           RETURNING id, name, enabled, base_url, bearer_token, default_ban_type,
                     auto_sync, notes_template, stats_template, created_at, updated_at"#,
    )
    .bind(id)
    .bind(name)
    .bind(input.enabled)
    .bind(base_url)
    .bind(normalize_optional(input.bearer_token))
    .bind(default_ban_type)
    .bind(input.auto_sync)
    .bind(input.notes_template.trim())
    .bind(normalize_optional(input.stats_template))
    .fetch_one(&db.pool)
    .await?;
    Ok(public_target(row))
}

pub async fn delete_target(db: &Database, id: Uuid) -> anyhow::Result<()> {
    let result = sqlx::query("DELETE FROM external_ban_api_targets WHERE id = $1")
        .bind(id)
        .execute(&db.pool)
        .await?;
    anyhow::ensure!(result.rows_affected() > 0, "外部 API 不存在");
    Ok(())
}

/// 根据目标 ID 获取目标信息描述（用于日志记录）
pub async fn find_target_info(db: &Database, id: Uuid) -> Option<String> {
    let row: Option<(String, String)> = sqlx::query_as(
        r#"SELECT name, base_url FROM external_ban_api_targets WHERE id = $1"#,
    )
    .bind(id)
    .fetch_optional(&db.pool)
    .await
    .ok()?;

    row.map(|(name, url)| format!("{} ({})", name, url))
}

pub async fn test_target(db: &Database, id: Uuid) -> anyhow::Result<ExternalBanApiTestResult> {
    let target = get_target_row(db, id).await?;
    let token = bearer_token(&target)?;
    let url = format!("{}/v1/bans?limit=1", target.base_url.trim_end_matches('/'));
    let response = http_client().get(url).bearer_auth(token).send().await?;
    if response.status().is_success() {
        return Ok(ExternalBanApiTestResult {
            ok: true,
            message: format!("{} 连接成功", target.name),
        });
    }

    let status = response.status();
    let message = read_error_message(response).await;
    Ok(ExternalBanApiTestResult {
        ok: false,
        message: format!("{} 返回 {status}: {message}", target.name),
    })
}

pub async fn sync_ban(db: &Database, ban_id: Uuid) -> anyhow::Result<ExternalBanSyncSummary> {
    let ban = ban_service::get_ban(db, ban_id).await?;
    let targets = sqlx::query_as::<_, ExternalBanApiTargetRow>(
        r#"SELECT id, name, enabled, base_url, bearer_token, default_ban_type, auto_sync,
                  notes_template, stats_template, created_at, updated_at
           FROM external_ban_api_targets
           WHERE enabled = true
           ORDER BY created_at ASC"#,
    )
    .fetch_all(&db.pool)
    .await?;
    anyhow::ensure!(!targets.is_empty(), "没有启用的外部封禁 API");
    Ok(sync_ban_to_targets(db, &targets, &ban).await)
}

pub async fn sync_ban_to_target(
    db: &Database,
    ban_id: Uuid,
    target_id: Uuid,
) -> anyhow::Result<ExternalBanSyncSummary> {
    let ban = ban_service::get_ban(db, ban_id).await?;
    let target = get_target_row(db, target_id).await?;
    anyhow::ensure!(target.enabled, "外部封禁 API 未启用");
    Ok(sync_ban_to_targets(db, &[target], &ban).await)
}

pub async fn sync_ban_if_enabled(db: &Database, ban: &BanItem) -> ExternalBanSyncSummary {
    let targets = sqlx::query_as::<_, ExternalBanApiTargetRow>(
        r#"SELECT id, name, enabled, base_url, bearer_token, default_ban_type, auto_sync,
                  notes_template, stats_template, created_at, updated_at
           FROM external_ban_api_targets
           WHERE enabled = true AND auto_sync = true
           ORDER BY created_at ASC"#,
    )
    .fetch_all(&db.pool)
    .await
    .unwrap_or_default();
    if targets.is_empty() {
        return ExternalBanSyncSummary {
            ok: true,
            message: "无自动同步目标".to_string(),
            items: vec![],
        };
    }
    sync_ban_to_targets(db, &targets, ban).await
}

async fn sync_ban_to_targets(
    db: &Database,
    targets: &[ExternalBanApiTargetRow],
    ban: &BanItem,
) -> ExternalBanSyncSummary {
    let futures: Vec<_> = targets
        .iter()
        .map(|target| {
            let db = db.clone();
            let target = target.clone();
            async move {
                match sync_ban_with_target(&db, &target, ban).await {
                    Ok(result) => result,
                    Err(error) => {
                        tracing::warn!(ban_id = %ban.id, target_id = %target.id, %error, "external ban sync failed");
                        ExternalBanSyncItem {
                            target_id: target.id,
                            target_name: target.name.clone(),
                            ok: false,
                            message: error.to_string(),
                            external_uuid: None,
                            external_id: None,
                        }
                    }
                }
            }
        })
        .collect();
    let items = futures::future::join_all(futures).await;

    let ok_count = items.iter().filter(|item| item.ok).count();
    ExternalBanSyncSummary {
        ok: ok_count == items.len(),
        message: format!("外部封禁 API 同步完成：成功 {ok_count}/{}", items.len()),
        items,
    }
}

async fn sync_ban_with_target(
    db: &Database,
    target: &ExternalBanApiTargetRow,
    ban: &BanItem,
) -> anyhow::Result<ExternalBanSyncItem> {
    if let Some(existing) = existing_successful_sync(db, ban.id, target.id).await? {
        return Ok(existing);
    }

    let token = bearer_token(target)?;
    let payload = build_payload(target, ban)?;
    let url = format!("{}/v1/bans", target.base_url.trim_end_matches('/'));
    let response = tokio::time::timeout(
        Duration::from_secs(15),
        http_client()
            .post(url)
            .bearer_auth(token)
            .json(&payload)
            .send(),
    )
    .await
    .map_err(|_| anyhow::anyhow!("{} 请求超时（15s）", target.name))??;

    let status = response.status();
    if status.is_success() {
        let value: serde_json::Value = response
            .json()
            .await
            .unwrap_or_else(|_| serde_json::json!({}));
        let external_uuid = value
            .get("uuid")
            .and_then(|v| v.as_str())
            .map(ToString::to_string);
        let external_id = value.get("id").and_then(|v| v.as_i64());
        upsert_sync(
            db,
            ban.id,
            target.id,
            external_uuid.as_deref(),
            external_id,
            "synced",
            None,
        )
        .await?;
        return Ok(ExternalBanSyncItem {
            target_id: target.id,
            target_name: target.name.clone(),
            ok: true,
            message: "已同步".to_string(),
            external_uuid,
            external_id,
        });
    }

    let message = read_error_message(response).await;
    upsert_sync(db, ban.id, target.id, None, None, "failed", Some(&message)).await?;

    if status == StatusCode::UNPROCESSABLE_ENTITY {
        anyhow::bail!("{} 校验失败: {message}", target.name);
    }
    anyhow::bail!("{} 返回 {status}: {message}", target.name);
}

fn bearer_token(target: &ExternalBanApiTargetRow) -> anyhow::Result<&str> {
    target
        .bearer_token
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("{} 未配置 Bearer Token", target.name))
}

async fn existing_successful_sync(
    db: &Database,
    local_ban_id: Uuid,
    target_id: Uuid,
) -> anyhow::Result<Option<ExternalBanSyncItem>> {
    #[derive(sqlx::FromRow)]
    struct SyncRow {
        external_uuid: Option<String>,
        external_id: Option<i64>,
        target_name: String,
    }

    let row = sqlx::query_as::<_, SyncRow>(
        r#"SELECT s.external_uuid, s.external_id, t.name AS target_name
           FROM external_ban_syncs s
           JOIN external_ban_api_targets t ON t.id = s.target_id
           WHERE s.local_ban_id = $1 AND s.target_id = $2 AND s.status = 'synced'"#,
    )
    .bind(local_ban_id)
    .bind(target_id)
    .fetch_optional(&db.pool)
    .await?;

    Ok(row.map(|item| ExternalBanSyncItem {
        target_id,
        target_name: item.target_name,
        ok: true,
        message: "已同步，跳过重复创建".to_string(),
        external_uuid: item.external_uuid,
        external_id: item.external_id,
    }))
}

fn build_payload(
    target: &ExternalBanApiTargetRow,
    ban: &BanItem,
) -> anyhow::Result<ExternalBanCreatePayload> {
    anyhow::ensure!(
        ban.steam_id.trim().len() == 17 && ban.steam_id.chars().all(|c| c.is_ascii_digit()),
        "外部封禁 API 需要 SteamID64"
    );

    Ok(ExternalBanCreatePayload {
        steamid64: ban.steam_id.trim().to_string(),
        ban_type: target.default_ban_type.clone(),
        expires_at: ban.expires_at.clone(),
        notes: Some(render_template(&target.notes_template, ban)),
        stats: target
            .stats_template
            .as_deref()
            .map(|template| render_template(template, ban)),
    })
}

fn render_template(template: &str, ban: &BanItem) -> String {
    template
        .replace("{player}", ban.player.as_deref().unwrap_or(""))
        .replace("{steam_id}", &ban.steam_id)
        .replace("{ip_address}", ban.ip_address.as_deref().unwrap_or(""))
        .replace("{reason}", &ban.reason)
        .replace("{operator}", &ban.operator_name)
        .replace("{source}", &ban.source)
        .replace("{server_name}", ban.server_name.as_deref().unwrap_or(""))
}

async fn read_error_message(response: reqwest::Response) -> String {
    let fallback = response.status().to_string();
    let value = response.json::<serde_json::Value>().await.ok();
    value
        .as_ref()
        .and_then(|json| {
            json.get("detail")
                .or_else(|| json.get("error"))
                .or_else(|| json.get("message"))
        })
        .map(|detail| {
            detail
                .as_str()
                .map(ToString::to_string)
                .unwrap_or_else(|| detail.to_string())
        })
        .filter(|message| !message.is_empty())
        .unwrap_or(fallback)
}

async fn upsert_sync(
    db: &Database,
    local_ban_id: Uuid,
    target_id: Uuid,
    external_uuid: Option<&str>,
    external_id: Option<i64>,
    status: &str,
    last_error: Option<&str>,
) -> anyhow::Result<()> {
    sqlx::query(
        r#"INSERT INTO external_ban_syncs (
               local_ban_id, target_id, external_uuid, external_id, status, last_error, synced_at, updated_at
           )
           VALUES ($1, $2, $3, $4, $5, $6, CASE WHEN $5 = 'synced' THEN now() ELSE NULL END, now())
           ON CONFLICT (local_ban_id, target_id) DO UPDATE SET
               external_uuid = COALESCE(EXCLUDED.external_uuid, external_ban_syncs.external_uuid),
               external_id = COALESCE(EXCLUDED.external_id, external_ban_syncs.external_id),
               status = EXCLUDED.status,
               last_error = EXCLUDED.last_error,
               synced_at = CASE WHEN EXCLUDED.status = 'synced' THEN now() ELSE external_ban_syncs.synced_at END,
               updated_at = now()"#,
    )
    .bind(local_ban_id)
    .bind(target_id)
    .bind(external_uuid)
    .bind(external_id)
    .bind(status)
    .bind(last_error)
    .execute(&db.pool)
    .await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Unsync: notify external APIs when a ban is unbanned or deleted
// ---------------------------------------------------------------------------

/// Delete a ban on an external API by its external_uuid.
async fn delete_external_ban(
    base_url: &str,
    token: &str,
    external_uuid: &str,
) -> anyhow::Result<()> {
    let url = format!(
        "{}/v1/bans/{}",
        base_url.trim_end_matches('/'),
        external_uuid
    );
    let response = tokio::time::timeout(
        Duration::from_secs(15),
        http_client().delete(&url).bearer_auth(token).send(),
    )
    .await
    .map_err(|_| anyhow::anyhow!("删除外部封禁请求超时（15s）"))??;

    if response.status().is_success() || response.status() == StatusCode::NOT_FOUND {
        return Ok(());
    }
    let status = response.status();
    let message = read_error_message(response).await;
    anyhow::bail!("删除外部封禁返回 {status}: {message}");
}

/// Notify all external APIs that a ban has been lifted.
/// Called after unban when the ban record still exists (status = 'inactive').
pub async fn unsync_ban(db: &Database, ban_id: Uuid) -> anyhow::Result<()> {
    #[derive(sqlx::FromRow)]
    struct SyncedRecord {
        target_id: Uuid,
        external_uuid: Option<String>,
    }

    let records: Vec<SyncedRecord> = sqlx::query_as(
        r#"SELECT target_id, external_uuid
           FROM external_ban_syncs
           WHERE local_ban_id = $1 AND status = 'synced' AND external_uuid IS NOT NULL"#,
    )
    .bind(ban_id)
    .fetch_all(&db.pool)
    .await?;

    if records.is_empty() {
        return Ok(());
    }

    for record in &records {
        let target = match get_target_row(db, record.target_id).await {
            Ok(t) => t,
            Err(e) => {
                tracing::warn!(target_id = %record.target_id, %e, "failed to fetch target for unsync");
                continue;
            }
        };

        let token = match bearer_token(&target) {
            Ok(t) => t,
            Err(e) => {
                tracing::warn!(target_id = %target.id, %e, "no bearer token for unsync");
                continue;
            }
        };

        let uuid_str = match record.external_uuid.as_deref() {
            Some(u) => u,
            None => continue,
        };

        match delete_external_ban(&target.base_url, token, uuid_str).await {
            Ok(()) => {
                if let Err(e) = upsert_sync(
                    db,
                    ban_id,
                    target.id,
                    None,
                    None,
                    "unsynced",
                    None,
                )
                .await
                {
                    tracing::warn!(%e, "failed to update sync status to unsynced");
                }
            }
            Err(e) => {
                tracing::warn!(ban_id = %ban_id, target_id = %target.id, %e, "external ban delete failed");
                if let Err(up_err) = upsert_sync(
                    db,
                    ban_id,
                    target.id,
                    None,
                    None,
                    "unsynced",
                    Some(&e.to_string()),
                )
                .await
                {
                    tracing::warn!(%up_err, "failed to update sync status");
                }
            }
        }
    }

    Ok(())
}

/// Read sync records for a ban *before* hard-deleting the ban record.
/// Returns (target_row, external_uuid) pairs so we can notify external APIs after the delete.
pub async fn read_sync_records_before_delete(
    db: &Database,
    ban_id: Uuid,
) -> anyhow::Result<Vec<(ExternalBanApiTargetRow, String)>> {
    #[derive(sqlx::FromRow)]
    struct DeletedSync {
        target_id: Option<Uuid>,
        external_uuid: Option<String>,
    }

    let deleted: Vec<DeletedSync> = sqlx::query_as(
        r#"DELETE FROM external_ban_syncs
           WHERE local_ban_id = $1
           RETURNING target_id, external_uuid"#,
    )
    .bind(ban_id)
    .fetch_all(&db.pool)
    .await?;

    let mut result = Vec::new();
    for row in deleted {
        let target_id = match row.target_id {
            Some(id) => id,
            None => continue,
        };
        let external_uuid = match row.external_uuid {
            Some(u) => u,
            None => continue,
        };
        if let Ok(target) = get_target_row(db, target_id).await {
            result.push((target, external_uuid));
        }
    }
    Ok(result)
}

/// Notify external APIs about deleted sync records (called after hard-deleting the ban).
pub async fn notify_external_deletes(
    records: &[(ExternalBanApiTargetRow, String)],
) {
    for (target, external_uuid) in records {
        let token = match bearer_token(target) {
            Ok(t) => t,
            Err(e) => {
                tracing::warn!(target_id = %target.id, %e, "no bearer token for external delete");
                continue;
            }
        };
        if let Err(e) = delete_external_ban(&target.base_url, token, external_uuid).await {
            tracing::warn!(target_id = %target.id, external_uuid = %external_uuid, %e, "external ban delete failed");
        }
    }
}

// ---------------------------------------------------------------------------
// Re-sync: when a ban is updated, delete old external bans and re-create
// ---------------------------------------------------------------------------

pub async fn resync_ban(
    db: &Database,
    ban_id: Uuid,
    updated_ban: &BanItem,
) -> anyhow::Result<ExternalBanSyncSummary> {
    // 1. Find all synced records for this ban with an external UUID
    #[derive(sqlx::FromRow)]
    struct SyncedRecord {
        target_id: Uuid,
        external_uuid: Option<String>,
    }
    let records: Vec<SyncedRecord> = sqlx::query_as(
        r#"SELECT target_id, external_uuid
           FROM external_ban_syncs
           WHERE local_ban_id = $1 AND status = 'synced'"#,
    )
    .bind(ban_id)
    .fetch_all(&db.pool)
    .await?;

    // 2. Delete external bans and collect targets to reset
    let mut reset_target_ids: Vec<Uuid> = Vec::with_capacity(records.len());
    for record in &records {
        if let Some(ref external_uuid) = record.external_uuid {
            if let Ok(target) = get_target_row(db, record.target_id).await {
                if let Ok(token) = bearer_token(&target) {
                    if let Err(e) =
                        delete_external_ban(&target.base_url, token, external_uuid).await
                    {
                        tracing::warn!(ban_id = %ban_id, target_id = %record.target_id, %e, "failed to delete external ban for resync");
                    }
                }
            }
        }
        reset_target_ids.push(record.target_id);
    }

    // 批量重置同步行为 pending（原逐条 UPDATE）
    if !reset_target_ids.is_empty() {
        if let Err(e) = sqlx::query(
            r#"UPDATE external_ban_syncs
               SET status = 'pending', external_uuid = NULL, external_id = NULL,
                   synced_at = NULL, last_error = NULL, updated_at = now()
               WHERE local_ban_id = $1 AND target_id = ANY($2)"#,
        )
        .bind(ban_id)
        .bind(&reset_target_ids)
        .execute(&db.pool)
        .await
        {
            tracing::warn!(ban_id = %ban_id, %e, "failed to reset sync rows to pending");
        }
    }

    // 3. Re-sync to all enabled targets
    let targets = sqlx::query_as::<_, ExternalBanApiTargetRow>(
        r#"SELECT id, name, enabled, base_url, bearer_token, default_ban_type, auto_sync,
                  notes_template, stats_template, created_at, updated_at
           FROM external_ban_api_targets
           WHERE enabled = true
           ORDER BY created_at ASC"#,
    )
    .fetch_all(&db.pool)
    .await?;

    if targets.is_empty() {
        return Ok(ExternalBanSyncSummary {
            ok: true,
            message: "无启用的外部封禁 API".to_string(),
            items: vec![],
        });
    }

    Ok(sync_ban_to_targets(db, &targets, updated_ban).await)
}

// ---------------------------------------------------------------------------
// Sync history: list sync records, get per-ban sync status
// ---------------------------------------------------------------------------

pub async fn list_sync_records(
    db: &Database,
    query: &SyncRecordQuery,
) -> anyhow::Result<(Vec<ExternalBanSyncRecord>, i64)> {
    let page = query.page.unwrap_or(1).max(1);
    let page_size = query.page_size.unwrap_or(20).clamp(1, 100);
    let offset = (page - 1) * page_size;

    let mut sql = String::from(
        r#"SELECT s.local_ban_id, s.target_id, t.name AS target_name,
                  s.external_uuid, s.external_id, s.status, s.last_error,
                  s.synced_at::TEXT, s.updated_at::TEXT,
                  b.player AS ban_player, b.steam_id AS ban_steam_id,
                  b.reason AS ban_reason, b.status AS ban_status,
                  COUNT(*) OVER() AS total
           FROM external_ban_syncs s
           JOIN external_ban_api_targets t ON t.id = s.target_id
           LEFT JOIN ban_records b ON b.id = s.local_ban_id
           WHERE 1=1"#,
    );
    let mut bind_idx = 1u32;

    if query.target_id.is_some() {
        bind_idx += 1;
        sql.push_str(&format!(" AND s.target_id = ${bind_idx}"));
    }
    if query.status.is_some() {
        bind_idx += 1;
        sql.push_str(&format!(" AND s.status = ${bind_idx}"));
    }
    if query.search.is_some() {
        bind_idx += 1;
        let search_idx = bind_idx;
        bind_idx += 1;
        sql.push_str(&format!(
            " AND (b.steam_id ILIKE ${search_idx} OR b.player ILIKE ${bind_idx})"
        ));
    }

    sql.push_str(" ORDER BY s.updated_at DESC");
    sql.push_str(&format!(
        " LIMIT {page_size_int} OFFSET {offset_int}",
        page_size_int = page_size,
        offset_int = offset
    ));

    let mut q = sqlx::query_as::<_, ExternalBanSyncRecord>(&sql);

    // Pre-compute search pattern to satisfy lifetime requirements
    let search_pattern = query
        .search
        .as_ref()
        .map(|s| format!("%{s}%"));

    if let Some(ref target_id) = query.target_id {
        q = q.bind(target_id);
    }
    if let Some(ref status) = query.status {
        q = q.bind(status);
    }
    if let Some(ref pattern) = search_pattern {
        q = q.bind(pattern).bind(pattern);
    }

    let rows = q.fetch_all(&db.pool).await?;
    let total = rows.first().and_then(|r| r.total).unwrap_or(0);

    Ok((rows, total))
}

pub async fn get_ban_sync_status(
    db: &Database,
    ban_id: Uuid,
) -> anyhow::Result<Vec<ExternalBanSyncRecord>> {
    let rows = sqlx::query_as::<_, ExternalBanSyncRecord>(
        r#"SELECT s.local_ban_id, s.target_id, t.name AS target_name,
                  s.external_uuid, s.external_id, s.status, s.last_error,
                  s.synced_at::TEXT, s.updated_at::TEXT,
                  b.player AS ban_player, b.steam_id AS ban_steam_id,
                  b.reason AS ban_reason, b.status AS ban_status
           FROM external_ban_syncs s
           JOIN external_ban_api_targets t ON t.id = s.target_id
           LEFT JOIN ban_records b ON b.id = s.local_ban_id
           WHERE s.local_ban_id = $1
           ORDER BY t.name ASC"#,
    )
    .bind(ban_id)
    .fetch_all(&db.pool)
    .await?;
    Ok(rows)
}

// ---------------------------------------------------------------------------
// Batch retry failed syncs
// ---------------------------------------------------------------------------

pub async fn retry_failed_syncs(db: &Database) -> anyhow::Result<ExternalBanSyncSummary> {
    // Find all failed sync records where the ban is still active
    #[derive(sqlx::FromRow)]
    struct FailedSync {
        local_ban_id: Uuid,
        target_id: Uuid,
    }

    let failed: Vec<FailedSync> = sqlx::query_as(
        r#"SELECT s.local_ban_id, s.target_id
           FROM external_ban_syncs s
           JOIN ban_records b ON b.id = s.local_ban_id
           WHERE s.status = 'failed' AND b.status = 'active'"#,
    )
    .fetch_all(&db.pool)
    .await?;

    if failed.is_empty() {
        return Ok(ExternalBanSyncSummary {
            ok: true,
            message: "没有可重试的失败同步".to_string(),
            items: vec![],
        });
    }

    // Group by ban, fetch bans and targets, then re-sync
    let mut all_items: Vec<ExternalBanSyncItem> = Vec::new();

    // Collect unique ban IDs
    let ban_ids: Vec<Uuid> = failed
        .iter()
        .map(|f| f.local_ban_id)
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();

    for ban_id in &ban_ids {
        let ban = match ban_service::get_ban(db, *ban_id).await {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!(ban_id = %ban_id, %e, "failed to fetch ban for retry");
                continue;
            }
        };

        let target_ids: Vec<Uuid> = failed
            .iter()
            .filter(|f| f.local_ban_id == *ban_id)
            .map(|f| f.target_id)
            .collect();

        let mut targets = Vec::new();
        for tid in &target_ids {
            if let Ok(t) = get_target_row(db, *tid).await {
                if t.enabled {
                    targets.push(t);
                }
            }
        }

        if targets.is_empty() {
            continue;
        }

        // 批量重置该封禁在所有目标上的同步行为 pending（原逐条 UPDATE）
        if !target_ids.is_empty() {
            if let Err(e) = sqlx::query(
                r#"UPDATE external_ban_syncs
                   SET status = 'pending', last_error = NULL, updated_at = now()
                   WHERE local_ban_id = $1 AND target_id = ANY($2)"#,
            )
            .bind(ban_id)
            .bind(&target_ids)
            .execute(&db.pool)
            .await
            {
                tracing::warn!(ban_id = %ban_id, %e, "failed to reset sync rows to pending");
            }
        }

        let summary = sync_ban_to_targets(db, &targets, &ban).await;
        all_items.extend(summary.items);
    }

    let ok_count = all_items.iter().filter(|item| item.ok).count();
    Ok(ExternalBanSyncSummary {
        ok: ok_count == all_items.len(),
        message: format!("重试完成：成功 {ok_count}/{}", all_items.len()),
        items: all_items,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_ban() -> BanItem {
        BanItem {
            id: Uuid::nil(),
            player: Some("Alice".to_string()),
            steam_id: "76561198000000000".to_string(),
            ip_address: Some("127.0.0.1".to_string()),
            server_name: Some("KZ Server".to_string()),
            ban_type: "steam".to_string(),
            duration_minutes: 0,
            expires_at: None,
            reason: "macro".to_string(),
            status: "active".to_string(),
            operator_name: "Admin".to_string(),
            source: "manual".to_string(),
            server_id: None,
            server_port: None,
            removed_reason: None,
            removed_by: None,
            removed_at: None,
            created_at: "2026-05-27T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn renders_external_notes_template() {
        let ban = sample_ban();
        let rendered = render_template(
            "{player}|{steam_id}|{reason}|{operator}|{server_name}",
            &ban,
        );
        assert_eq!(rendered, "Alice|76561198000000000|macro|Admin|KZ Server");
    }

    #[test]
    fn validates_external_ban_type() {
        assert!(normalize_ban_type("bhop_macro").is_ok());
        assert!(normalize_ban_type("steam").is_err());
    }
}

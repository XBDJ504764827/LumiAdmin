use crate::{
    config::Config,
    db::Database,
    routes::ListQuery,
    services::{
        ban_service::{self, BanItem, BanRow},
        plugin_ban_service::normalize_steam_id,
        steam_service::SteamResolver,
    },
};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct BanApiKeyPublic {
    pub id: Uuid,
    pub name: String,
    pub token_prefix: String,
    pub enabled: bool,
    pub last_used_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CreatedBanApiKey {
    pub key: BanApiKeyPublic,
    pub token: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct BanApiKeyAuth {
    pub id: Uuid,
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateBanApiKeyInput {
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct IntegrationCreateBanInput {
    pub player: Option<String>,
    pub steam_id: String,
    pub ban_type: String,
    pub ip_address: Option<String>,
    pub reason: String,
    pub duration_minutes: Option<i32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct IntegrationCheckInput {
    pub steam_id: Option<String>,
    pub ip_address: Option<String>,
}

#[derive(Serialize)]
pub struct IntegrationCheckResult {
    pub banned: bool,
    pub item: Option<BanItem>,
}

fn hash_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hex::encode(hasher.finalize())
}

fn generate_token() -> String {
    format!(
        "lumi_ban_{}{}",
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple()
    )
}

fn token_prefix(token: &str) -> String {
    token.chars().take(18).collect()
}

pub async fn list_keys(db: &Database) -> anyhow::Result<Vec<BanApiKeyPublic>> {
    sqlx::query_as::<_, BanApiKeyPublic>(
        r#"SELECT id, name, token_prefix, enabled, last_used_at, created_at
           FROM ban_api_keys
           ORDER BY created_at DESC"#,
    )
    .fetch_all(&db.pool)
    .await
    .map_err(Into::into)
}

pub async fn create_key(
    db: &Database,
    actor_id: Uuid,
    input: CreateBanApiKeyInput,
) -> anyhow::Result<CreatedBanApiKey> {
    let name = input.name.trim();
    anyhow::ensure!(!name.is_empty(), "API Key 名称不能为空");

    let token = generate_token();
    let row = sqlx::query_as::<_, BanApiKeyPublic>(
        r#"INSERT INTO ban_api_keys (id, name, token_hash, token_prefix, enabled, created_by)
           VALUES ($1, $2, $3, $4, true, $5)
           RETURNING id, name, token_prefix, enabled, last_used_at, created_at"#,
    )
    .bind(Uuid::new_v4())
    .bind(name)
    .bind(hash_token(&token))
    .bind(token_prefix(&token))
    .bind(actor_id)
    .fetch_one(&db.pool)
    .await?;

    Ok(CreatedBanApiKey { key: row, token })
}

pub async fn delete_key(db: &Database, id: Uuid) -> anyhow::Result<()> {
    let result = sqlx::query("DELETE FROM ban_api_keys WHERE id = $1")
        .bind(id)
        .execute(&db.pool)
        .await?;
    anyhow::ensure!(result.rows_affected() > 0, "API Key 不存在");
    Ok(())
}

/// 根据密钥 ID 获取密钥名称（用于日志记录）
pub async fn find_key_name(db: &Database, id: Uuid) -> Option<String> {
    let row: Option<(String,)> = sqlx::query_as(r#"SELECT name FROM ban_api_keys WHERE id = $1"#)
        .bind(id)
        .fetch_optional(&db.pool)
        .await
        .ok()?;

    row.map(|(name,)| name)
}

pub async fn authenticate_key(db: &Database, token: &str) -> anyhow::Result<BanApiKeyAuth> {
    let token = token.trim();
    anyhow::ensure!(!token.is_empty(), "API Key 不能为空");
    let key = sqlx::query_as::<_, BanApiKeyAuth>(
        r#"SELECT id, name
           FROM ban_api_keys
           WHERE token_hash = $1 AND enabled = true"#,
    )
    .bind(hash_token(token))
    .fetch_optional(&db.pool)
    .await?
    .ok_or_else(|| anyhow::anyhow!("API Key 无效"))?;

    sqlx::query("UPDATE ban_api_keys SET last_used_at = now() WHERE id = $1")
        .bind(key.id)
        .execute(&db.pool)
        .await?;

    Ok(key)
}

pub async fn list_integration_bans(
    db: &Database,
    query: &ListQuery,
) -> anyhow::Result<crate::routes::PaginatedResponse<ban_service::BanListItem>> {
    ban_service::list_bans(db, query).await
}

pub async fn create_integration_ban(
    db: &Database,
    config: &Config,
    key: &BanApiKeyAuth,
    input: IntegrationCreateBanInput,
) -> anyhow::Result<BanItem> {
    let ban_type = input.ban_type.trim();
    let reason = input.reason.trim();
    let duration_minutes = input.duration_minutes.unwrap_or(0);

    anyhow::ensure!(matches!(ban_type, "steam" | "ip"), "封禁属性无效");
    anyhow::ensure!(duration_minutes >= 0, "封禁时长不能为负数");
    anyhow::ensure!(!reason.is_empty(), "封禁理由不能为空");
    anyhow::ensure!(!input.steam_id.trim().is_empty(), "SteamID 不能为空");

    let steam_id = SteamResolver::new(config)
        .resolve(input.steam_id.trim())
        .await?
        .steamid64;
    let expires_at = if duration_minutes == 0 {
        None
    } else {
        Some(Utc::now() + Duration::minutes(i64::from(duration_minutes)))
    };

    let existing: Option<(Uuid,)> =
        sqlx::query_as(r#"SELECT id FROM ban_records WHERE steam_id = $1 AND status = 'active'"#)
            .bind(&steam_id)
            .fetch_optional(&db.pool)
            .await?;
    anyhow::ensure!(
        existing.is_none(),
        "该玩家已有活跃封禁记录，请先解封后再重新封禁"
    );

    let row = sqlx::query_as::<_, BanRow>(
        r#"INSERT INTO ban_records (
               id, player, steam_id, ip_address, server_name, ban_type,
               duration_minutes, expires_at, reason, status, operator_name, source,
               server_id, server_port
           )
           VALUES ($1, $2, $3, $4, NULL, $5, $6, $7, $8, 'active', $9, 'external_api', NULL, NULL)
           RETURNING id, player, steam_id, ip_address, server_name, ban_type,
                     duration_minutes, expires_at, reason, status, operator_name, source,
                     server_id, server_port, removed_reason, removed_by, removed_at, created_at"#,
    )
    .bind(Uuid::new_v4())
    .bind(super::normalize_optional_string(input.player))
    .bind(&steam_id)
    .bind(super::normalize_optional_string(input.ip_address))
    .bind(ban_type)
    .bind(duration_minutes)
    .bind(expires_at)
    .bind(reason)
    .bind(format!("API: {}", key.name))
    .fetch_one(&db.pool)
    .await?;

    Ok(ban_service::row_to_item(row))
}

pub async fn check_integration_ban(
    db: &Database,
    input: IntegrationCheckInput,
) -> anyhow::Result<IntegrationCheckResult> {
    let steam_id =
        super::normalize_optional_string(input.steam_id).map(|value| normalize_steam_id(&value));
    let ip_address = super::normalize_optional_string(input.ip_address);
    anyhow::ensure!(
        steam_id.is_some() || ip_address.is_some(),
        "SteamID 或 IP 至少填写一项"
    );

    let row = sqlx::query_as::<_, BanRow>(
        r#"SELECT id, player, steam_id, ip_address, server_name, ban_type,
                  duration_minutes, expires_at, reason, status, operator_name, source,
                  server_id, server_port, removed_reason, removed_by, removed_at, created_at
           FROM ban_records
           WHERE status = 'active'
             AND (expires_at IS NULL OR expires_at > now())
             AND (($1::TEXT IS NOT NULL AND steam_id = $1) OR ($2::TEXT IS NOT NULL AND ip_address = $2))
           ORDER BY created_at DESC
           LIMIT 1"#,
    )
    .bind(steam_id.as_deref())
    .bind(ip_address.as_deref())
    .fetch_optional(&db.pool)
    .await?;

    Ok(IntegrationCheckResult {
        banned: row.is_some(),
        item: row.map(ban_service::row_to_item),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_token_has_expected_prefix() {
        let token = generate_token();
        assert!(token.starts_with("lumi_ban_"));
        assert_eq!(token_prefix(&token).len(), 18);
    }

    #[test]
    fn token_hash_is_stable() {
        assert_eq!(hash_token("abc"), hash_token("abc"));
        assert_ne!(hash_token("abc"), hash_token("def"));
    }
}

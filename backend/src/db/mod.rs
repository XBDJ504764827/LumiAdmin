use sqlx::{postgres::PgPoolOptions, PgPool};
use std::time::Duration;

use crate::config::Config;

mod ban_appeals;
mod ban_records;
mod core;
mod external;
mod indexes;
mod logs;
mod map_sync;
mod notifications;
mod player_api;
mod player_reports;
mod servers;
mod user_schema;
mod utils;
mod whitelist;

#[cfg(test)]
mod tests;

#[derive(Clone)]
pub struct Database {
    pub pool: PgPool,
}

impl Database {
    pub async fn connect(database_url: &str, config: &Config) -> anyhow::Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(config.db_max_connections)
            .min_connections(config.db_min_connections)
            .acquire_timeout(Duration::from_secs(config.db_acquire_timeout_secs))
            .idle_timeout(Duration::from_secs(config.db_idle_timeout_secs))
            .connect(database_url)
            .await
            .map_err(|e| anyhow::anyhow!("database connect failed: {e}"))?;

        Ok(Self { pool })
    }

    /// 测试用连接方法，使用默认配置
    #[cfg(test)]
    pub async fn connect_for_test(database_url: &str) -> anyhow::Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(1)
            .acquire_timeout(Duration::from_secs(10))
            .connect(database_url)
            .await
            .map_err(|e| anyhow::anyhow!("database connect failed: {e}"))?;

        Ok(Self { pool })
    }

    pub async fn migrate(&self) -> anyhow::Result<()> {
        self.migrate_core_tables().await?;
        self.migrate_ban_records_schema().await?;
        self.migrate_users_and_communities_schema().await?;
        self.migrate_servers_schema().await?;
        self.migrate_player_api_schema().await?;
        self.migrate_server_data().await?;
        self.migrate_whitelist_schema().await?;
        self.migrate_logs_operations_and_indexes().await?;
        self.migrate_external_servers_schema().await?;
        self.migrate_external_ban_api_schema().await?;
        self.migrate_ban_api_keys_schema().await?;
        self.migrate_map_tiers_table().await?;
        self.migrate_map_sync_schema().await?;
        self.migrate_notifications_schema().await?;
        self.migrate_ban_appeals_schema().await?;
        self.migrate_appeal_files_schema().await?;
        self.migrate_ban_files_schema().await?;
        self.migrate_player_reports_schema().await?;
        self.migrate_player_internal_notes_schema().await?;
        self.migrate_adds_missing_constraints_and_indexes().await?;
        self.migrate_player_access_cache_extended().await?;
        Ok(())
    }

    pub async fn seed(&self, config: &Config) -> anyhow::Result<()> {
        let password_hash = crate::password::hash_password(&config.dev_password)?;
        sqlx::query(
            r#"INSERT INTO users (id, username, display_name, password_hash, role, steam_id, remark)
               VALUES
               ('22222222-2222-2222-2222-222222222222', $1, 'DevAdmin', $2, 'developer', '76561198000000000', '开发管理员')
               ON CONFLICT (username) DO UPDATE SET password_hash = $2"#,
        )
        .bind(&config.dev_username)
        .bind(&password_hash)
        .execute(&self.pool)
        .await?;

        // 修复所有存储为明文的密码（非 $argon2 开头的）
        let rows: Vec<(uuid::Uuid, String)> = sqlx::query_as(
            "SELECT id, password_hash FROM users WHERE password_hash NOT LIKE '$argon2%'",
        )
        .fetch_all(&self.pool)
        .await?;

        for (user_id, plain) in &rows {
            let hashed = crate::password::hash_password(plain)?;
            sqlx::query("UPDATE users SET password_hash = $1 WHERE id = $2")
                .bind(&hashed)
                .bind(user_id)
                .execute(&self.pool)
                .await?;
        }
        if !rows.is_empty() {
            tracing::info!(
                count = rows.len(),
                "migrated plaintext passwords to argon2 hashes"
            );
        }

        sqlx::query(
            r#"UPDATE map_sync_config
               SET map_pool_url = $1
               WHERE id = true
                 AND (
                   btrim(map_pool_url) = ''
                   OR map_pool_url = 'https://kztimerglobal.com/api/v1.0/maps?is_validated=true&limit=999'
                 )"#,
        )
        .bind(&config.map_sync_map_pool_url)
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}

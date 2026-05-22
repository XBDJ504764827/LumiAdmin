use crate::db::Database;

/// 启动封禁过期检查循环
/// 每隔 interval_seconds 秒检查一次过期的封禁记录并自动解封
pub fn start_expiry_loop(db: Database, interval_seconds: u64) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(interval_seconds));
        loop {
            interval.tick().await;
            match process_expired_bans(&db).await {
                Ok(count) => {
                    if count > 0 {
                        tracing::info!(count, "自动解封过期封禁记录");
                    }
                }
                Err(error) => {
                    tracing::warn!(%error, "处理过期封禁记录失败");
                }
            }
        }
    });
}

/// 处理所有已过期的封禁记录
/// 返回解封的记录数量
pub async fn process_expired_bans(db: &Database) -> anyhow::Result<usize> {
    let result = sqlx::query(
        r#"UPDATE ban_records
           SET status = 'inactive',
               removed_reason = '临时封禁已到期，自动解封',
               removed_by = 'system',
               removed_at = now()
           WHERE status = 'active'
             AND expires_at IS NOT NULL
             AND expires_at <= now()"#,
    )
    .execute(&db.pool)
    .await?;

    Ok(result.rows_affected() as usize)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::Config, db::Database};
    use chrono::Duration;
    use sqlx::postgres::PgPoolOptions;

    fn schema_url(base_url: &str, schema: &str) -> String {
        let separator = if base_url.contains('?') { '&' } else { '?' };
        format!("{base_url}{separator}options=-csearch_path%3D{schema}")
    }

    async fn create_schema(base_url: &str, schema: &str) {
        let pool = PgPoolOptions::new().max_connections(1).connect(base_url).await.unwrap();
        sqlx::query(&format!(r#"CREATE SCHEMA "{schema}""#)).execute(&pool).await.unwrap();
        pool.close().await;
    }

    async fn drop_schema(base_url: &str, schema: &str) {
        let pool = PgPoolOptions::new().max_connections(1).connect(base_url).await.unwrap();
        sqlx::query(&format!(r#"DROP SCHEMA IF EXISTS "{schema}" CASCADE"#))
            .execute(&pool)
            .await
            .unwrap();
        pool.close().await;
    }

    #[tokio::test]
    async fn process_expired_bans_updates_expired_records() {
        let config = Config::from_env();
        let base_url = config.database_url.clone();
        let schema = format!("test_{}", Uuid::new_v4().simple());
        let scoped_url = schema_url(&base_url, &schema);

        create_schema(&base_url, &schema).await;

        let result = async {
            let db = Database::connect(&scoped_url).await?;
            db.migrate().await?;

            // 插入一条已过期的封禁记录
            let expired_id = Uuid::new_v4();
            sqlx::query(
                r#"INSERT INTO ban_records (
                    id, steam_id, ban_type, duration_minutes, expires_at,
                    reason, status, operator_name, source, created_at
                ) VALUES ($1, $2, $3, $4, $5, $6, 'active', $7, 'game_plugin', now())"#,
            )
            .bind(expired_id)
            .bind("76561198000000001")
            .bind("steam")
            .bind(30_i32)
            .bind(Utc::now() - Duration::minutes(5)) // 5 分钟前过期
            .bind("测试封禁")
            .bind("admin")
            .execute(&db.pool)
            .await?;

            // 插入一条未过期的封禁记录
            let active_id = Uuid::new_v4();
            sqlx::query(
                r#"INSERT INTO ban_records (
                    id, steam_id, ban_type, duration_minutes, expires_at,
                    reason, status, operator_name, source, created_at
                ) VALUES ($1, $2, $3, $4, $5, $6, 'active', $7, 'game_plugin', now())"#,
            )
            .bind(active_id)
            .bind("76561198000000002")
            .bind("steam")
            .bind(30_i32)
            .bind(Utc::now() + Duration::minutes(30)) // 30 分钟后过期
            .bind("测试封禁")
            .bind("admin")
            .execute(&db.pool)
            .await?;

            // 插入一条永久封禁记录
            let permanent_id = Uuid::new_v4();
            sqlx::query(
                r#"INSERT INTO ban_records (
                    id, steam_id, ban_type, duration_minutes, expires_at,
                    reason, status, operator_name, source, created_at
                ) VALUES ($1, $2, $3, 0, NULL, $4, 'active', $5, 'manual', now())"#,
            )
            .bind(permanent_id)
            .bind("76561198000000003")
            .bind("steam")
            .bind("永久封禁")
            .bind("admin")
            .execute(&db.pool)
            .await?;

            // 执行过期处理
            let count = process_expired_bans(&db).await?;
            assert_eq!(count, 1); // 只有 1 条过期

            // 验证过期记录已被解封
            let expired_status: (String, Option<String>, Option<String>) = sqlx::query_as(
                "SELECT status, removed_reason, removed_by FROM ban_records WHERE id = $1",
            )
            .bind(expired_id)
            .fetch_one(&db.pool)
            .await?;
            assert_eq!(expired_status.0, "inactive");
            assert_eq!(expired_status.1, Some("临时封禁已到期，自动解封".to_string()));
            assert_eq!(expired_status.2, Some("system".to_string()));

            // 验证未过期记录仍为 active
            let active_status: (String,) = sqlx::query_as(
                "SELECT status FROM ban_records WHERE id = $1",
            )
            .bind(active_id)
            .fetch_one(&db.pool)
            .await?;
            assert_eq!(active_status.0, "active");

            // 验证永久封禁记录仍为 active
            let permanent_status: (String,) = sqlx::query_as(
                "SELECT status FROM ban_records WHERE id = $1",
            )
            .bind(permanent_id)
            .fetch_one(&db.pool)
            .await?;
            assert_eq!(permanent_status.0, "active");

            Ok::<(), anyhow::Error>(())
        }
        .await;

        drop_schema(&base_url, &schema).await;
        result.unwrap();
    }

    #[tokio::test]
    async fn process_expired_bans_returns_zero_when_no_expired() {
        let config = Config::from_env();
        let base_url = config.database_url.clone();
        let schema = format!("test_{}", Uuid::new_v4().simple());
        let scoped_url = schema_url(&base_url, &schema);

        create_schema(&base_url, &schema).await;

        let result = async {
            let db = Database::connect(&scoped_url).await?;
            db.migrate().await?;

            let count = process_expired_bans(&db).await?;
            assert_eq!(count, 0);

            Ok::<(), anyhow::Error>(())
        }
        .await;

        drop_schema(&base_url, &schema).await;
        result.unwrap();
    }
}
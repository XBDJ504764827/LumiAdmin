use crate::{db::Database, services::observability_service, services::whitelist_service};

/// 启动白名单过期检查循环
/// 每隔 interval_seconds 秒检查一次已到期的白名单并置为 expired
pub fn start_expiry_loop(db: Database, interval_seconds: u64) {
    observability_service::register_task(
        "whitelist_expiry",
        "白名单期限到期自动过期",
        "白名单",
        Some(interval_seconds),
        true,
    );
    super::task_runtime::spawn_persistent("whitelist_expiry", move || {
        let db = db.clone();
        async move {
            let mut interval =
                tokio::time::interval(std::time::Duration::from_secs(interval_seconds));
            loop {
                interval.tick().await;
                match observability_service::observe_task(
                    "whitelist_expiry",
                    process_expired_whitelist(&db),
                    |count| format!("本轮过期 {} 条", count),
                )
                .await
                {
                    Ok(count) => {
                        if count > 0 {
                            tracing::info!(count, "白名单期限到期已自动过期");
                        }
                    }
                    Err(error) => {
                        tracing::warn!(%error, "处理到期白名单失败");
                    }
                }
            }
        }
    });
}

/// 处理所有已到期的白名单记录（复用 whitelist_service 的原子更新）
/// 返回过期处理的记录数量
async fn process_expired_whitelist(db: &Database) -> anyhow::Result<usize> {
    whitelist_service::process_expired_whitelist(db).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::Config, db::Database, test_util};
    use chrono::{Duration, Utc};
    use sqlx::Row;
    use uuid::Uuid;

    async fn with_test_db(test: impl AsyncFnOnce(Database) -> anyhow::Result<()>) {
        let config = Config::from_env();
        let base_url = config.database_url;
        let schema = format!("test_{}", Uuid::new_v4().simple());
        let scoped_url = test_util::schema_url(&base_url, &schema);
        test_util::create_schema(&base_url, &schema).await;

        let result = async {
            let db = Database::connect_for_test(&scoped_url).await?;
            db.migrate().await?;
            test(db).await
        }
        .await;

        test_util::drop_schema(&base_url, &schema).await;
        result.unwrap();
    }

    async fn insert_approved_with_expiry(
        db: &Database,
        steamid64: &str,
        expires_at: Option<chrono::DateTime<Utc>>,
    ) -> Uuid {
        let id = Uuid::new_v4();
        sqlx::query(
            r#"INSERT INTO whitelist_requests (
                id, steam_id, steamid64, nickname, status,
                applied_at, approved_at, approved_by, source, updated_at, expires_at, duration_days
            ) VALUES ($1, $2, $3, '期限玩家', 'approved', now(), now(), '管理员A', 'public', now(), $4, $5)"#,
        )
        .bind(id)
        .bind(steamid64)
        .bind(steamid64)
        .bind(expires_at)
        .bind(expires_at.map(|_| 7_i32))
        .execute(&db.pool)
        .await
        .unwrap();
        id
    }

    #[tokio::test]
    async fn process_expired_whitelist_updates_expired_records() {
        with_test_db(async |db| {
            let expired_id =
                insert_approved_with_expiry(&db, "76561198000000071", Some(Utc::now() - Duration::minutes(5)))
                    .await;
            let active_id =
                insert_approved_with_expiry(&db, "76561198000000072", Some(Utc::now() + Duration::days(7)))
                    .await;
            let permanent_id = insert_approved_with_expiry(&db, "76561198000000073", None).await;

            let count = process_expired_whitelist(&db).await?;
            assert_eq!(count, 1);

            let row = sqlx::query("SELECT status, expired_at FROM whitelist_requests WHERE id = $1")
                .bind(expired_id)
                .fetch_one(&db.pool)
                .await?;
            assert_eq!(row.get::<String, _>("status"), "expired");
            assert!(row.get::<Option<chrono::DateTime<Utc>>, _>("expired_at").is_some());

            let row = sqlx::query("SELECT status FROM whitelist_requests WHERE id = $1")
                .bind(active_id)
                .fetch_one(&db.pool)
                .await?;
            assert_eq!(row.get::<String, _>("status"), "approved");

            let row = sqlx::query("SELECT status FROM whitelist_requests WHERE id = $1")
                .bind(permanent_id)
                .fetch_one(&db.pool)
                .await?;
            assert_eq!(row.get::<String, _>("status"), "approved");
            Ok(())
        })
        .await;
    }

    #[tokio::test]
    async fn process_expired_whitelist_returns_zero_when_no_expired() {
        with_test_db(async |db| {
            let count = process_expired_whitelist(&db).await?;
            assert_eq!(count, 0);
            Ok(())
        })
        .await;
    }
}
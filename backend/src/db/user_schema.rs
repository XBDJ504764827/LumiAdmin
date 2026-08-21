use super::Database;

impl Database {
    pub(super) async fn migrate_users_and_communities_schema(&self) -> anyhow::Result<()> {
        let alters = [
            r#"ALTER TABLE users ADD COLUMN IF NOT EXISTS steam_id TEXT"#,
            r#"ALTER TABLE users ADD COLUMN IF NOT EXISTS remark TEXT"#,
            r#"ALTER TABLE users ADD COLUMN IF NOT EXISTS enabled BOOLEAN NOT NULL DEFAULT true"#,
            r#"ALTER TABLE users ADD COLUMN IF NOT EXISTS openid TEXT"#,
            r#"ALTER TABLE users ADD COLUMN IF NOT EXISTS whitelist_notification_enabled BOOLEAN NOT NULL DEFAULT true"#,
            // 若历史库中存在旧字段 qq_account：
            //  - 当 openid 不存在时，直接改名迁移；
            //  - 当 openid 已存在时，说明迁移已部分完成，直接丢弃遗留的 qq_account。
            r#"DO $$
                BEGIN
                    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_schema = current_schema() AND table_name='users' AND column_name='qq_account') THEN
                        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_schema = current_schema() AND table_name='users' AND column_name='openid') THEN
                            ALTER TABLE users RENAME COLUMN qq_account TO openid;
                        ELSE
                            ALTER TABLE users DROP COLUMN qq_account;
                        END IF;
                    END IF;
                END
            $$
            "#,
            r#"ALTER TABLE communities ADD COLUMN IF NOT EXISTS created_by UUID"#,
        ];
        for sql in alters {
            sqlx::query(sql).execute(&self.pool).await?;
        }
        Ok(())
    }
}

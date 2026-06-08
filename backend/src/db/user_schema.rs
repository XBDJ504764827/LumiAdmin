use super::Database;

impl Database {
    pub(super) async fn migrate_users_and_communities_schema(&self) -> anyhow::Result<()> {
        let alters = [
            r#"ALTER TABLE users ADD COLUMN IF NOT EXISTS steam_id TEXT"#,
            r#"ALTER TABLE users ADD COLUMN IF NOT EXISTS remark TEXT"#,
            r#"ALTER TABLE users ADD COLUMN IF NOT EXISTS enabled BOOLEAN NOT NULL DEFAULT true"#,
            r#"ALTER TABLE communities ADD COLUMN IF NOT EXISTS created_by UUID"#,
        ];
        for sql in alters {
            sqlx::query(sql).execute(&self.pool).await?;
        }
        Ok(())
    }
}

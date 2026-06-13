use super::Database;

impl Database {
pub(super) async fn migrate_ban_records_schema(&self) -> anyhow::Result<()> {
    let alters = [
        r#"ALTER TABLE ban_records ADD COLUMN IF NOT EXISTS player TEXT"#,
        r#"ALTER TABLE ban_records ADD COLUMN IF NOT EXISTS ip_address TEXT"#,
        r#"ALTER TABLE ban_records ADD COLUMN IF NOT EXISTS server_name TEXT"#,
        r#"ALTER TABLE ban_records ADD COLUMN IF NOT EXISTS ban_type TEXT NOT NULL DEFAULT 'steam'"#,
        r#"ALTER TABLE ban_records ADD COLUMN IF NOT EXISTS reason TEXT NOT NULL DEFAULT '未填写'"#,
        r#"ALTER TABLE ban_records ADD COLUMN IF NOT EXISTS duration_minutes INTEGER NOT NULL DEFAULT 0"#,
        r#"ALTER TABLE ban_records ADD COLUMN IF NOT EXISTS expires_at TIMESTAMPTZ"#,
        r#"ALTER TABLE ban_records ADD COLUMN IF NOT EXISTS source TEXT NOT NULL DEFAULT 'manual'"#,
        r#"ALTER TABLE ban_records ADD COLUMN IF NOT EXISTS server_id UUID"#,
        r#"ALTER TABLE ban_records ADD COLUMN IF NOT EXISTS server_port INTEGER"#,
        r#"ALTER TABLE ban_records ADD COLUMN IF NOT EXISTS removed_reason TEXT"#,
        r#"ALTER TABLE ban_records ADD COLUMN IF NOT EXISTS removed_by TEXT"#,
        r#"ALTER TABLE ban_records ADD COLUMN IF NOT EXISTS removed_at TIMESTAMPTZ"#,
        r#"ALTER TABLE ban_records ALTER COLUMN player DROP NOT NULL"#,
        r#"ALTER TABLE ban_records ALTER COLUMN ip_address DROP NOT NULL"#,
        r#"ALTER TABLE ban_records ALTER COLUMN server_name DROP NOT NULL"#,
    ];
    for sql in alters {
        sqlx::query(sql).execute(&self.pool).await?;
    }
    Ok(())
}

/// users 和 communities 补充列
pub(super) async fn migrate_ban_files_schema(&self) -> anyhow::Result<()> {
    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS ban_files (
          id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
          ban_id UUID NOT NULL REFERENCES ban_records(id) ON DELETE CASCADE,
          file_name TEXT NOT NULL,
          file_size BIGINT NOT NULL,
          content_type TEXT NOT NULL,
          storage_key TEXT NOT NULL,
          category TEXT NOT NULL,
          uploaded_by TEXT NOT NULL,
          uploaded_at TIMESTAMPTZ NOT NULL DEFAULT now()
        )"#,
    )
    .execute(&self.pool)
    .await?;

    sqlx::query(r#"CREATE INDEX IF NOT EXISTS idx_ban_files_ban_id ON ban_files (ban_id)"#)
        .execute(&self.pool)
        .await?;
    sqlx::query(r#"ALTER TABLE ban_files ADD COLUMN IF NOT EXISTS tags TEXT[] NOT NULL DEFAULT ARRAY[]::TEXT[]"#)
        .execute(&self.pool)
        .await?;
    sqlx::query(r#"ALTER TABLE ban_files ADD COLUMN IF NOT EXISTS note TEXT"#)
        .execute(&self.pool)
        .await?;
    Ok(())
}

}

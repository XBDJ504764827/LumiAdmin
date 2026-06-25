use super::Database;

impl Database {
    pub(super) async fn migrate_ban_appeals_schema(&self) -> anyhow::Result<()> {
        sqlx::query(
            r#"CREATE TABLE IF NOT EXISTS ban_appeals (
          id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
          ban_id UUID NOT NULL REFERENCES ban_records(id) ON DELETE CASCADE,
          steam_id TEXT NOT NULL,
          player_name TEXT NOT NULL,
          appeal_reason TEXT NOT NULL,
          status TEXT NOT NULL DEFAULT 'pending',
          reviewed_by TEXT,
          review_note TEXT,
          reviewed_at TIMESTAMPTZ,
          created_at TIMESTAMPTZ NOT NULL DEFAULT now()
        )"#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(r#"CREATE INDEX IF NOT EXISTS idx_ban_appeals_status ON ban_appeals (status)"#)
            .execute(&self.pool)
            .await?;
        sqlx::query(
            r#"CREATE INDEX IF NOT EXISTS idx_ban_appeals_steam_id ON ban_appeals (steam_id)"#,
        )
        .execute(&self.pool)
        .await?;
        sqlx::query(
        r#"CREATE INDEX IF NOT EXISTS idx_ban_appeals_created_at ON ban_appeals (created_at DESC)"#,
    ).execute(&self.pool).await?;
        sqlx::query(
        r#"ALTER TABLE ban_appeals ADD COLUMN IF NOT EXISTS evidence_paths TEXT[] NOT NULL DEFAULT ARRAY[]::TEXT[]"#,
    ).execute(&self.pool).await?;
        sqlx::query(r#"ALTER TABLE ban_appeals ADD COLUMN IF NOT EXISTS upload_token_hash TEXT"#)
            .execute(&self.pool)
            .await?;
        sqlx::query(r#"ALTER TABLE ban_appeals ADD COLUMN IF NOT EXISTS contact TEXT"#)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub(super) async fn migrate_appeal_files_schema(&self) -> anyhow::Result<()> {
        sqlx::query(
            r#"CREATE TABLE IF NOT EXISTS appeal_files (
          id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
          appeal_id UUID NOT NULL REFERENCES ban_appeals(id) ON DELETE CASCADE,
          file_name TEXT NOT NULL,
          file_size BIGINT NOT NULL,
          content_type TEXT NOT NULL,
          storage_key TEXT NOT NULL,
          category TEXT NOT NULL,
          uploaded_at TIMESTAMPTZ NOT NULL DEFAULT now()
        )"#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"CREATE INDEX IF NOT EXISTS idx_appeal_files_appeal_id ON appeal_files (appeal_id)"#,
        )
        .execute(&self.pool)
        .await?;
        sqlx::query(r#"ALTER TABLE appeal_files ADD COLUMN IF NOT EXISTS tags TEXT[] NOT NULL DEFAULT ARRAY[]::TEXT[]"#)
        .execute(&self.pool)
        .await?;
        sqlx::query(r#"ALTER TABLE appeal_files ADD COLUMN IF NOT EXISTS note TEXT"#)
            .execute(&self.pool)
            .await?;
        sqlx::query(r#"ALTER TABLE appeal_files ADD COLUMN IF NOT EXISTS uploaded_by TEXT"#)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

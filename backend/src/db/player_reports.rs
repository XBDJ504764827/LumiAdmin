use super::Database;

impl Database {
    pub(super) async fn migrate_player_reports_schema(&self) -> anyhow::Result<()> {
        sqlx::query(
            r#"CREATE TABLE IF NOT EXISTS player_reports (
          id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
          target_steam_id TEXT NOT NULL,
          target_player_name TEXT,
          reporter_contact TEXT,
          report_reason TEXT NOT NULL,
          status TEXT NOT NULL DEFAULT 'pending',
          reviewed_by TEXT,
          review_note TEXT,
          reviewed_at TIMESTAMPTZ,
          upload_token_hash TEXT,
          created_at TIMESTAMPTZ NOT NULL DEFAULT now()
        )"#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"CREATE TABLE IF NOT EXISTS player_report_files (
          id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
          report_id UUID NOT NULL REFERENCES player_reports(id) ON DELETE CASCADE,
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
            r#"CREATE INDEX IF NOT EXISTS idx_player_reports_status_created
           ON player_reports (status, created_at DESC)"#,
        )
        .execute(&self.pool)
        .await?;
        sqlx::query(
            r#"CREATE INDEX IF NOT EXISTS idx_player_reports_steam_id
           ON player_reports (target_steam_id)"#,
        )
        .execute(&self.pool)
        .await?;
        sqlx::query(
            r#"CREATE INDEX IF NOT EXISTS idx_player_report_files_report_id
           ON player_report_files (report_id)"#,
        )
        .execute(&self.pool)
        .await?;
        sqlx::query(r#"ALTER TABLE player_report_files ADD COLUMN IF NOT EXISTS tags TEXT[] NOT NULL DEFAULT ARRAY[]::TEXT[]"#)
        .execute(&self.pool)
        .await?;
        sqlx::query(r#"ALTER TABLE player_report_files ADD COLUMN IF NOT EXISTS note TEXT"#)
            .execute(&self.pool)
            .await?;
        sqlx::query(r#"ALTER TABLE player_report_files ADD COLUMN IF NOT EXISTS uploaded_by TEXT"#)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub(super) async fn migrate_player_internal_notes_schema(&self) -> anyhow::Result<()> {
        sqlx::query(
            r#"CREATE TABLE IF NOT EXISTS player_internal_notes (
          steamid64 TEXT PRIMARY KEY,
          note TEXT NOT NULL DEFAULT '',
          tags TEXT[] NOT NULL DEFAULT ARRAY[]::TEXT[],
          updated_by TEXT,
          updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
          created_at TIMESTAMPTZ NOT NULL DEFAULT now()
        )"#,
        )
        .execute(&self.pool)
        .await?;
        sqlx::query(
            r#"CREATE INDEX IF NOT EXISTS idx_player_internal_notes_updated_at
           ON player_internal_notes (updated_at DESC)"#,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

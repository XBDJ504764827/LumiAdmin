use super::Database;

impl Database {
    pub(super) async fn migrate_map_feedback_schema(&self) -> anyhow::Result<()> {
        sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS map_feedback (
          id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
          feedback_type TEXT NOT NULL,
          steam_id TEXT,
          steam_persona_name TEXT,
          contact TEXT,
          detail TEXT NOT NULL,
          status TEXT NOT NULL DEFAULT 'pending',
          reviewed_by TEXT,
          review_note TEXT,
          reviewed_at TIMESTAMPTZ,
          created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
          CONSTRAINT map_feedback_type_check CHECK (feedback_type IN ('missing', 'broken', 'request')),
          CONSTRAINT map_feedback_status_check CHECK (status IN ('pending', 'resolved', 'rejected'))
        )"#,
    )
    .execute(&self.pool)
    .await?;

        sqlx::query(
            r#"CREATE INDEX IF NOT EXISTS idx_map_feedback_status_created
           ON map_feedback (status, created_at DESC)"#,
        )
        .execute(&self.pool)
        .await?;
        sqlx::query(
            r#"CREATE INDEX IF NOT EXISTS idx_map_feedback_steam_id
           ON map_feedback (steam_id)"#,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

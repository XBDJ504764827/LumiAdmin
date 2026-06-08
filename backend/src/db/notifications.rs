use super::Database;

impl Database {
pub(super) async fn migrate_notifications_schema(&self) -> anyhow::Result<()> {
    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS notifications (
          id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
          user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
          type TEXT NOT NULL,
          title TEXT NOT NULL,
          message TEXT NOT NULL,
          link TEXT,
          read BOOLEAN NOT NULL DEFAULT false,
          created_at TIMESTAMPTZ NOT NULL DEFAULT now()
        )"#,
    )
    .execute(&self.pool)
    .await?;

    sqlx::query(
        r#"CREATE INDEX IF NOT EXISTS idx_notifications_user_unread
           ON notifications (user_id, read, created_at DESC) WHERE read = false"#,
    )
    .execute(&self.pool)
    .await?;

    sqlx::query(
        r#"CREATE INDEX IF NOT EXISTS idx_notifications_created_at
           ON notifications (created_at)"#,
    )
    .execute(&self.pool)
    .await?;

    Ok(())
}

}

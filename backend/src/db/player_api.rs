use super::Database;

impl Database {
pub(super) async fn migrate_player_api_schema(&self) -> anyhow::Result<()> {
    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS player_api_config (
          id BOOLEAN PRIMARY KEY DEFAULT true,
          max_api_count INTEGER NOT NULL DEFAULT 3,
          interval_seconds INTEGER NOT NULL DEFAULT 30,
          updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
          CONSTRAINT player_api_config_single_row CHECK (id)
        )"#,
    )
    .execute(&self.pool)
    .await?;

    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS player_api_webhooks (
          id UUID PRIMARY KEY,
          public_path TEXT NOT NULL DEFAULT '',
          webhook_url TEXT NOT NULL DEFAULT '',
          secret TEXT,
          server_ids UUID[] NOT NULL DEFAULT ARRAY[]::UUID[],
          last_status TEXT,
          last_error TEXT,
          last_dispatched_at TIMESTAMPTZ,
          created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
          updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
        )"#,
    )
    .execute(&self.pool)
    .await?;

    let alters = [
        r#"ALTER TABLE player_api_webhooks ADD COLUMN IF NOT EXISTS public_path TEXT NOT NULL DEFAULT ''"#,
        r#"ALTER TABLE player_api_config ADD COLUMN IF NOT EXISTS max_api_count INTEGER NOT NULL DEFAULT 3"#,
        r#"ALTER TABLE player_api_config ADD COLUMN IF NOT EXISTS interval_seconds INTEGER NOT NULL DEFAULT 30"#,
        r#"ALTER TABLE player_api_config ADD COLUMN IF NOT EXISTS updated_at TIMESTAMPTZ NOT NULL DEFAULT now()"#,
        r#"ALTER TABLE player_api_webhooks ADD COLUMN IF NOT EXISTS secret TEXT"#,
        r#"ALTER TABLE player_api_webhooks ADD COLUMN IF NOT EXISTS server_ids UUID[] NOT NULL DEFAULT ARRAY[]::UUID[]"#,
        r#"ALTER TABLE player_api_webhooks ADD COLUMN IF NOT EXISTS last_status TEXT"#,
        r#"ALTER TABLE player_api_webhooks ADD COLUMN IF NOT EXISTS last_error TEXT"#,
        r#"ALTER TABLE player_api_webhooks ADD COLUMN IF NOT EXISTS last_dispatched_at TIMESTAMPTZ"#,
    ];
    for sql in alters {
        sqlx::query(sql).execute(&self.pool).await?;
    }

    sqlx::query(
        r#"INSERT INTO player_api_config (id, max_api_count, interval_seconds)
           VALUES (true, 3, 30)
           ON CONFLICT (id) DO NOTHING"#,
    )
    .execute(&self.pool)
    .await?;

    Ok(())
}

}

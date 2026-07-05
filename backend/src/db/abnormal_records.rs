use super::Database;

impl Database {
    pub(super) async fn migrate_abnormal_records_schema(&self) -> anyhow::Result<()> {
        sqlx::query(
            r#"CREATE TABLE IF NOT EXISTS abnormal_record_rules (
          id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
          map_name TEXT NOT NULL,
          course INTEGER NOT NULL DEFAULT 0,
          mode TEXT,
          time_type TEXT,
          threshold_seconds REAL NOT NULL,
          enabled BOOLEAN NOT NULL DEFAULT true,
          note TEXT,
          created_by TEXT,
          updated_by TEXT,
          created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
          updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
        )"#,
        )
        .execute(&self.pool)
        .await?;

        let rule_alters = [
            r#"ALTER TABLE abnormal_record_rules ADD COLUMN IF NOT EXISTS course INTEGER NOT NULL DEFAULT 0"#,
            r#"ALTER TABLE abnormal_record_rules ADD COLUMN IF NOT EXISTS mode TEXT"#,
            r#"ALTER TABLE abnormal_record_rules ADD COLUMN IF NOT EXISTS time_type TEXT"#,
            r#"ALTER TABLE abnormal_record_rules ADD COLUMN IF NOT EXISTS threshold_seconds REAL NOT NULL DEFAULT 5"#,
            r#"ALTER TABLE abnormal_record_rules ADD COLUMN IF NOT EXISTS enabled BOOLEAN NOT NULL DEFAULT true"#,
            r#"ALTER TABLE abnormal_record_rules ADD COLUMN IF NOT EXISTS note TEXT"#,
            r#"ALTER TABLE abnormal_record_rules ADD COLUMN IF NOT EXISTS created_by TEXT"#,
            r#"ALTER TABLE abnormal_record_rules ADD COLUMN IF NOT EXISTS updated_by TEXT"#,
            r#"ALTER TABLE abnormal_record_rules ADD COLUMN IF NOT EXISTS created_at TIMESTAMPTZ NOT NULL DEFAULT now()"#,
            r#"ALTER TABLE abnormal_record_rules ADD COLUMN IF NOT EXISTS updated_at TIMESTAMPTZ NOT NULL DEFAULT now()"#,
        ];
        for sql in rule_alters {
            sqlx::query(sql).execute(&self.pool).await?;
        }

        sqlx::query(
            r#"CREATE UNIQUE INDEX IF NOT EXISTS idx_abnormal_record_rules_unique_scope
           ON abnormal_record_rules (
             lower(map_name),
             course,
             COALESCE(lower(mode), ''),
             COALESCE(lower(time_type), '')
           )"#,
        )
        .execute(&self.pool)
        .await?;
        sqlx::query(
            r#"CREATE INDEX IF NOT EXISTS idx_abnormal_record_rules_lookup
           ON abnormal_record_rules (lower(map_name), course, enabled)"#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"CREATE TABLE IF NOT EXISTS abnormal_records (
          id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
          idempotency_key TEXT UNIQUE NOT NULL,
          server_id UUID REFERENCES servers(id) ON DELETE SET NULL,
          server_name TEXT,
          server_port INTEGER,
          steam_id64 TEXT NOT NULL,
          steam_id2 TEXT,
          player_name TEXT,
          map_name TEXT NOT NULL,
          map_id INTEGER,
          course INTEGER NOT NULL DEFAULT 0,
          mode TEXT NOT NULL,
          time_type TEXT NOT NULL,
          teleports INTEGER NOT NULL DEFAULT 0,
          run_time_seconds REAL NOT NULL,
          threshold_seconds REAL NOT NULL,
          replay_storage_key TEXT,
          replay_file_name TEXT,
          replay_file_size BIGINT,
          replay_content_type TEXT,
          replay_category TEXT NOT NULL DEFAULT 'replay',
          status TEXT NOT NULL DEFAULT 'pending',
          reviewed_by TEXT,
          review_note TEXT,
          reviewed_at TIMESTAMPTZ,
          global_submit_status TEXT NOT NULL DEFAULT 'not_submitted',
          global_record_id INTEGER,
          global_submit_error TEXT,
          global_submitted_at TIMESTAMPTZ,
          ban_id UUID REFERENCES ban_records(id) ON DELETE SET NULL,
          created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
          updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
        )"#,
        )
        .execute(&self.pool)
        .await?;

        let record_alters = [
            r#"ALTER TABLE abnormal_records ADD COLUMN IF NOT EXISTS idempotency_key TEXT"#,
            r#"ALTER TABLE abnormal_records ADD COLUMN IF NOT EXISTS server_id UUID REFERENCES servers(id) ON DELETE SET NULL"#,
            r#"ALTER TABLE abnormal_records ADD COLUMN IF NOT EXISTS server_name TEXT"#,
            r#"ALTER TABLE abnormal_records ADD COLUMN IF NOT EXISTS server_port INTEGER"#,
            r#"ALTER TABLE abnormal_records ADD COLUMN IF NOT EXISTS steam_id64 TEXT"#,
            r#"ALTER TABLE abnormal_records ADD COLUMN IF NOT EXISTS steam_id2 TEXT"#,
            r#"ALTER TABLE abnormal_records ADD COLUMN IF NOT EXISTS player_name TEXT"#,
            r#"ALTER TABLE abnormal_records ADD COLUMN IF NOT EXISTS map_name TEXT"#,
            r#"ALTER TABLE abnormal_records ADD COLUMN IF NOT EXISTS map_id INTEGER"#,
            r#"ALTER TABLE abnormal_records ADD COLUMN IF NOT EXISTS course INTEGER NOT NULL DEFAULT 0"#,
            r#"ALTER TABLE abnormal_records ADD COLUMN IF NOT EXISTS mode TEXT NOT NULL DEFAULT 'unknown'"#,
            r#"ALTER TABLE abnormal_records ADD COLUMN IF NOT EXISTS time_type TEXT NOT NULL DEFAULT 'tp'"#,
            r#"ALTER TABLE abnormal_records ADD COLUMN IF NOT EXISTS teleports INTEGER NOT NULL DEFAULT 0"#,
            r#"ALTER TABLE abnormal_records ADD COLUMN IF NOT EXISTS run_time_seconds REAL NOT NULL DEFAULT 0"#,
            r#"ALTER TABLE abnormal_records ADD COLUMN IF NOT EXISTS threshold_seconds REAL NOT NULL DEFAULT 0"#,
            r#"ALTER TABLE abnormal_records ADD COLUMN IF NOT EXISTS replay_storage_key TEXT"#,
            r#"ALTER TABLE abnormal_records ADD COLUMN IF NOT EXISTS replay_file_name TEXT"#,
            r#"ALTER TABLE abnormal_records ADD COLUMN IF NOT EXISTS replay_file_size BIGINT"#,
            r#"ALTER TABLE abnormal_records ADD COLUMN IF NOT EXISTS replay_content_type TEXT"#,
            r#"ALTER TABLE abnormal_records ADD COLUMN IF NOT EXISTS replay_category TEXT NOT NULL DEFAULT 'replay'"#,
            r#"ALTER TABLE abnormal_records ADD COLUMN IF NOT EXISTS status TEXT NOT NULL DEFAULT 'pending'"#,
            r#"ALTER TABLE abnormal_records ADD COLUMN IF NOT EXISTS reviewed_by TEXT"#,
            r#"ALTER TABLE abnormal_records ADD COLUMN IF NOT EXISTS review_note TEXT"#,
            r#"ALTER TABLE abnormal_records ADD COLUMN IF NOT EXISTS reviewed_at TIMESTAMPTZ"#,
            r#"ALTER TABLE abnormal_records ADD COLUMN IF NOT EXISTS global_submit_status TEXT NOT NULL DEFAULT 'not_submitted'"#,
            r#"ALTER TABLE abnormal_records ADD COLUMN IF NOT EXISTS global_record_id INTEGER"#,
            r#"ALTER TABLE abnormal_records ADD COLUMN IF NOT EXISTS global_submit_error TEXT"#,
            r#"ALTER TABLE abnormal_records ADD COLUMN IF NOT EXISTS global_submitted_at TIMESTAMPTZ"#,
            r#"ALTER TABLE abnormal_records ADD COLUMN IF NOT EXISTS ban_id UUID REFERENCES ban_records(id) ON DELETE SET NULL"#,
            r#"ALTER TABLE abnormal_records ADD COLUMN IF NOT EXISTS created_at TIMESTAMPTZ NOT NULL DEFAULT now()"#,
            r#"ALTER TABLE abnormal_records ADD COLUMN IF NOT EXISTS updated_at TIMESTAMPTZ NOT NULL DEFAULT now()"#,
        ];
        for sql in record_alters {
            sqlx::query(sql).execute(&self.pool).await?;
        }

        sqlx::query(
            r#"CREATE UNIQUE INDEX IF NOT EXISTS idx_abnormal_records_idempotency_key
           ON abnormal_records (idempotency_key)
           WHERE idempotency_key IS NOT NULL"#,
        )
        .execute(&self.pool)
        .await?;
        sqlx::query(
            r#"CREATE INDEX IF NOT EXISTS idx_abnormal_records_status_created
           ON abnormal_records (status, created_at DESC)"#,
        )
        .execute(&self.pool)
        .await?;
        sqlx::query(
            r#"CREATE INDEX IF NOT EXISTS idx_abnormal_records_player
           ON abnormal_records (steam_id64)"#,
        )
        .execute(&self.pool)
        .await?;
        sqlx::query(
            r#"CREATE INDEX IF NOT EXISTS idx_abnormal_records_map
           ON abnormal_records (lower(map_name), created_at DESC)"#,
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}

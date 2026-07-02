use super::Database;

impl Database {
    pub(super) async fn migrate_whitelist_schema(&self) -> anyhow::Result<()> {
        let alters = [
            r#"ALTER TABLE whitelist_requests ADD COLUMN IF NOT EXISTS nickname TEXT"#,
            r#"ALTER TABLE whitelist_requests ADD COLUMN IF NOT EXISTS steamid64 TEXT"#,
            r#"ALTER TABLE whitelist_requests ADD COLUMN IF NOT EXISTS steamid TEXT"#,
            r#"ALTER TABLE whitelist_requests ADD COLUMN IF NOT EXISTS steamid3 TEXT"#,
            r#"ALTER TABLE whitelist_requests ADD COLUMN IF NOT EXISTS profile_url TEXT"#,
            r#"ALTER TABLE whitelist_requests ADD COLUMN IF NOT EXISTS steam_persona_name TEXT"#,
            r#"ALTER TABLE whitelist_requests ADD COLUMN IF NOT EXISTS contact TEXT"#,
            r#"ALTER TABLE whitelist_requests ADD COLUMN IF NOT EXISTS applied_at TIMESTAMPTZ"#,
            r#"ALTER TABLE whitelist_requests ADD COLUMN IF NOT EXISTS approved_at TIMESTAMPTZ"#,
            r#"ALTER TABLE whitelist_requests ADD COLUMN IF NOT EXISTS approved_by TEXT"#,
            r#"ALTER TABLE whitelist_requests ADD COLUMN IF NOT EXISTS rejected_at TIMESTAMPTZ"#,
            r#"ALTER TABLE whitelist_requests ADD COLUMN IF NOT EXISTS rejected_by TEXT"#,
            r#"ALTER TABLE whitelist_requests ADD COLUMN IF NOT EXISTS rejection_reason TEXT"#,
            r#"ALTER TABLE whitelist_requests ADD COLUMN IF NOT EXISTS revoked_at TIMESTAMPTZ"#,
            r#"ALTER TABLE whitelist_requests ADD COLUMN IF NOT EXISTS revoked_by TEXT"#,
            r#"ALTER TABLE whitelist_requests ADD COLUMN IF NOT EXISTS source TEXT"#,
            r#"ALTER TABLE whitelist_requests ADD COLUMN IF NOT EXISTS updated_at TIMESTAMPTZ"#,
            r#"ALTER TABLE whitelist_requests ADD COLUMN IF NOT EXISTS approval_reason TEXT"#,
        ];
        for sql in alters {
            sqlx::query(sql).execute(&self.pool).await?;
        }

        // 旧字段数据迁移
        sqlx::query(
        r#"
        DO $$
        BEGIN
          IF EXISTS (
            SELECT 1
            FROM information_schema.columns
            WHERE table_schema = current_schema()
              AND table_name = 'whitelist_requests'
              AND column_name = 'player_name'
          ) THEN
            EXECUTE 'UPDATE whitelist_requests
                     SET nickname = player_name
                     WHERE (nickname IS NULL OR btrim(nickname) = '''')
                       AND player_name IS NOT NULL';
          END IF;

          IF EXISTS (
            SELECT 1
            FROM information_schema.columns
            WHERE table_schema = current_schema()
              AND table_name = 'whitelist_requests'
              AND column_name = 'steam_id64'
          ) THEN
            EXECUTE 'UPDATE whitelist_requests
                     SET steamid64 = steam_id64
                     WHERE steamid64 IS NULL
                       AND steam_id64 IS NOT NULL';
          END IF;

          IF EXISTS (
            SELECT 1
            FROM information_schema.columns
            WHERE table_schema = current_schema()
              AND table_name = 'whitelist_requests'
              AND column_name = 'steam_profile_url'
          ) THEN
            EXECUTE 'UPDATE whitelist_requests
                     SET profile_url = steam_profile_url
                     WHERE profile_url IS NULL
                       AND steam_profile_url IS NOT NULL';
          END IF;

          IF EXISTS (
            SELECT 1
            FROM information_schema.columns
            WHERE table_schema = current_schema()
              AND table_name = 'whitelist_requests'
              AND column_name = 'reject_reason'
          ) THEN
            EXECUTE 'UPDATE whitelist_requests
                     SET rejection_reason = reject_reason
                     WHERE rejection_reason IS NULL
                       AND reject_reason IS NOT NULL';
          END IF;

          IF EXISTS (
            SELECT 1
            FROM information_schema.columns
            WHERE table_schema = current_schema()
              AND table_name = 'whitelist_requests'
              AND column_name = 'created_at'
          ) THEN
            EXECUTE 'UPDATE whitelist_requests SET applied_at = created_at WHERE applied_at IS NULL';
            EXECUTE 'UPDATE whitelist_requests SET updated_at = created_at WHERE updated_at IS NULL';
            EXECUTE 'UPDATE whitelist_requests SET approved_at = created_at WHERE approved_at IS NULL AND status = ''approved''';
            EXECUTE 'UPDATE whitelist_requests SET rejected_at = created_at WHERE rejected_at IS NULL AND status = ''rejected''';
          END IF;

          IF EXISTS (
            SELECT 1
            FROM information_schema.columns
            WHERE table_schema = current_schema()
              AND table_name = 'whitelist_requests'
              AND column_name = 'reviewed_at'
          ) THEN
            EXECUTE 'UPDATE whitelist_requests
                     SET applied_at = reviewed_at
                     WHERE applied_at IS NULL
                       AND reviewed_at IS NOT NULL';
            EXECUTE 'UPDATE whitelist_requests
                     SET updated_at = reviewed_at
                     WHERE updated_at IS NULL
                       AND reviewed_at IS NOT NULL';
            EXECUTE 'UPDATE whitelist_requests
                     SET approved_at = reviewed_at
                     WHERE approved_at IS NULL
                       AND status = ''approved''
                       AND reviewed_at IS NOT NULL';
            EXECUTE 'UPDATE whitelist_requests
                     SET rejected_at = reviewed_at
                     WHERE rejected_at IS NULL
                       AND status = ''rejected''
                       AND reviewed_at IS NOT NULL';
            EXECUTE 'UPDATE whitelist_requests
                     SET revoked_at = reviewed_at
                     WHERE revoked_at IS NULL
                       AND status = ''revoked''
                       AND reviewed_at IS NOT NULL';
          END IF;

          IF EXISTS (
            SELECT 1
            FROM information_schema.columns
            WHERE table_schema = current_schema()
              AND table_name = 'whitelist_requests'
              AND column_name = 'reviewer'
          ) THEN
            EXECUTE 'UPDATE whitelist_requests
                     SET approved_by = reviewer
                     WHERE approved_by IS NULL
                       AND reviewer IS NOT NULL
                       AND status = ''approved''';
            EXECUTE 'UPDATE whitelist_requests
                     SET rejected_by = reviewer
                     WHERE rejected_by IS NULL
                       AND reviewer IS NOT NULL
                       AND status = ''rejected''';
            EXECUTE 'UPDATE whitelist_requests
                     SET revoked_by = reviewer
                     WHERE revoked_by IS NULL
                       AND reviewer IS NOT NULL
                       AND status = ''revoked''';
          END IF;

          IF EXISTS (
            SELECT 1
            FROM information_schema.columns
            WHERE table_schema = current_schema()
              AND table_name = 'whitelist_requests'
              AND column_name = 'reviewed_by'
          ) THEN
            EXECUTE 'UPDATE whitelist_requests
                     SET approved_by = reviewed_by
                     WHERE approved_by IS NULL
                       AND reviewed_by IS NOT NULL
                       AND status = ''approved''';
            EXECUTE 'UPDATE whitelist_requests
                     SET rejected_by = reviewed_by
                     WHERE rejected_by IS NULL
                       AND reviewed_by IS NOT NULL
                       AND status = ''rejected''';
            EXECUTE 'UPDATE whitelist_requests
                     SET revoked_by = reviewed_by
                     WHERE revoked_by IS NULL
                       AND reviewed_by IS NOT NULL
                       AND status = ''revoked''';
          END IF;
        END
        $$;
        "#,
    ).execute(&self.pool).await?;

        // 数据修补
        let fixes = [
            r#"UPDATE whitelist_requests SET steamid64 = steam_id WHERE steamid64 IS NULL AND steam_id ~ '^[0-9]{17}$'"#,
            r#"UPDATE whitelist_requests SET steamid = steam_id WHERE steamid IS NULL AND steam_id IS NOT NULL"#,
            r#"UPDATE whitelist_requests SET nickname = COALESCE(steamid64, steam_id, '未知玩家') WHERE nickname IS NULL OR btrim(nickname) = ''"#,
            r#"UPDATE whitelist_requests SET source = 'public' WHERE source IS NULL OR btrim(source) = ''"#,
            r#"UPDATE whitelist_requests SET updated_at = COALESCE(approved_at, rejected_at, revoked_at, applied_at) WHERE updated_at IS NULL"#,
            r#"UPDATE whitelist_requests SET applied_at = COALESCE(updated_at, now()) WHERE applied_at IS NULL"#,
        ];
        for sql in fixes {
            sqlx::query(sql).execute(&self.pool).await?;
        }

        // 索引
        sqlx::query(r#"DROP INDEX IF EXISTS idx_whitelist_requests_steamid64"#)
            .execute(&self.pool)
            .await?;
        sqlx::query(
            r#"CREATE INDEX IF NOT EXISTS idx_whitelist_requests_steamid64_lookup
           ON whitelist_requests (steamid64, updated_at DESC, applied_at DESC)
           WHERE steamid64 IS NOT NULL"#,
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    // 日志、审计、离线操作、权限规则 + 性能索引 + 访问控制
}

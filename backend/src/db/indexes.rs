#[cfg(not(test))]
use super::utils::quote_ident;
use super::Database;

impl Database {
    pub(super) async fn migrate_adds_missing_constraints_and_indexes(&self) -> anyhow::Result<()> {
        // ban_records.server_id 外键约束
        sqlx::query(
        r#"DO $$ BEGIN
            IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'fk_ban_records_server_id') THEN
                UPDATE ban_records SET server_id = NULL
                WHERE server_id IS NOT NULL
                  AND server_id NOT IN (SELECT id FROM servers);
                ALTER TABLE ban_records
                ADD CONSTRAINT fk_ban_records_server_id
                FOREIGN KEY (server_id) REFERENCES servers(id) ON DELETE SET NULL;
            END IF;
        END $$;"#,
    ).execute(&self.pool).await?;
        self.migrate_server_foreign_key_delete_actions().await?;

        // communities.created_by 外键约束
        sqlx::query(
        r#"DO $$ BEGIN
            IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'fk_communities_created_by') THEN
                UPDATE communities SET created_by = NULL
                WHERE created_by IS NOT NULL
                  AND created_by NOT IN (SELECT id FROM users);
                ALTER TABLE communities
                ADD CONSTRAINT fk_communities_created_by
                FOREIGN KEY (created_by) REFERENCES users(id) ON DELETE SET NULL;
            END IF;
        END $$;"#,
    ).execute(&self.pool).await?;

        // whitelist_requests 复合索引
        sqlx::query(
            r#"CREATE INDEX IF NOT EXISTS idx_whitelist_requests_status_applied
           ON whitelist_requests (status, applied_at DESC)"#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"CREATE INDEX IF NOT EXISTS idx_whitelist_requests_status_approved_applied
           ON whitelist_requests (status, approved_at DESC NULLS LAST, applied_at DESC)"#,
        )
        .execute(&self.pool)
        .await?;

        // ban_records 复合索引
        sqlx::query(
            r#"CREATE INDEX IF NOT EXISTS idx_ban_records_status_created
           ON ban_records (status, created_at DESC)"#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"CREATE INDEX IF NOT EXISTS idx_servers_status
           ON servers (status)"#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"CREATE INDEX IF NOT EXISTS idx_users_role_created
           ON users (role, created_at DESC)"#,
        )
        .execute(&self.pool)
        .await?;

        // ban_appeals 复合索引
        sqlx::query(
            r#"CREATE INDEX IF NOT EXISTS idx_ban_appeals_ban_id_status
           ON ban_appeals (ban_id, status)"#,
        )
        .execute(&self.pool)
        .await?;

        #[cfg(not(test))]
        self.migrate_query_performance_indexes().await?;

        Ok(())
    }

    async fn migrate_server_foreign_key_delete_actions(&self) -> anyhow::Result<()> {
        let server_fks = [
            ("ban_records", "fk_ban_records_server_id", "SET NULL", "n"),
            ("audit_logs", "fk_audit_logs_server_id", "SET NULL", "n"),
            (
                "server_status_history",
                "fk_server_status_history_server_id",
                "CASCADE",
                "c",
            ),
            (
                "offline_operations",
                "fk_offline_operations_server_id",
                "CASCADE",
                "c",
            ),
            (
                "server_online_players",
                "fk_server_online_players_server_id",
                "CASCADE",
                "c",
            ),
            (
                "player_server_sessions",
                "fk_player_server_sessions_server_id",
                "CASCADE",
                "c",
            ),
            (
                "player_access_logs",
                "fk_player_access_logs_server_id",
                "CASCADE",
                "c",
            ),
        ];

        for (table, constraint_name, delete_action, pg_delete_action) in server_fks {
            let sql = format!(
                r#"
                DO $$
                DECLARE
                    existing_constraint TEXT;
                BEGIN
                    IF to_regclass('{table}') IS NULL THEN
                        RETURN;
                    END IF;

                    FOR existing_constraint IN
                        SELECT con.conname
                        FROM pg_constraint con
                        JOIN pg_attribute att
                          ON att.attrelid = con.conrelid
                         AND att.attnum = ANY(con.conkey)
                        WHERE con.contype = 'f'
                          AND con.conrelid = '{table}'::regclass
                          AND con.confrelid = 'servers'::regclass
                          AND att.attname = 'server_id'
                          AND con.confdeltype <> '{pg_delete_action}'
                    LOOP
                        EXECUTE format('ALTER TABLE {table} DROP CONSTRAINT %I', existing_constraint);
                    END LOOP;

                    IF '{pg_delete_action}' = 'n' THEN
                        UPDATE {table} child
                        SET server_id = NULL
                        WHERE server_id IS NOT NULL
                          AND NOT EXISTS (
                              SELECT 1 FROM servers parent WHERE parent.id = child.server_id
                          );
                    ELSE
                        DELETE FROM {table} child
                        WHERE server_id IS NOT NULL
                          AND NOT EXISTS (
                              SELECT 1 FROM servers parent WHERE parent.id = child.server_id
                          );
                    END IF;

                    IF NOT EXISTS (
                        SELECT 1
                        FROM pg_constraint con
                        JOIN pg_attribute att
                          ON att.attrelid = con.conrelid
                         AND att.attnum = ANY(con.conkey)
                        WHERE con.contype = 'f'
                          AND con.conrelid = '{table}'::regclass
                          AND con.confrelid = 'servers'::regclass
                          AND att.attname = 'server_id'
                          AND con.confdeltype = '{pg_delete_action}'
                    ) THEN
                        ALTER TABLE {table}
                        ADD CONSTRAINT {constraint_name}
                        FOREIGN KEY (server_id) REFERENCES servers(id) ON DELETE {delete_action};
                    END IF;
                END $$;
                "#
            );
            sqlx::query(&sql).execute(&self.pool).await?;
        }

        Ok(())
    }

    #[cfg(not(test))]
    pub(super) async fn migrate_query_performance_indexes(&self) -> anyhow::Result<()> {
        let query_perf_indexes = [
            r#"CREATE INDEX IF NOT EXISTS idx_notifications_user_created
           ON notifications (user_id, created_at DESC)"#,
            r#"CREATE INDEX IF NOT EXISTS idx_ban_appeals_status_created
           ON ban_appeals (status, created_at DESC)"#,
            r#"CREATE INDEX IF NOT EXISTS idx_audit_logs_source_created
           ON audit_logs (source, created_at DESC)"#,
            r#"CREATE INDEX IF NOT EXISTS idx_audit_logs_success_created
           ON audit_logs (success, created_at DESC)"#,
            r#"CREATE INDEX IF NOT EXISTS idx_player_reports_created
           ON player_reports (created_at DESC)"#,
        ];
        for sql in query_perf_indexes {
            sqlx::query(sql).execute(&self.pool).await?;
        }

        if let Some(trgm_schema) = self.pg_trgm_schema().await {
            let trgm_ops = format!("{}.gin_trgm_ops", quote_ident(&trgm_schema));
            let trigram_indexes = [
                format!(
                    r#"CREATE INDEX IF NOT EXISTS idx_ban_records_steam_id_trgm
                   ON ban_records USING gin (steam_id {trgm_ops})"#
                ),
                format!(
                    r#"CREATE INDEX IF NOT EXISTS idx_ban_records_player_trgm
                   ON ban_records USING gin (player {trgm_ops}) WHERE player IS NOT NULL"#
                ),
                format!(
                    r#"CREATE INDEX IF NOT EXISTS idx_whitelist_requests_steamid64_trgm
                   ON whitelist_requests USING gin (steamid64 {trgm_ops})"#
                ),
                format!(
                    r#"CREATE INDEX IF NOT EXISTS idx_whitelist_requests_nickname_trgm
                   ON whitelist_requests USING gin (nickname {trgm_ops})"#
                ),
                format!(
                    r#"CREATE INDEX IF NOT EXISTS idx_player_reports_target_steam_id_trgm
                   ON player_reports USING gin (target_steam_id {trgm_ops})"#
                ),
                format!(
                    r#"CREATE INDEX IF NOT EXISTS idx_player_reports_target_player_name_trgm
                   ON player_reports USING gin (target_player_name {trgm_ops}) WHERE target_player_name IS NOT NULL"#
                ),
                format!(
                    r#"CREATE INDEX IF NOT EXISTS idx_ban_appeals_steam_id_trgm
                   ON ban_appeals USING gin (steam_id {trgm_ops})"#
                ),
                format!(
                    r#"CREATE INDEX IF NOT EXISTS idx_ban_appeals_player_name_trgm
                   ON ban_appeals USING gin (player_name {trgm_ops})"#
                ),
                format!(
                    r#"CREATE INDEX IF NOT EXISTS idx_users_username_trgm
                   ON users USING gin (username {trgm_ops})"#
                ),
                format!(
                    r#"CREATE INDEX IF NOT EXISTS idx_users_display_name_trgm
                   ON users USING gin (display_name {trgm_ops})"#
                ),
                format!(
                    r#"CREATE INDEX IF NOT EXISTS idx_users_remark_trgm
                   ON users USING gin (remark {trgm_ops}) WHERE remark IS NOT NULL"#
                ),
                format!(
                    r#"CREATE INDEX IF NOT EXISTS idx_admin_logs_operator_name_trgm
                   ON admin_logs USING gin (operator_name {trgm_ops})"#
                ),
                format!(
                    r#"CREATE INDEX IF NOT EXISTS idx_admin_logs_module_trgm
                   ON admin_logs USING gin (module {trgm_ops})"#
                ),
                format!(
                    r#"CREATE INDEX IF NOT EXISTS idx_admin_logs_action_trgm
                   ON admin_logs USING gin (action {trgm_ops})"#
                ),
                format!(
                    r#"CREATE INDEX IF NOT EXISTS idx_audit_logs_operator_name_trgm
                   ON audit_logs USING gin (operator_name {trgm_ops})"#
                ),
                format!(
                    r#"CREATE INDEX IF NOT EXISTS idx_audit_logs_target_trgm
                   ON audit_logs USING gin (target {trgm_ops})"#
                ),
                format!(
                    r#"CREATE INDEX IF NOT EXISTS idx_audit_logs_player_name_trgm
                   ON audit_logs USING gin (player_name {trgm_ops}) WHERE player_name IS NOT NULL"#
                ),
            ];
            for sql in trigram_indexes {
                sqlx::query(&sql).execute(&self.pool).await?;
            }
        }

        Ok(())
    }
}

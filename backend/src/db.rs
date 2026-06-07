use sqlx::{postgres::PgPoolOptions, PgPool};
use std::time::Duration;

use crate::config::Config;

#[derive(Clone)]
pub struct Database {
    pub pool: PgPool,
}

impl Database {
    pub async fn connect(database_url: &str, config: &Config) -> anyhow::Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(config.db_max_connections)
            .min_connections(config.db_min_connections)
            .acquire_timeout(Duration::from_secs(config.db_acquire_timeout_secs))
            .idle_timeout(Duration::from_secs(config.db_idle_timeout_secs))
            .connect(database_url)
            .await
            .map_err(|e| anyhow::anyhow!("database connect failed: {e}"))?;

        Ok(Self { pool })
    }

    /// 测试用连接方法，使用默认配置
    #[cfg(test)]
    pub async fn connect_for_test(database_url: &str) -> anyhow::Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(1)
            .acquire_timeout(Duration::from_secs(10))
            .connect(database_url)
            .await
            .map_err(|e| anyhow::anyhow!("database connect failed: {e}"))?;

        Ok(Self { pool })
    }

    pub async fn migrate(&self) -> anyhow::Result<()> {
        self.migrate_core_tables().await?;
        self.migrate_ban_records_schema().await?;
        self.migrate_users_and_communities_schema().await?;
        self.migrate_servers_schema().await?;
        self.migrate_player_api_schema().await?;
        self.migrate_server_data().await?;
        self.migrate_whitelist_schema().await?;
        self.migrate_logs_operations_and_indexes().await?;
        self.migrate_external_servers_schema().await?;
        self.migrate_external_ban_api_schema().await?;
        self.migrate_ban_api_keys_schema().await?;
        self.migrate_map_tiers_table().await?;
        self.migrate_map_sync_schema().await?;
        self.migrate_notifications_schema().await?;
        self.migrate_ban_appeals_schema().await?;
        self.migrate_appeal_files_schema().await?;
        self.migrate_ban_files_schema().await?;
        self.migrate_player_reports_schema().await?;
        self.migrate_player_internal_notes_schema().await?;
        self.migrate_adds_missing_constraints_and_indexes().await?;
        Ok(())
    }

    /// 核心 CREATE TABLE 语句（首次建表）
    async fn migrate_core_tables(&self) -> anyhow::Result<()> {
        let statements = [
            r#"CREATE TABLE IF NOT EXISTS users (
              id UUID PRIMARY KEY,
              username TEXT UNIQUE NOT NULL,
              display_name TEXT NOT NULL,
              password_hash TEXT NOT NULL,
              role TEXT NOT NULL,
              created_at TIMESTAMPTZ NOT NULL DEFAULT now()
            )"#,
            r#"CREATE TABLE IF NOT EXISTS sessions (
              token UUID PRIMARY KEY,
              user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
              role TEXT NOT NULL,
              display_name TEXT NOT NULL,
              role_label TEXT NOT NULL,
              expires_at TIMESTAMPTZ NOT NULL,
              created_at TIMESTAMPTZ NOT NULL DEFAULT now()
            )"#,
            r#"CREATE TABLE IF NOT EXISTS public_whitelist (
              id UUID PRIMARY KEY,
              nickname TEXT NOT NULL,
              steam_id64 TEXT NOT NULL,
              status TEXT NOT NULL,
              submitted_at TIMESTAMPTZ NOT NULL DEFAULT now()
            )"#,
            r#"CREATE TABLE IF NOT EXISTS public_bans (
              id UUID PRIMARY KEY,
              player TEXT NOT NULL,
              steam_id TEXT NOT NULL,
              reason TEXT NOT NULL,
              status TEXT NOT NULL,
              created_at TIMESTAMPTZ NOT NULL DEFAULT now()
            )"#,
            r#"CREATE TABLE IF NOT EXISTS communities (
              id UUID PRIMARY KEY,
              name TEXT NOT NULL,
              created_by UUID,
              created_at TIMESTAMPTZ NOT NULL DEFAULT now()
            )"#,
            r#"CREATE TABLE IF NOT EXISTS servers (
              id UUID PRIMARY KEY,
              community_id UUID NOT NULL REFERENCES communities(id) ON DELETE CASCADE,
              name TEXT NOT NULL,
              ip TEXT NOT NULL,
              port INTEGER NOT NULL,
              rcon_password TEXT NOT NULL,
              report_token TEXT UNIQUE NOT NULL DEFAULT gen_random_uuid(),
              note TEXT,
              status TEXT NOT NULL DEFAULT 'untested',
              players TEXT[] NOT NULL DEFAULT ARRAY[]::TEXT[],
              last_tested_at TIMESTAMPTZ,
              last_reported_at TIMESTAMPTZ,
              access_restriction_enabled BOOLEAN NOT NULL DEFAULT false,
              min_rating INTEGER NOT NULL DEFAULT 0,
              min_steam_level INTEGER NOT NULL DEFAULT 0,
              whitelist_mode_enabled BOOLEAN NOT NULL DEFAULT false,
              created_at TIMESTAMPTZ NOT NULL DEFAULT now()
            )"#,
            r#"CREATE TABLE IF NOT EXISTS player_access_cache (
              steamid64 TEXT PRIMARY KEY,
              rating INTEGER NOT NULL,
              steam_level INTEGER NOT NULL,
              expires_at TIMESTAMPTZ NOT NULL,
              updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
            )"#,
            r#"CREATE TABLE IF NOT EXISTS whitelist_requests (
              id UUID PRIMARY KEY,
              steam_id TEXT NOT NULL,
              nickname TEXT NOT NULL,
              status TEXT NOT NULL,
              reviewer TEXT,
              created_at TIMESTAMPTZ NOT NULL DEFAULT now()
            )"#,
            r#"CREATE TABLE IF NOT EXISTS ban_records (
              id UUID PRIMARY KEY,
              player TEXT,
              steam_id TEXT NOT NULL,
              ip_address TEXT,
              server_name TEXT,
              ban_type TEXT NOT NULL DEFAULT 'steam',
              duration_minutes INTEGER NOT NULL DEFAULT 0,
              expires_at TIMESTAMPTZ,
              reason TEXT NOT NULL DEFAULT '未填写',
              status TEXT NOT NULL,
              operator_name TEXT NOT NULL,
              source TEXT NOT NULL DEFAULT 'manual',
              server_id UUID,
              server_port INTEGER,
              removed_reason TEXT,
              removed_by TEXT,
              removed_at TIMESTAMPTZ,
              created_at TIMESTAMPTZ NOT NULL DEFAULT now()
            )"#,
            r#"CREATE TABLE IF NOT EXISTS player_api_config (
              id BOOLEAN PRIMARY KEY DEFAULT true,
              max_api_count INTEGER NOT NULL DEFAULT 3,
              interval_seconds INTEGER NOT NULL DEFAULT 30,
              updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
              CONSTRAINT player_api_config_single_row CHECK (id)
            )"#,
            r#"CREATE TABLE IF NOT EXISTS player_api_webhooks (
              id UUID PRIMARY KEY,
              webhook_url TEXT NOT NULL,
              secret TEXT,
              server_ids UUID[] NOT NULL DEFAULT ARRAY[]::UUID[],
              last_status TEXT,
              last_error TEXT,
              last_dispatched_at TIMESTAMPTZ,
              created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
              updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
            )"#,
            r#"CREATE TABLE IF NOT EXISTS admin_logs (
              id UUID PRIMARY KEY,
              operator_name TEXT NOT NULL,
              module TEXT NOT NULL,
              action TEXT NOT NULL,
              target_detail TEXT NOT NULL,
              ip_address TEXT NOT NULL,
              created_at TIMESTAMPTZ NOT NULL DEFAULT now()
            )"#,
        ];

        for stmt in statements {
            sqlx::query(stmt).execute(&self.pool).await?;
        }
        Ok(())
    }

    /// ban_records 补充列
    async fn migrate_ban_records_schema(&self) -> anyhow::Result<()> {
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
    async fn migrate_users_and_communities_schema(&self) -> anyhow::Result<()> {
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

    /// servers 补充列 + player_access_cache / server_online_players
    async fn migrate_servers_schema(&self) -> anyhow::Result<()> {
        let alters = [
            r#"ALTER TABLE servers ADD COLUMN IF NOT EXISTS ip TEXT"#,
            r#"ALTER TABLE servers ADD COLUMN IF NOT EXISTS port INTEGER"#,
            r#"ALTER TABLE servers ADD COLUMN IF NOT EXISTS rcon_password TEXT"#,
            r#"ALTER TABLE servers ADD COLUMN IF NOT EXISTS report_token TEXT"#,
            r#"ALTER TABLE servers ADD COLUMN IF NOT EXISTS note TEXT"#,
            r#"ALTER TABLE servers ADD COLUMN IF NOT EXISTS last_tested_at TIMESTAMPTZ"#,
            r#"ALTER TABLE servers ADD COLUMN IF NOT EXISTS last_reported_at TIMESTAMPTZ"#,
            r#"ALTER TABLE servers ADD COLUMN IF NOT EXISTS access_restriction_enabled BOOLEAN NOT NULL DEFAULT false"#,
            r#"ALTER TABLE servers ADD COLUMN IF NOT EXISTS min_rating INTEGER NOT NULL DEFAULT 0"#,
            r#"ALTER TABLE servers ADD COLUMN IF NOT EXISTS min_steam_level INTEGER NOT NULL DEFAULT 0"#,
            r#"ALTER TABLE servers ADD COLUMN IF NOT EXISTS whitelist_mode_enabled BOOLEAN NOT NULL DEFAULT false"#,
            r#"ALTER TABLE servers ADD COLUMN IF NOT EXISTS max_players INTEGER NOT NULL DEFAULT 0"#,
        ];
        for sql in alters {
            sqlx::query(sql).execute(&self.pool).await?;
        }

        // report_token 填充 + 唯一约束
        sqlx::query(r#"UPDATE servers SET report_token = gen_random_uuid()::TEXT WHERE report_token IS NULL OR btrim(report_token) = ''"#)
            .execute(&self.pool).await?;
        sqlx::query(r#"ALTER TABLE servers ALTER COLUMN report_token SET NOT NULL"#)
            .execute(&self.pool)
            .await?;
        sqlx::query(r#"CREATE UNIQUE INDEX IF NOT EXISTS idx_servers_report_token_unique ON servers (report_token)"#)
            .execute(&self.pool).await?;

        sqlx::query(r#"ALTER TABLE servers ALTER COLUMN status SET DEFAULT 'untested'"#)
            .execute(&self.pool)
            .await?;

        // player_access_cache 表 + 索引
        sqlx::query(
            r#"CREATE TABLE IF NOT EXISTS player_access_cache (
              steamid64 TEXT PRIMARY KEY,
              rating INTEGER NOT NULL,
              steam_level INTEGER NOT NULL,
              expires_at TIMESTAMPTZ NOT NULL,
              updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
            )"#,
        )
        .execute(&self.pool)
        .await?;
        sqlx::query(r#"CREATE INDEX IF NOT EXISTS idx_player_access_cache_expires_at ON player_access_cache (expires_at)"#)
            .execute(&self.pool).await?;

        // server_online_players 表 + 索引
        sqlx::query(
            r#"CREATE TABLE IF NOT EXISTS server_online_players (
              server_id UUID NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
              name TEXT NOT NULL,
              steam_id64 TEXT NOT NULL,
              ip TEXT NOT NULL,
              ping INTEGER NOT NULL,
              server_port INTEGER NOT NULL,
              current_map TEXT NOT NULL DEFAULT '',
              reported_at TIMESTAMPTZ NOT NULL DEFAULT now()
            )"#,
        )
        .execute(&self.pool)
        .await?;
        sqlx::query(r#"CREATE INDEX IF NOT EXISTS idx_server_online_players_server_id ON server_online_players (server_id)"#)
            .execute(&self.pool).await?;
        sqlx::query(r#"ALTER TABLE server_online_players ADD COLUMN IF NOT EXISTS current_map TEXT NOT NULL DEFAULT ''"#)
            .execute(&self.pool).await?;

        Ok(())
    }

    /// 玩家 API 分发配置表
    async fn migrate_player_api_schema(&self) -> anyhow::Result<()> {
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

    /// 服务器遗留数据迁移（players text→text[]、addr 列迁移）
    async fn migrate_server_data(&self) -> anyhow::Result<()> {
        // players: text → text[]
        sqlx::query(
            r#"
            DO $$
            BEGIN
              IF EXISTS (
                SELECT 1
                FROM information_schema.columns
                WHERE table_schema = current_schema()
                  AND table_name = 'servers'
                  AND column_name = 'players'
                  AND udt_name = 'text'
              ) THEN
                EXECUTE 'ALTER TABLE servers ALTER COLUMN players DROP DEFAULT';
                EXECUTE $sql$
                  ALTER TABLE servers
                  ALTER COLUMN players TYPE TEXT[]
                  USING CASE
                    WHEN players IS NULL OR btrim(players) = '' THEN ARRAY[]::TEXT[]
                    WHEN players LIKE '{%}' THEN players::TEXT[]
                    ELSE ARRAY[players]
                  END
                $sql$;
              END IF;
            END
            $$;
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(r#"ALTER TABLE servers ALTER COLUMN players SET DEFAULT ARRAY[]::TEXT[]"#)
            .execute(&self.pool)
            .await?;
        sqlx::query(r#"UPDATE servers SET players = ARRAY[]::TEXT[] WHERE players IS NULL"#)
            .execute(&self.pool)
            .await?;
        sqlx::query(r#"ALTER TABLE servers ALTER COLUMN players SET NOT NULL"#)
            .execute(&self.pool)
            .await?;

        // addr → ip/port 迁移
        sqlx::query(
            r#"
            DO $$
            BEGIN
              IF EXISTS (
                SELECT 1
                FROM information_schema.columns
                WHERE table_schema = current_schema()
                  AND table_name = 'servers'
                  AND column_name = 'addr'
              ) THEN
                EXECUTE 'UPDATE servers SET ip = split_part(addr, '':'' , 1) WHERE ip IS NULL AND addr IS NOT NULL';
                EXECUTE 'UPDATE servers SET port = NULLIF(split_part(addr, '':'' , 2), '''')::INTEGER WHERE port IS NULL AND addr IS NOT NULL';
                EXECUTE 'ALTER TABLE servers DROP COLUMN IF EXISTS addr';
              END IF;
            END
            $$;
            "#,
        ).execute(&self.pool).await?;

        Ok(())
    }

    /// 白名单表字段扩充 + 旧数据迁移
    async fn migrate_whitelist_schema(&self) -> anyhow::Result<()> {
        let alters = [
            r#"ALTER TABLE whitelist_requests ADD COLUMN IF NOT EXISTS nickname TEXT"#,
            r#"ALTER TABLE whitelist_requests ADD COLUMN IF NOT EXISTS steamid64 TEXT"#,
            r#"ALTER TABLE whitelist_requests ADD COLUMN IF NOT EXISTS steamid TEXT"#,
            r#"ALTER TABLE whitelist_requests ADD COLUMN IF NOT EXISTS steamid3 TEXT"#,
            r#"ALTER TABLE whitelist_requests ADD COLUMN IF NOT EXISTS profile_url TEXT"#,
            r#"ALTER TABLE whitelist_requests ADD COLUMN IF NOT EXISTS steam_persona_name TEXT"#,
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

    /// 日志、审计、离线操作、权限规则 + 性能索引 + 访问控制
    async fn migrate_logs_operations_and_indexes(&self) -> anyhow::Result<()> {
        // 服务器状态历史
        sqlx::query(
            r#"CREATE TABLE IF NOT EXISTS server_status_history (
              id UUID PRIMARY KEY,
              server_id UUID NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
              fps REAL NOT NULL,
              cpu_usage REAL NOT NULL,
              tickrate REAL NOT NULL,
              in_rate REAL NOT NULL DEFAULT 0,
              out_rate REAL NOT NULL DEFAULT 0,
              uptime_seconds BIGINT NOT NULL DEFAULT 0,
              players_count INTEGER NOT NULL DEFAULT 0,
              max_players INTEGER NOT NULL DEFAULT 0,
              current_map TEXT NOT NULL DEFAULT '',
              reported_at TIMESTAMPTZ NOT NULL DEFAULT now()
            )"#,
        )
        .execute(&self.pool)
        .await?;
        sqlx::query(r#"CREATE INDEX IF NOT EXISTS idx_server_status_history_server_id ON server_status_history (server_id, reported_at DESC)"#)
            .execute(&self.pool).await?;
        // 为清理查询添加独立索引（按时间排序，加速 DELETE ... WHERE reported_at < ...）
        sqlx::query(r#"CREATE INDEX IF NOT EXISTS idx_server_status_history_reported_at ON server_status_history (reported_at)"#)
            .execute(&self.pool).await?;

        // 玩家进服权限规则
        sqlx::query(
            r#"CREATE TABLE IF NOT EXISTS player_access_rules (
              id UUID PRIMARY KEY,
              steamid64 TEXT UNIQUE NOT NULL,
              nickname TEXT NOT NULL,
              allowed_communities UUID[] NOT NULL DEFAULT ARRAY[]::UUID[],
              blocked_communities UUID[] NOT NULL DEFAULT ARRAY[]::UUID[],
              allowed_servers UUID[] NOT NULL DEFAULT ARRAY[]::UUID[],
              blocked_servers UUID[] NOT NULL DEFAULT ARRAY[]::UUID[],
              created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
              updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
            )"#,
        )
        .execute(&self.pool)
        .await?;

        // 审计日志
        sqlx::query(
            r#"CREATE TABLE IF NOT EXISTS audit_logs (
              id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
              operation TEXT NOT NULL,
              target TEXT NOT NULL,
              target_type TEXT NOT NULL,
              player_name TEXT,
              reason TEXT,
              duration_minutes INTEGER,
              operator_name TEXT NOT NULL,
              operator_steamid TEXT,
              source TEXT NOT NULL,
              server_id UUID REFERENCES servers(id),
              server_name TEXT,
              server_port INTEGER,
              success BOOLEAN NOT NULL DEFAULT true,
              message TEXT,
              idempotency_key TEXT UNIQUE,
              created_at TIMESTAMPTZ DEFAULT now()
            )"#,
        )
        .execute(&self.pool)
        .await?;

        let audit_indexes = [
            r#"CREATE INDEX IF NOT EXISTS idx_audit_logs_created_at ON audit_logs (created_at DESC)"#,
            r#"CREATE INDEX IF NOT EXISTS idx_audit_logs_server_id ON audit_logs (server_id)"#,
            r#"CREATE INDEX IF NOT EXISTS idx_audit_logs_operation ON audit_logs (operation)"#,
        ];
        for sql in audit_indexes {
            sqlx::query(sql).execute(&self.pool).await?;
        }

        // 离线操作同步表
        sqlx::query(
            r#"CREATE TABLE IF NOT EXISTS offline_operations (
              id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
              operation TEXT NOT NULL,
              target TEXT NOT NULL,
              target_type TEXT NOT NULL,
              player_name TEXT,
              reason TEXT,
              duration_minutes INTEGER,
              operator_name TEXT NOT NULL,
              operator_steamid TEXT,
              server_id UUID NOT NULL REFERENCES servers(id),
              server_port INTEGER NOT NULL,
              created_at TIMESTAMPTZ NOT NULL,
              synced_at TIMESTAMPTZ DEFAULT now(),
              idempotency_key TEXT UNIQUE NOT NULL,
              applied BOOLEAN NOT NULL DEFAULT false,
              apply_error TEXT
            )"#,
        )
        .execute(&self.pool)
        .await?;

        let offline_indexes = [
            r#"CREATE INDEX IF NOT EXISTS idx_offline_operations_server_id ON offline_operations (server_id)"#,
            r#"CREATE INDEX IF NOT EXISTS idx_offline_operations_created_at ON offline_operations (created_at DESC)"#,
        ];
        for sql in offline_indexes {
            sqlx::query(sql).execute(&self.pool).await?;
        }

        // ========== 性能优化索引 ==========
        let perf_indexes = [
            // 封禁记录
            r#"CREATE INDEX IF NOT EXISTS idx_ban_records_status_expires ON ban_records (status, expires_at) WHERE status = 'active'"#,
            r#"CREATE INDEX IF NOT EXISTS idx_ban_records_steam_id ON ban_records (steam_id)"#,
            r#"CREATE INDEX IF NOT EXISTS idx_ban_records_ip_address ON ban_records (ip_address) WHERE ip_address IS NOT NULL"#,
            r#"CREATE INDEX IF NOT EXISTS idx_ban_records_created_at ON ban_records (created_at DESC)"#,
            // 白名单
            r#"CREATE INDEX IF NOT EXISTS idx_whitelist_requests_status ON whitelist_requests (status)"#,
            r#"CREATE INDEX IF NOT EXISTS idx_whitelist_requests_steamid64 ON whitelist_requests (steamid64)"#,
            r#"CREATE INDEX IF NOT EXISTS idx_whitelist_requests_status_steamid64 ON whitelist_requests (status, steamid64)"#,
            // 服务器
            r#"CREATE INDEX IF NOT EXISTS idx_servers_port ON servers (port)"#,
            r#"CREATE INDEX IF NOT EXISTS idx_servers_token_port ON servers (report_token, port)"#,
            // 用户
            r#"CREATE INDEX IF NOT EXISTS idx_users_steamid64 ON users (steam_id) WHERE steam_id IS NOT NULL"#,
            // Session
            r#"CREATE INDEX IF NOT EXISTS idx_sessions_user_id ON sessions (user_id)"#,
            r#"CREATE INDEX IF NOT EXISTS idx_sessions_expires_at ON sessions (expires_at)"#,
            // 管理日志
            r#"CREATE INDEX IF NOT EXISTS idx_admin_logs_operator_name ON admin_logs (operator_name)"#,
            r#"CREATE INDEX IF NOT EXISTS idx_admin_logs_created_at ON admin_logs (created_at DESC)"#,
            // 封禁操作者
            r#"CREATE INDEX IF NOT EXISTS idx_ban_records_operator_name ON ban_records (operator_name)"#,
        ];
        for sql in perf_indexes {
            sqlx::query(sql).execute(&self.pool).await?;
        }

        // 社区级访问限制
        let community_alters = [
            r#"ALTER TABLE communities ADD COLUMN IF NOT EXISTS min_rating INTEGER NOT NULL DEFAULT 0"#,
            r#"ALTER TABLE communities ADD COLUMN IF NOT EXISTS min_steam_level INTEGER NOT NULL DEFAULT 0"#,
            r#"ALTER TABLE communities ADD COLUMN IF NOT EXISTS whitelist_mode_enabled BOOLEAN NOT NULL DEFAULT false"#,
            r#"ALTER TABLE servers ADD COLUMN IF NOT EXISTS use_custom_access BOOLEAN NOT NULL DEFAULT false"#,
        ];
        for sql in community_alters {
            sqlx::query(sql).execute(&self.pool).await?;
        }
        sqlx::query(r#"UPDATE servers SET use_custom_access = true WHERE access_restriction_enabled = true OR min_rating > 0 OR min_steam_level > 0"#)
            .execute(&self.pool).await?;

        Ok(())
    }

    /// 外部服务器（RCON/A2S 查询）
    async fn migrate_external_servers_schema(&self) -> anyhow::Result<()> {
        sqlx::query(
            r#"CREATE TABLE IF NOT EXISTS external_servers (
              id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
              name TEXT NOT NULL,
              ip TEXT NOT NULL,
              port INTEGER NOT NULL,
              rcon_password TEXT NOT NULL,
              enabled BOOLEAN NOT NULL DEFAULT true,
              last_queried_at TIMESTAMPTZ,
              created_at TIMESTAMPTZ NOT NULL DEFAULT now()
            )"#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"CREATE TABLE IF NOT EXISTS external_server_status (
              server_id UUID PRIMARY KEY REFERENCES external_servers(id) ON DELETE CASCADE,
              server_name TEXT NOT NULL DEFAULT '',
              current_map TEXT NOT NULL DEFAULT '',
              player_count INTEGER NOT NULL DEFAULT 0,
              max_players INTEGER NOT NULL DEFAULT 0,
              players TEXT[] NOT NULL DEFAULT ARRAY[]::TEXT[],
              queried_at TIMESTAMPTZ NOT NULL DEFAULT now()
            )"#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(r#"CREATE INDEX IF NOT EXISTS idx_external_servers_enabled ON external_servers (enabled) WHERE enabled = true"#)
            .execute(&self.pool).await?;
        sqlx::query(r#"ALTER TABLE external_servers ALTER COLUMN rcon_password DROP NOT NULL"#)
            .execute(&self.pool)
            .await?;
        sqlx::query(r#"ALTER TABLE external_servers ADD COLUMN IF NOT EXISTS poll_interval INTEGER NOT NULL DEFAULT 30"#)
            .execute(&self.pool).await?;
        // player_api_webhooks 关联外部服务器
        let webhook_alters = [
            r#"ALTER TABLE player_api_webhooks ADD COLUMN IF NOT EXISTS external_server_ids UUID[] NOT NULL DEFAULT ARRAY[]::UUID[]"#,
            r#"ALTER TABLE player_api_webhooks ADD COLUMN IF NOT EXISTS enabled BOOLEAN NOT NULL DEFAULT true"#,
            r#"ALTER TABLE player_api_webhooks ADD COLUMN IF NOT EXISTS public_access BOOLEAN NOT NULL DEFAULT true"#,
            r#"ALTER TABLE player_api_webhooks ALTER COLUMN webhook_url DROP NOT NULL"#,
        ];
        for sql in webhook_alters {
            sqlx::query(sql).execute(&self.pool).await?;
        }

        Ok(())
    }

    /// 外部封禁 API（GOKZ.TOP Bans 接入）
    async fn migrate_external_ban_api_schema(&self) -> anyhow::Result<()> {
        sqlx::query(
            r#"CREATE TABLE IF NOT EXISTS external_ban_api_config (
              id BOOLEAN PRIMARY KEY DEFAULT true,
              enabled BOOLEAN NOT NULL DEFAULT false,
              base_url TEXT NOT NULL DEFAULT 'https://api.kzcharm.com',
              bearer_token TEXT,
              default_ban_type TEXT NOT NULL DEFAULT 'other',
              auto_sync BOOLEAN NOT NULL DEFAULT false,
              notes_template TEXT NOT NULL DEFAULT '来源: LumiAdmin
玩家: {player}
SteamID64: {steam_id}
原因: {reason}
操作人: {operator}',
              stats_template TEXT,
              updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
              CONSTRAINT external_ban_api_config_single_row CHECK (id)
            )"#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"INSERT INTO external_ban_api_config (id)
               VALUES (true)
               ON CONFLICT (id) DO NOTHING"#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"CREATE TABLE IF NOT EXISTS external_ban_api_targets (
              id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
              name TEXT NOT NULL,
              enabled BOOLEAN NOT NULL DEFAULT false,
              base_url TEXT NOT NULL DEFAULT 'https://api.kzcharm.com',
              bearer_token TEXT,
              default_ban_type TEXT NOT NULL DEFAULT 'other',
              auto_sync BOOLEAN NOT NULL DEFAULT false,
              notes_template TEXT NOT NULL DEFAULT '来源: LumiAdmin
玩家: {player}
SteamID64: {steam_id}
原因: {reason}
操作人: {operator}',
              stats_template TEXT,
              created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
              updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
            )"#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"INSERT INTO external_ban_api_targets (
                 id, name, enabled, base_url, bearer_token, default_ban_type,
                 auto_sync, notes_template, stats_template, created_at, updated_at
               )
               SELECT gen_random_uuid(), 'GOKZ.TOP', enabled, base_url, bearer_token,
                      default_ban_type, auto_sync, notes_template, stats_template, updated_at, updated_at
               FROM external_ban_api_config
               WHERE NOT EXISTS (SELECT 1 FROM external_ban_api_targets)
                 AND (enabled = true OR bearer_token IS NOT NULL)"#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"CREATE TABLE IF NOT EXISTS external_ban_syncs (
              local_ban_id UUID NOT NULL REFERENCES ban_records(id) ON DELETE CASCADE,
              target_id UUID REFERENCES external_ban_api_targets(id) ON DELETE CASCADE,
              external_uuid TEXT,
              external_id BIGINT,
              status TEXT NOT NULL DEFAULT 'pending',
              last_error TEXT,
              synced_at TIMESTAMPTZ,
              created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
              updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
              PRIMARY KEY (local_ban_id, target_id)
            )"#,
        )
        .execute(&self.pool)
        .await?;

        let sync_alters = [
            r#"ALTER TABLE external_ban_syncs ADD COLUMN IF NOT EXISTS target_id UUID"#,
            r#"UPDATE external_ban_syncs
               SET target_id = (SELECT id FROM external_ban_api_targets ORDER BY created_at ASC LIMIT 1)
               WHERE target_id IS NULL"#,
            r#"DELETE FROM external_ban_syncs WHERE target_id IS NULL"#,
            r#"ALTER TABLE external_ban_syncs ALTER COLUMN target_id SET NOT NULL"#,
            r#"DO $$ BEGIN
                IF EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'external_ban_syncs_pkey') THEN
                    ALTER TABLE external_ban_syncs DROP CONSTRAINT external_ban_syncs_pkey;
                END IF;
                IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'external_ban_syncs_local_target_pkey') THEN
                    ALTER TABLE external_ban_syncs
                    ADD CONSTRAINT external_ban_syncs_local_target_pkey
                    PRIMARY KEY (local_ban_id, target_id);
                END IF;
            END $$;"#,
            r#"DO $$ BEGIN
                IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'fk_external_ban_syncs_target_id') THEN
                    ALTER TABLE external_ban_syncs
                    ADD CONSTRAINT fk_external_ban_syncs_target_id
                    FOREIGN KEY (target_id) REFERENCES external_ban_api_targets(id) ON DELETE CASCADE;
                END IF;
            END $$;"#,
        ];
        for sql in sync_alters {
            sqlx::query(sql).execute(&self.pool).await?;
        }

        sqlx::query(
            r#"CREATE INDEX IF NOT EXISTS idx_external_ban_syncs_status
               ON external_ban_syncs (status, updated_at DESC)"#,
        )
        .execute(&self.pool)
        .await?;
        sqlx::query(
            r#"CREATE INDEX IF NOT EXISTS idx_external_ban_api_targets_enabled
               ON external_ban_api_targets (enabled, auto_sync)"#,
        )
        .execute(&self.pool)
        .await?;
        sqlx::query(
            r#"CREATE INDEX IF NOT EXISTS idx_external_ban_syncs_local_ban_id
               ON external_ban_syncs (local_ban_id)"#,
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// 封禁管理对外接入 API Key
    async fn migrate_ban_api_keys_schema(&self) -> anyhow::Result<()> {
        sqlx::query(
            r#"CREATE TABLE IF NOT EXISTS ban_api_keys (
              id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
              name TEXT NOT NULL,
              token_hash TEXT NOT NULL UNIQUE,
              token_prefix TEXT NOT NULL,
              enabled BOOLEAN NOT NULL DEFAULT true,
              created_by UUID,
              last_used_at TIMESTAMPTZ,
              created_at TIMESTAMPTZ NOT NULL DEFAULT now()
            )"#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"CREATE INDEX IF NOT EXISTS idx_ban_api_keys_enabled
               ON ban_api_keys (enabled)"#,
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// 地图等级表
    async fn migrate_map_tiers_table(&self) -> anyhow::Result<()> {
        sqlx::query(
            r#"CREATE TABLE IF NOT EXISTS map_tiers (
              map_name TEXT PRIMARY KEY,
              tier INTEGER NOT NULL
            )"#,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// 地图同步配置（游戏服务器 maps 目录 / 下载站 maps 目录）
    async fn migrate_map_sync_schema(&self) -> anyhow::Result<()> {
        sqlx::query(
            r#"CREATE TABLE IF NOT EXISTS map_sync_config (
              id BOOLEAN PRIMARY KEY DEFAULT true,
              enabled BOOLEAN NOT NULL DEFAULT false,
              auto_update BOOLEAN NOT NULL DEFAULT true,
              source_urls TEXT[] NOT NULL DEFAULT ARRAY[
                'https://files.femboykz.com/fastdl/csgo/maps/',
                'https://download.axekz.com/csgo/maps/'
              ]::TEXT[],
              map_pool_url TEXT NOT NULL DEFAULT 'https://kztimerglobal.com/api/v1.0/maps?is_validated=true&limit=999',
              check_interval_secs INTEGER NOT NULL DEFAULT 3600,
              last_checked_at TIMESTAMPTZ,
              last_status TEXT,
              last_error TEXT,
              updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
              CONSTRAINT map_sync_config_single_row CHECK (id)
            )"#,
        )
        .execute(&self.pool)
        .await?;

        let alters = [
            r#"ALTER TABLE map_sync_config ADD COLUMN IF NOT EXISTS enabled BOOLEAN NOT NULL DEFAULT false"#,
            r#"ALTER TABLE map_sync_config ADD COLUMN IF NOT EXISTS auto_update BOOLEAN NOT NULL DEFAULT true"#,
            r#"ALTER TABLE map_sync_config ADD COLUMN IF NOT EXISTS source_urls TEXT[] NOT NULL DEFAULT ARRAY[
                'https://files.femboykz.com/fastdl/csgo/maps/',
                'https://download.axekz.com/csgo/maps/'
              ]::TEXT[]"#,
            r#"ALTER TABLE map_sync_config ADD COLUMN IF NOT EXISTS map_pool_url TEXT NOT NULL DEFAULT 'https://kztimerglobal.com/api/v1.0/maps?is_validated=true&limit=999'"#,
            r#"ALTER TABLE map_sync_config ADD COLUMN IF NOT EXISTS check_interval_secs INTEGER NOT NULL DEFAULT 3600"#,
            r#"ALTER TABLE map_sync_config ADD COLUMN IF NOT EXISTS last_checked_at TIMESTAMPTZ"#,
            r#"ALTER TABLE map_sync_config ADD COLUMN IF NOT EXISTS last_status TEXT"#,
            r#"ALTER TABLE map_sync_config ADD COLUMN IF NOT EXISTS last_error TEXT"#,
            r#"ALTER TABLE map_sync_config ADD COLUMN IF NOT EXISTS updated_at TIMESTAMPTZ NOT NULL DEFAULT now()"#,
        ];
        for sql in alters {
            sqlx::query(sql).execute(&self.pool).await?;
        }

        sqlx::query(
            r#"INSERT INTO map_sync_config (id)
               VALUES (true)
               ON CONFLICT (id) DO NOTHING"#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"CREATE TABLE IF NOT EXISTS map_sync_agents (
              id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
              name TEXT NOT NULL,
              target_type TEXT NOT NULL,
              token TEXT UNIQUE NOT NULL DEFAULT gen_random_uuid(),
              enabled BOOLEAN NOT NULL DEFAULT true,
              last_seen_at TIMESTAMPTZ,
              last_inventory_at TIMESTAMPTZ,
              created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
              CONSTRAINT map_sync_agents_target_type_check CHECK (target_type IN ('game', 'download'))
            )"#,
        )
        .execute(&self.pool)
        .await?;
        sqlx::query(r#"ALTER TABLE map_sync_agents ALTER COLUMN token SET DEFAULT gen_random_uuid()"#)
            .execute(&self.pool).await?;

        sqlx::query(
            r#"CREATE TABLE IF NOT EXISTS map_sync_agent_maps (
              agent_id UUID NOT NULL REFERENCES map_sync_agents(id) ON DELETE CASCADE,
              file_name TEXT NOT NULL,
              map_name TEXT NOT NULL,
              size_bytes BIGINT NOT NULL,
              modified_at TIMESTAMPTZ,
              reported_at TIMESTAMPTZ NOT NULL DEFAULT now(),
              PRIMARY KEY (agent_id, file_name)
            )"#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"CREATE TABLE IF NOT EXISTS map_sync_remote_maps (
              map_name TEXT PRIMARY KEY,
              has_bsp BOOLEAN NOT NULL DEFAULT false,
              has_bsp_bz2 BOOLEAN NOT NULL DEFAULT false,
              updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
            )"#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"CREATE TABLE IF NOT EXISTS map_sync_tasks (
              id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
              agent_id UUID NOT NULL REFERENCES map_sync_agents(id) ON DELETE CASCADE,
              map_name TEXT NOT NULL,
              file_name TEXT NOT NULL,
              source_url TEXT NOT NULL,
              source_size_bytes BIGINT,
              source_modified_at TIMESTAMPTZ,
              status TEXT NOT NULL DEFAULT 'pending',
              reason TEXT NOT NULL,
              error TEXT,
              created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
              updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
              CONSTRAINT map_sync_tasks_status_check CHECK (status IN ('pending', 'running', 'succeeded', 'failed'))
            )"#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(r#"CREATE INDEX IF NOT EXISTS idx_map_sync_tasks_agent_status ON map_sync_tasks (agent_id, status, created_at)"#)
            .execute(&self.pool).await?;
        sqlx::query(r#"CREATE INDEX IF NOT EXISTS idx_map_sync_agent_maps_map_name ON map_sync_agent_maps (agent_id, map_name)"#)
            .execute(&self.pool).await?;
        Ok(())
    }
    /// 通知表
    async fn migrate_notifications_schema(&self) -> anyhow::Result<()> {
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

    /// 封禁申诉表
    async fn migrate_ban_appeals_schema(&self) -> anyhow::Result<()> {
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
        Ok(())
    }

    async fn migrate_appeal_files_schema(&self) -> anyhow::Result<()> {
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

    async fn migrate_ban_files_schema(&self) -> anyhow::Result<()> {
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

    async fn migrate_player_reports_schema(&self) -> anyhow::Result<()> {
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

    async fn migrate_player_internal_notes_schema(&self) -> anyhow::Result<()> {
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

    pub async fn seed(&self, config: &Config) -> anyhow::Result<()> {
        let password_hash = crate::password::hash_password(&config.dev_password)?;
        sqlx::query(
            r#"INSERT INTO users (id, username, display_name, password_hash, role, steam_id, remark)
               VALUES
               ('22222222-2222-2222-2222-222222222222', $1, 'DevAdmin', $2, 'developer', '76561198000000000', '开发管理员')
               ON CONFLICT (username) DO UPDATE SET password_hash = $2"#,
        )
        .bind(&config.dev_username)
        .bind(&password_hash)
        .execute(&self.pool)
        .await?;

        // 修复所有存储为明文的密码（非 $argon2 开头的）
        let rows: Vec<(uuid::Uuid, String)> = sqlx::query_as(
            "SELECT id, password_hash FROM users WHERE password_hash NOT LIKE '$argon2%'",
        )
        .fetch_all(&self.pool)
        .await?;

        for (user_id, plain) in &rows {
            let hashed = crate::password::hash_password(plain)?;
            sqlx::query("UPDATE users SET password_hash = $1 WHERE id = $2")
                .bind(&hashed)
                .bind(user_id)
                .execute(&self.pool)
                .await?;
        }
        if !rows.is_empty() {
            tracing::info!(
                count = rows.len(),
                "migrated plaintext passwords to argon2 hashes"
            );
        }

        sqlx::query(
            r#"UPDATE map_sync_config
               SET map_pool_url = $1
               WHERE id = true
                 AND (
                   btrim(map_pool_url) = ''
                   OR map_pool_url = 'https://kztimerglobal.com/api/v1.0/maps?is_validated=true&limit=999'
                 )"#,
        )
        .bind(&config.map_sync_map_pool_url)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// 添加缺失的外键约束和复合索引
    async fn migrate_adds_missing_constraints_and_indexes(&self) -> anyhow::Result<()> {
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

    #[cfg(not(test))]
    async fn migrate_query_performance_indexes(&self) -> anyhow::Result<()> {
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

    #[cfg(not(test))]
    async fn pg_trgm_schema(&self) -> Option<String> {
        if let Err(error) = sqlx::query("CREATE EXTENSION IF NOT EXISTS pg_trgm")
            .execute(&self.pool)
            .await
        {
            tracing::warn!(
                error = %error,
                "pg_trgm extension could not be created; checking whether it already exists"
            );
        }

        match sqlx::query_scalar::<_, String>(
            r#"SELECT n.nspname
               FROM pg_extension e
               JOIN pg_namespace n ON n.oid = e.extnamespace
               WHERE e.extname = 'pg_trgm'"#,
        )
        .fetch_one(&self.pool)
        .await
        {
            Ok(schema) => Some(schema),
            Err(error) => {
                tracing::warn!(
                    error = %error,
                    "failed to check pg_trgm extension; skipping trigram search indexes"
                );
                None
            }
        }
    }
}

#[cfg(not(test))]
fn quote_ident(identifier: &str) -> String {
    format!(r#""{}""#, identifier.replace('"', r#""""#))
}

#[cfg(test)]
mod tests {
    use super::Database;
    use crate::{
        config::Config,
        services::{dashboard_service, public_service, whitelist_service},
    };
    use crate::test_util;

    use sqlx::postgres::PgPoolOptions;
    use uuid::Uuid;

    fn schema_url(base_url: &str, schema: &str) -> String {
        crate::test_util::schema_url(base_url, schema)
    }

    async fn create_schema(base_url: &str, schema: &str) {
        crate::test_util::create_schema(base_url, schema).await;
    }

    async fn drop_schema(base_url: &str, schema: &str) {
        crate::test_util::drop_schema(base_url, schema).await;
    }

    #[tokio::test]
    async fn dashboard_metrics_count_online_players_from_player_array() {
        let config = Config::from_env();
        let base_url = config.database_url.clone();
        let schema = format!("test_{}", Uuid::new_v4().simple());
        let scoped_url = schema_url(&base_url, &schema);

        create_schema(&base_url, &schema).await;

        let result = async {
            let db = Database::connect_for_test(&scoped_url).await?;
            db.migrate().await?;

            let community_id = Uuid::new_v4();
            sqlx::query(r#"INSERT INTO communities (id, name) VALUES ($1, $2)"#)
                .bind(community_id)
                .bind("像素方块社区")
                .execute(&db.pool)
                .await?;

            sqlx::query(
                r#"INSERT INTO servers (id, community_id, name, ip, port, rcon_password, status, players)
                   VALUES ($1, $2, $3, $4, $5, $6, 'online', $7),
                          ($8, $2, $9, $10, $11, $12, 'offline', $13)"#,
            )
            .bind(Uuid::new_v4())
            .bind(community_id)
            .bind("在线服")
            .bind("127.0.0.1")
            .bind(25575_i32)
            .bind("secret")
            .bind(vec!["玩家甲".to_string(), "玩家乙".to_string(), "玩家丙".to_string()])
            .bind(Uuid::new_v4())
            .bind("离线服")
            .bind("127.0.0.2")
            .bind(25576_i32)
            .bind("secret")
            .bind(vec!["不应统计".to_string()])
            .execute(&db.pool)
            .await?;

            let metrics = dashboard_service::get_metrics(&db).await?;
            assert_eq!(metrics.online_players, 3);

            Ok::<(), anyhow::Error>(())
        }
        .await;

        drop_schema(&base_url, &schema).await;
        result.unwrap();
    }

    #[tokio::test]
    async fn dashboard_metrics_include_all_admin_roles_in_preview() {
        let config = Config::from_env();
        let base_url = config.database_url.clone();
        let schema = format!("test_{}", Uuid::new_v4().simple());
        let scoped_url = schema_url(&base_url, &schema);

        create_schema(&base_url, &schema).await;

        let result = async {
            let db = Database::connect_for_test(&scoped_url).await?;
            db.migrate().await?;

            sqlx::query(
                r#"INSERT INTO users (id, username, display_name, password_hash, role, remark)
                   VALUES ($1, 'admin_preview_admin', 'Admin One', 'pw', 'admin', 'Admin Remark'),
                          ($2, 'admin_preview_dev', 'Dev One', 'pw', 'developer', NULL),
                          ($3, 'admin_preview_normal', 'Normal One', 'pw', 'normal', ''),
                          ($4, 'admin_preview_guest', 'Guest One', 'pw', 'guest', 'Guest Remark')"#,
            )
            .bind(Uuid::new_v4())
            .bind(Uuid::new_v4())
            .bind(Uuid::new_v4())
            .bind(Uuid::new_v4())
            .execute(&db.pool)
            .await?;

            let metrics = dashboard_service::get_metrics(&db).await?;
            let mut preview_names: Vec<_> = metrics
                .admin_preview
                .iter()
                .map(|item| item.display_name.as_str())
                .collect();
            preview_names.sort_unstable();

            assert_eq!(metrics.admins, 3);
            assert_eq!(
                preview_names,
                vec![
                    "Admin Remark",
                    "admin_preview_dev",
                    "admin_preview_normal"
                ]
            );
            assert!(metrics
                .admin_preview
                .iter()
                .all(|item| item.status == "可用"));

            Ok::<(), anyhow::Error>(())
        }
        .await;

        drop_schema(&base_url, &schema).await;
        result.unwrap();
    }

    #[tokio::test]
    async fn migrate_converts_legacy_players_text_to_text_array() {
        let config = Config::from_env();
        let base_url = config.database_url;
        let schema = format!("test_{}", Uuid::new_v4().simple());
        let scoped_url = schema_url(&base_url, &schema);

        create_schema(&base_url, &schema).await;

        let result = async {
            let db = Database::connect_for_test(&scoped_url).await?;

            sqlx::query(
                r#"CREATE TABLE communities (
                  id UUID PRIMARY KEY,
                  name TEXT NOT NULL,
                  created_by UUID,
                  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
                )"#,
            )
            .execute(&db.pool)
            .await?;

            sqlx::query(
                r#"CREATE TABLE servers (
                  id UUID PRIMARY KEY,
                  community_id UUID NOT NULL REFERENCES communities(id) ON DELETE CASCADE,
                  name TEXT NOT NULL,
                  ip TEXT,
                  port INTEGER,
                  rcon_password TEXT,
                  note TEXT,
                  status TEXT NOT NULL DEFAULT 'untested',
                  players TEXT NOT NULL DEFAULT '',
                  last_tested_at TIMESTAMPTZ,
                  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
                )"#,
            )
            .execute(&db.pool)
            .await?;

            let community_id = Uuid::new_v4();
            let server_id = Uuid::new_v4();

            sqlx::query(r#"INSERT INTO communities (id, name) VALUES ($1, $2)"#)
                .bind(community_id)
                .bind("像素方块社区")
                .execute(&db.pool)
                .await?;

            sqlx::query(
                r#"INSERT INTO servers (id, community_id, name, ip, port, rcon_password, status, players)
                   VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"#,
            )
            .bind(server_id)
            .bind(community_id)
            .bind("一号服")
            .bind("127.0.0.1")
            .bind(25575_i32)
            .bind("secret")
            .bind("online")
            .bind("{测试玩家A,测试玩家B}")
            .execute(&db.pool)
            .await?;

            db.migrate().await?;

            let players_udt_name: (String,) = sqlx::query_as(
                r#"SELECT udt_name
                   FROM information_schema.columns
                   WHERE table_schema = current_schema()
                     AND table_name = 'servers'
                     AND column_name = 'players'"#,
            )
            .fetch_one(&db.pool)
            .await?;
            assert_eq!(players_udt_name.0, "_text");

            let stored_players: (Vec<String>,) = sqlx::query_as(
                r#"SELECT players FROM servers WHERE id = $1"#,
            )
            .bind(server_id)
            .fetch_one(&db.pool)
            .await?;
            assert_eq!(stored_players.0, vec!["测试玩家A", "测试玩家B"]);

            Ok::<(), anyhow::Error>(())
        }
        .await;

        drop_schema(&base_url, &schema).await;
        result.unwrap();
    }

    #[tokio::test]
    async fn migrate_adds_server_report_tokens_and_online_players_table() {
        let config = Config::from_env();
        let base_url = config.database_url;
        let schema = format!("test_{}", Uuid::new_v4().simple());
        let scoped_url = schema_url(&base_url, &schema);

        create_schema(&base_url, &schema).await;

        let result = async {
            let db = Database::connect_for_test(&scoped_url).await?;

            sqlx::query(
                r#"CREATE TABLE communities (
                  id UUID PRIMARY KEY,
                  name TEXT NOT NULL,
                  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
                )"#,
            )
            .execute(&db.pool)
            .await?;

            sqlx::query(
                r#"CREATE TABLE servers (
                  id UUID PRIMARY KEY,
                  community_id UUID NOT NULL REFERENCES communities(id) ON DELETE CASCADE,
                  name TEXT NOT NULL,
                  ip TEXT NOT NULL,
                  port INTEGER NOT NULL,
                  rcon_password TEXT NOT NULL,
                  status TEXT NOT NULL DEFAULT 'untested',
                  players TEXT[] NOT NULL DEFAULT ARRAY[]::TEXT[],
                  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
                )"#,
            )
            .execute(&db.pool)
            .await?;

            let community_id = Uuid::new_v4();
            let server_id = Uuid::new_v4();

            sqlx::query(r#"INSERT INTO communities (id, name) VALUES ($1, $2)"#)
                .bind(community_id)
                .bind("Token 迁移社区")
                .execute(&db.pool)
                .await?;

            sqlx::query(
                r#"INSERT INTO servers (id, community_id, name, ip, port, rcon_password, status, players)
                   VALUES ($1, $2, $3, $4, $5, $6, 'online', $7)"#,
            )
            .bind(server_id)
            .bind(community_id)
            .bind("一号服")
            .bind("127.0.0.1")
            .bind(27015_i32)
            .bind("secret")
            .bind(vec!["测试玩家".to_string()])
            .execute(&db.pool)
            .await?;

            db.migrate().await?;

            let token_and_reported_at: (String, Option<chrono::DateTime<chrono::Utc>>) = sqlx::query_as(
                r#"SELECT report_token, last_reported_at FROM servers WHERE id = $1"#,
            )
            .bind(server_id)
            .fetch_one(&db.pool)
            .await?;
            assert!(!token_and_reported_at.0.is_empty());
            assert!(token_and_reported_at.1.is_none());

            let table_count: (i64,) = sqlx::query_as(
                r#"SELECT COUNT(*)
                   FROM information_schema.tables
                   WHERE table_schema = current_schema()
                     AND table_name = 'server_online_players'"#,
            )
            .fetch_one(&db.pool)
            .await?;
            assert_eq!(table_count.0, 1);

            Ok::<(), anyhow::Error>(())
        }
        .await;

        drop_schema(&base_url, &schema).await;
        result.unwrap();
    }

    #[tokio::test]
    async fn migrate_creates_player_api_distribution_config() {
        let config = Config::from_env();
        let base_url = config.database_url;
        let schema = format!("test_{}", Uuid::new_v4().simple());
        let scoped_url = schema_url(&base_url, &schema);

        create_schema(&base_url, &schema).await;

        let result = async {
            let db = Database::connect_for_test(&scoped_url).await?;
            db.migrate().await?;

            let config_row: (i32, i32) = sqlx::query_as(
                r#"SELECT max_api_count, interval_seconds FROM player_api_config WHERE id = true"#,
            )
            .fetch_one(&db.pool)
            .await?;
            assert_eq!(config_row, (3, 30));

            let webhook_columns: Vec<(String,)> = sqlx::query_as(
                r#"SELECT column_name
                   FROM information_schema.columns
                   WHERE table_schema = current_schema()
                     AND table_name = 'player_api_webhooks'
                   ORDER BY column_name"#,
            )
            .fetch_all(&db.pool)
            .await?;
            let names = webhook_columns
                .into_iter()
                .map(|row| row.0)
                .collect::<Vec<_>>();
            assert!(names.contains(&"webhook_url".to_string()));
            assert!(names.contains(&"secret".to_string()));
            assert!(names.contains(&"server_ids".to_string()));
            assert!(names.contains(&"last_status".to_string()));
            assert!(names.contains(&"last_error".to_string()));
            assert!(names.contains(&"last_dispatched_at".to_string()));

            Ok::<(), anyhow::Error>(())
        }
        .await;

        drop_schema(&base_url, &schema).await;
        result.unwrap();
    }

    #[tokio::test]
    async fn migrate_expands_whitelist_requests_schema() {
        let config = Config::from_env();
        let base_url = config.database_url;
        let schema = format!("test_{}", Uuid::new_v4().simple());
        let scoped_url = schema_url(&base_url, &schema);

        create_schema(&base_url, &schema).await;

        let result = async {
            let db = Database::connect_for_test(&scoped_url).await?;
            db.migrate().await?;

            let columns: Vec<(String,)> = sqlx::query_as(
                r#"SELECT column_name
                   FROM information_schema.columns
                   WHERE table_schema = current_schema()
                     AND table_name = 'whitelist_requests'
                   ORDER BY column_name"#,
            )
            .fetch_all(&db.pool)
            .await?;

            let names = columns.into_iter().map(|x| x.0).collect::<Vec<_>>();
            assert!(names.contains(&"steamid64".to_string()));
            assert!(names.contains(&"steamid".to_string()));
            assert!(names.contains(&"steamid3".to_string()));
            assert!(names.contains(&"rejection_reason".to_string()));
            assert!(names.contains(&"revoked_at".to_string()));
            assert!(names.contains(&"approved_by".to_string()));

            Ok::<(), anyhow::Error>(())
        }
        .await;

        drop_schema(&base_url, &schema).await;
        result.unwrap();
    }

    #[tokio::test]
    async fn migrate_supports_legacy_whitelist_requests_without_created_at() {
        let config = Config::from_env();
        let base_url = config.database_url.clone();
        let schema = format!("test_{}", Uuid::new_v4().simple());
        let scoped_url = schema_url(&base_url, &schema);

        create_schema(&base_url, &schema).await;

        let result = async {
            let db = Database::connect_for_test(&scoped_url).await?;

            sqlx::query(
                r#"CREATE TABLE whitelist_requests (
                  id UUID PRIMARY KEY,
                  player_name TEXT NOT NULL,
                  steam_id64 TEXT NOT NULL,
                  steam_id TEXT,
                  steam_profile_url TEXT,
                  source TEXT,
                  status TEXT NOT NULL,
                  reject_reason TEXT,
                  applied_at TIMESTAMPTZ,
                  reviewed_at TIMESTAMPTZ,
                  reviewed_by TEXT
                )"#,
            )
            .execute(&db.pool)
            .await?;

            let rejected_id = Uuid::new_v4();
            let approved_id = Uuid::new_v4();
            let applied_at = chrono::Utc::now() - chrono::Duration::days(1);
            let reviewed_at = chrono::Utc::now();

            sqlx::query(
                r#"INSERT INTO whitelist_requests (
                  id, player_name, steam_id64, steam_id, steam_profile_url, source,
                  status, reject_reason, applied_at, reviewed_at, reviewed_by
                ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)"#,
            )
            .bind(rejected_id)
            .bind("旧版被拒玩家")
            .bind("76561198000000001")
            .bind("STEAM_0:1:1")
            .bind("https://steamcommunity.com/profiles/76561198000000001")
            .bind("public")
            .bind("rejected")
            .bind("资料不完整")
            .bind(applied_at)
            .bind(reviewed_at)
            .bind("Alex")
            .execute(&db.pool)
            .await?;

            sqlx::query(
                r#"INSERT INTO whitelist_requests (
                  id, player_name, steam_id64, steam_id, steam_profile_url, source,
                  status, reject_reason, applied_at, reviewed_at, reviewed_by
                ) VALUES ($1, $2, $3, $4, $5, $6, $7, NULL, $8, $9, $10)"#,
            )
            .bind(approved_id)
            .bind("旧版已通过玩家")
            .bind("76561198000000002")
            .bind("STEAM_0:0:2")
            .bind("https://steamcommunity.com/profiles/76561198000000002")
            .bind("manual")
            .bind("approved")
            .bind(applied_at)
            .bind(reviewed_at)
            .bind("DevAdmin")
            .execute(&db.pool)
            .await?;

            db.migrate().await?;

            let items = whitelist_service::list_whitelist(
                &db,
                &crate::routes::ListQuery {
                    search: None,
                    status: None,
                    page: None,
                    page_size: None,
                },
            )
            .await?;
            assert_eq!(items.items.len(), 2);
            assert!(items.items.iter().any(|item| {
                item.nickname == "旧版被拒玩家"
                    && item.steamid64 == "76561198000000001"
                    && item.profile_url.as_deref()
                        == Some("https://steamcommunity.com/profiles/76561198000000001")
                    && item.rejection_reason.as_deref() == Some("资料不完整")
                    && item.rejected_by.as_deref() == Some("Alex")
                    && item.rejected_at.is_some()
            }));
            assert!(items.items.iter().any(|item| {
                item.nickname == "旧版已通过玩家"
                    && item.steamid64 == "76561198000000002"
                    && item.approved_by.as_deref() == Some("DevAdmin")
                    && item.approved_at.is_some()
            }));

            let public_items = public_service::list_public_whitelist(
                &db,
                &crate::routes::ListQuery {
                    search: None,
                    status: None,
                    page: None,
                    page_size: None,
                },
            )
            .await?;
            assert_eq!(public_items.items.len(), 1);
            assert_eq!(public_items.items[0].nickname, "旧版已通过玩家");
            assert_eq!(public_items.items[0].steamid64, "76561198000000002");
            assert!(public_items.items[0].approved_at.is_some());

            Ok::<(), anyhow::Error>(())
        }
        .await;

        drop_schema(&base_url, &schema).await;
        result.unwrap();
    }

    #[tokio::test]
    async fn migrate_keeps_duplicate_whitelist_requests_steamid64_records() {
        let config = Config::from_env();
        let base_url = config.database_url.clone();
        let schema = format!("test_{}", Uuid::new_v4().simple());
        let scoped_url = schema_url(&base_url, &schema);

        create_schema(&base_url, &schema).await;

        let result = async {
            let db = Database::connect_for_test(&scoped_url).await?;

            sqlx::query(
                r#"CREATE TABLE whitelist_requests (
                  id UUID PRIMARY KEY,
                  nickname TEXT NOT NULL,
                  steam_id TEXT,
                  steamid64 TEXT,
                  status TEXT NOT NULL,
                  applied_at TIMESTAMPTZ,
                  updated_at TIMESTAMPTZ
                )"#,
            )
            .execute(&db.pool)
            .await?;

            let duplicate_steamid64 = "76561198000000021";
            let first_id = Uuid::new_v4();
            let second_id = Uuid::new_v4();
            let first_time = chrono::Utc::now() - chrono::Duration::hours(2);
            let second_time = chrono::Utc::now() - chrono::Duration::hours(1);

            sqlx::query(
                r#"INSERT INTO whitelist_requests (id, nickname, steam_id, steamid64, status, applied_at, updated_at)
                   VALUES ($1, $2, $3, $4, 'pending', $5, $5)"#,
            )
            .bind(first_id)
            .bind("旧重复记录")
            .bind("STEAM_0:1:1")
            .bind(duplicate_steamid64)
            .bind(first_time)
            .execute(&db.pool)
            .await?;

            sqlx::query(
                r#"INSERT INTO whitelist_requests (id, nickname, steam_id, steamid64, status, applied_at, updated_at)
                   VALUES ($1, $2, $3, $4, 'pending', $5, $5)"#,
            )
            .bind(second_id)
            .bind("新重复记录")
            .bind("STEAM_0:1:1")
            .bind(duplicate_steamid64)
            .bind(second_time)
            .execute(&db.pool)
            .await?;

            db.migrate().await?;

            let count: (i64,) = sqlx::query_as(
                r#"SELECT COUNT(*) FROM whitelist_requests WHERE steamid64 = $1"#,
            )
            .bind(duplicate_steamid64)
            .fetch_one(&db.pool)
            .await?;
            assert_eq!(count.0, 2);

            Ok::<(), anyhow::Error>(())
        }
        .await;

        drop_schema(&base_url, &schema).await;
        result.unwrap();
    }

    #[tokio::test]
    async fn migrate_expands_users_schema_for_admin_management() {
        let config = Config::from_env();
        let base_url = config.database_url.clone();
        let schema = format!("test_{}", Uuid::new_v4().simple());
        let scoped_url = schema_url(&base_url, &schema);

        create_schema(&base_url, &schema).await;

        let result = async {
            let db = Database::connect_for_test(&scoped_url).await?;
            db.migrate().await?;
            db.seed(&config).await?;

            sqlx::query(
                r#"INSERT INTO users (id, username, display_name, password_hash, role)
                   VALUES ('11111111-1111-1111-1111-111111111111', 'test-admin', 'Admin', 'test', 'admin'),
                          ('33333333-3333-3333-3333-333333333333', 'test-normal', 'Normal', 'test', 'normal')
                   ON CONFLICT (id) DO NOTHING"#,
            )
            .execute(&db.pool)
            .await?;

            let columns = sqlx::query_as::<_, (String,)>(
                r#"
                SELECT column_name
                FROM information_schema.columns
                WHERE table_schema = current_schema()
                  AND table_name = 'users'
                ORDER BY column_name
                "#,
            )
            .fetch_all(&db.pool)
            .await?;

            let names: Vec<String> = columns.into_iter().map(|row| row.0).collect();
            assert!(names.contains(&"steam_id".to_string()));
            assert!(names.contains(&"remark".to_string()));

            let roles = sqlx::query_as::<_, (String,)>(
                r#"SELECT role FROM users ORDER BY role"#,
            )
            .fetch_all(&db.pool)
            .await?;

            let role_values: Vec<String> = roles.into_iter().map(|row| row.0).collect();
            assert!(role_values.contains(&"admin".to_string()));
            assert!(role_values.contains(&"developer".to_string()));
            assert!(role_values.contains(&"normal".to_string()));

            Ok::<(), anyhow::Error>(())
        }
        .await;

        drop_schema(&base_url, &schema).await;
        result.unwrap();
    }

    #[tokio::test]
    async fn migrate_expands_ban_records_missing_manual_ban_columns() {
        let config = Config::from_env();
        let base_url = config.database_url.clone();
        let schema = format!("test_{}", Uuid::new_v4().simple());
        let scoped_url = schema_url(&base_url, &schema);

        create_schema(&base_url, &schema).await;

        let result = async {
            let db = Database::connect_for_test(&scoped_url).await?;

            sqlx::query(
                r#"CREATE TABLE ban_records (
                  id UUID PRIMARY KEY,
                  steam_id TEXT NOT NULL,
                  status TEXT NOT NULL,
                  operator_name TEXT NOT NULL,
                  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
                )"#,
            )
            .execute(&db.pool)
            .await?;

            db.migrate().await?;

            let columns = sqlx::query_as::<_, (String,)>(
                r#"SELECT column_name
                   FROM information_schema.columns
                   WHERE table_schema = current_schema()
                     AND table_name = 'ban_records'
                   ORDER BY column_name"#,
            )
            .fetch_all(&db.pool)
            .await?;

            let names: Vec<String> = columns.into_iter().map(|row| row.0).collect();
            assert!(names.contains(&"player".to_string()));
            assert!(names.contains(&"ip_address".to_string()));
            assert!(names.contains(&"server_name".to_string()));
            assert!(names.contains(&"ban_type".to_string()));
            assert!(names.contains(&"reason".to_string()));

            Ok::<(), anyhow::Error>(())
        }
        .await;

        drop_schema(&base_url, &schema).await;
        result.unwrap();
    }

    #[tokio::test]
    async fn migrate_expands_ban_records_for_manual_ban_creation() {
        let config = Config::from_env();
        let base_url = config.database_url.clone();
        let schema = format!("test_{}", Uuid::new_v4().simple());
        let scoped_url = schema_url(&base_url, &schema);

        create_schema(&base_url, &schema).await;

        let result = async {
            let db = Database::connect_for_test(&scoped_url).await?;

            sqlx::query(
                r#"CREATE TABLE ban_records (
                  id UUID PRIMARY KEY,
                  player TEXT NOT NULL,
                  steam_id TEXT NOT NULL,
                  ip_address TEXT NOT NULL,
                  server_name TEXT NOT NULL,
                  status TEXT NOT NULL,
                  operator_name TEXT NOT NULL,
                  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
                )"#,
            )
            .execute(&db.pool)
            .await?;

            db.migrate().await?;

            let columns = sqlx::query_as::<_, (String, String, String)>(
                r#"
                SELECT column_name, is_nullable, data_type
                FROM information_schema.columns
                WHERE table_schema = current_schema()
                  AND table_name = 'ban_records'
                ORDER BY column_name
                "#,
            )
            .fetch_all(&db.pool)
            .await?;

            let column = |name: &str| {
                columns
                    .iter()
                    .find(|row| row.0 == name)
                    .expect("ban_records column exists")
            };

            assert_eq!(column("player").1, "YES");
            assert_eq!(column("ip_address").1, "YES");
            assert_eq!(column("server_name").1, "YES");
            assert_eq!(column("ban_type").1, "NO");
            assert_eq!(column("reason").1, "NO");

            Ok::<(), anyhow::Error>(())
        }
        .await;

        drop_schema(&base_url, &schema).await;
        result.unwrap();
    }

    #[tokio::test]
    async fn migrate_adds_plugin_ban_fields() {
        let config = Config::from_env();
        let base_url = config.database_url.clone();
        let schema = format!("test_{}", Uuid::new_v4().simple());
        let scoped_url = schema_url(&base_url, &schema);

        create_schema(&base_url, &schema).await;

        let result = async {
            let db = Database::connect_for_test(&scoped_url).await?;
            db.migrate().await?;

            let columns = sqlx::query_as::<_, (String,)>(
                r#"SELECT column_name
                   FROM information_schema.columns
                   WHERE table_schema = current_schema()
                     AND table_name = 'ban_records'"#,
            )
            .fetch_all(&db.pool)
            .await?;
            let names = columns.into_iter().map(|row| row.0).collect::<Vec<_>>();

            assert!(names.contains(&"duration_minutes".to_string()));
            assert!(names.contains(&"expires_at".to_string()));
            assert!(names.contains(&"source".to_string()));
            assert!(names.contains(&"server_id".to_string()));
            assert!(names.contains(&"server_port".to_string()));
            assert!(names.contains(&"removed_reason".to_string()));
            assert!(names.contains(&"removed_by".to_string()));
            assert!(names.contains(&"removed_at".to_string()));

            let server_columns = sqlx::query_as::<_, (String,)>(
                r#"SELECT column_name
                   FROM information_schema.columns
                   WHERE table_schema = current_schema()
                     AND table_name = 'servers'"#,
            )
            .fetch_all(&db.pool)
            .await?;
            let server_names = server_columns
                .into_iter()
                .map(|row| row.0)
                .collect::<Vec<_>>();
            assert!(server_names.contains(&"report_token".to_string()));

            Ok::<(), anyhow::Error>(())
        }
        .await;

        drop_schema(&base_url, &schema).await;
        result.unwrap();
    }

    #[tokio::test]
    async fn migrate_adds_server_access_control_fields_and_cache_table() {
        let config = Config::from_env();
        let base_url = config.database_url.clone();
        let schema = format!("test_{}", Uuid::new_v4().simple());
        let scoped_url = schema_url(&base_url, &schema);

        create_schema(&base_url, &schema).await;

        let result = async {
            let db = Database::connect_for_test(&scoped_url).await?;
            db.migrate().await?;

            let server_columns = sqlx::query_scalar::<_, String>(
                r#"SELECT column_name
                   FROM information_schema.columns
                   WHERE table_schema = current_schema()
                     AND table_name = 'servers'
                     AND column_name IN (
                        'access_restriction_enabled',
                        'min_rating',
                        'min_steam_level',
                        'whitelist_mode_enabled'
                     )
                   ORDER BY column_name"#,
            )
            .fetch_all(&db.pool)
            .await?;

            assert_eq!(
                server_columns,
                vec![
                    "access_restriction_enabled".to_string(),
                    "min_rating".to_string(),
                    "min_steam_level".to_string(),
                    "whitelist_mode_enabled".to_string(),
                ]
            );

            let cache_columns = sqlx::query_scalar::<_, String>(
                r#"SELECT column_name
                   FROM information_schema.columns
                   WHERE table_schema = current_schema()
                     AND table_name = 'player_access_cache'
                     AND column_name IN ('steamid64', 'rating', 'steam_level', 'expires_at', 'updated_at')
                   ORDER BY column_name"#,
            )
            .fetch_all(&db.pool)
            .await?;

            assert_eq!(
                cache_columns,
                vec![
                    "expires_at".to_string(),
                    "rating".to_string(),
                    "steam_level".to_string(),
                    "steamid64".to_string(),
                    "updated_at".to_string(),
                ]
            );

            Ok::<(), anyhow::Error>(())
        }
        .await;

        drop_schema(&base_url, &schema).await;
        result.unwrap();
    }
}

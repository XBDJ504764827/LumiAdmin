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
              report_token TEXT UNIQUE NOT NULL DEFAULT md5(random()::TEXT || clock_timestamp()::TEXT),
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

        sqlx::query(r#"ALTER TABLE ban_records ADD COLUMN IF NOT EXISTS player TEXT"#)
            .execute(&self.pool)
            .await?;
        sqlx::query(r#"ALTER TABLE ban_records ADD COLUMN IF NOT EXISTS ip_address TEXT"#)
            .execute(&self.pool)
            .await?;
        sqlx::query(r#"ALTER TABLE ban_records ADD COLUMN IF NOT EXISTS server_name TEXT"#)
            .execute(&self.pool)
            .await?;
        sqlx::query(r#"ALTER TABLE ban_records ADD COLUMN IF NOT EXISTS ban_type TEXT NOT NULL DEFAULT 'steam'"#)
            .execute(&self.pool)
            .await?;
        sqlx::query(r#"ALTER TABLE ban_records ADD COLUMN IF NOT EXISTS reason TEXT NOT NULL DEFAULT '未填写'"#)
            .execute(&self.pool)
            .await?;
        sqlx::query(r#"ALTER TABLE ban_records ADD COLUMN IF NOT EXISTS duration_minutes INTEGER NOT NULL DEFAULT 0"#)
            .execute(&self.pool)
            .await?;
        sqlx::query(r#"ALTER TABLE ban_records ADD COLUMN IF NOT EXISTS expires_at TIMESTAMPTZ"#)
            .execute(&self.pool)
            .await?;
        sqlx::query(r#"ALTER TABLE ban_records ADD COLUMN IF NOT EXISTS source TEXT NOT NULL DEFAULT 'manual'"#)
            .execute(&self.pool)
            .await?;
        sqlx::query(r#"ALTER TABLE ban_records ADD COLUMN IF NOT EXISTS server_id UUID"#)
            .execute(&self.pool)
            .await?;
        sqlx::query(r#"ALTER TABLE ban_records ADD COLUMN IF NOT EXISTS server_port INTEGER"#)
            .execute(&self.pool)
            .await?;
        sqlx::query(r#"ALTER TABLE ban_records ADD COLUMN IF NOT EXISTS removed_reason TEXT"#)
            .execute(&self.pool)
            .await?;
        sqlx::query(r#"ALTER TABLE ban_records ADD COLUMN IF NOT EXISTS removed_by TEXT"#)
            .execute(&self.pool)
            .await?;
        sqlx::query(r#"ALTER TABLE ban_records ADD COLUMN IF NOT EXISTS removed_at TIMESTAMPTZ"#)
            .execute(&self.pool)
            .await?;
        sqlx::query(r#"ALTER TABLE ban_records ALTER COLUMN player DROP NOT NULL"#)
            .execute(&self.pool)
            .await?;
        sqlx::query(r#"ALTER TABLE ban_records ALTER COLUMN ip_address DROP NOT NULL"#)
            .execute(&self.pool)
            .await?;
        sqlx::query(r#"ALTER TABLE ban_records ALTER COLUMN server_name DROP NOT NULL"#)
            .execute(&self.pool)
            .await?;

        sqlx::query(r#"ALTER TABLE users ADD COLUMN IF NOT EXISTS steam_id TEXT"#)
            .execute(&self.pool)
            .await?;
        sqlx::query(r#"ALTER TABLE users ADD COLUMN IF NOT EXISTS remark TEXT"#)
            .execute(&self.pool)
            .await?;
        sqlx::query(r#"ALTER TABLE users ADD COLUMN IF NOT EXISTS enabled BOOLEAN NOT NULL DEFAULT true"#)
            .execute(&self.pool)
            .await?;

        sqlx::query(r#"ALTER TABLE communities ADD COLUMN IF NOT EXISTS created_by UUID"#)
            .execute(&self.pool)
            .await?;

        sqlx::query(r#"ALTER TABLE servers ADD COLUMN IF NOT EXISTS ip TEXT"#)
            .execute(&self.pool)
            .await?;
        sqlx::query(r#"ALTER TABLE servers ADD COLUMN IF NOT EXISTS port INTEGER"#)
            .execute(&self.pool)
            .await?;
        sqlx::query(r#"ALTER TABLE servers ADD COLUMN IF NOT EXISTS rcon_password TEXT"#)
            .execute(&self.pool)
            .await?;
        sqlx::query(r#"ALTER TABLE servers ADD COLUMN IF NOT EXISTS report_token TEXT"#)
            .execute(&self.pool)
            .await?;
        sqlx::query(r#"ALTER TABLE servers ADD COLUMN IF NOT EXISTS note TEXT"#)
            .execute(&self.pool)
            .await?;
        sqlx::query(r#"ALTER TABLE servers ADD COLUMN IF NOT EXISTS last_tested_at TIMESTAMPTZ"#)
            .execute(&self.pool)
            .await?;
        sqlx::query(r#"UPDATE servers SET report_token = md5(random()::TEXT || clock_timestamp()::TEXT) WHERE report_token IS NULL OR btrim(report_token) = ''"#)
            .execute(&self.pool)
            .await?;
        sqlx::query(r#"ALTER TABLE servers ALTER COLUMN report_token SET NOT NULL"#)
            .execute(&self.pool)
            .await?;
        sqlx::query(r#"CREATE UNIQUE INDEX IF NOT EXISTS idx_servers_report_token_unique ON servers (report_token)"#)
            .execute(&self.pool)
            .await?;
        sqlx::query(r#"ALTER TABLE servers ADD COLUMN IF NOT EXISTS last_reported_at TIMESTAMPTZ"#)
            .execute(&self.pool)
            .await?;
        sqlx::query(r#"ALTER TABLE servers ADD COLUMN IF NOT EXISTS access_restriction_enabled BOOLEAN NOT NULL DEFAULT false"#)
            .execute(&self.pool)
            .await?;
        sqlx::query(r#"ALTER TABLE servers ADD COLUMN IF NOT EXISTS min_rating INTEGER NOT NULL DEFAULT 0"#)
            .execute(&self.pool)
            .await?;
        sqlx::query(r#"ALTER TABLE servers ADD COLUMN IF NOT EXISTS min_steam_level INTEGER NOT NULL DEFAULT 0"#)
            .execute(&self.pool)
            .await?;
        sqlx::query(r#"ALTER TABLE servers ADD COLUMN IF NOT EXISTS whitelist_mode_enabled BOOLEAN NOT NULL DEFAULT false"#)
            .execute(&self.pool)
            .await?;
        sqlx::query(r#"ALTER TABLE servers ADD COLUMN IF NOT EXISTS max_players INTEGER NOT NULL DEFAULT 0"#)
            .execute(&self.pool)
            .await?;
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
            .execute(&self.pool)
            .await?;
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
            .execute(&self.pool)
            .await?;
        sqlx::query(
            r#"ALTER TABLE server_online_players ADD COLUMN IF NOT EXISTS current_map TEXT NOT NULL DEFAULT ''"#,
        )
        .execute(&self.pool)
        .await?;
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
        sqlx::query(r#"ALTER TABLE player_api_webhooks ADD COLUMN IF NOT EXISTS public_path TEXT NOT NULL DEFAULT ''"#)
            .execute(&self.pool)
            .await?;
        sqlx::query(r#"ALTER TABLE player_api_config ADD COLUMN IF NOT EXISTS max_api_count INTEGER NOT NULL DEFAULT 3"#)
            .execute(&self.pool)
            .await?;
        sqlx::query(r#"ALTER TABLE player_api_config ADD COLUMN IF NOT EXISTS interval_seconds INTEGER NOT NULL DEFAULT 30"#)
            .execute(&self.pool)
            .await?;
        sqlx::query(r#"ALTER TABLE player_api_config ADD COLUMN IF NOT EXISTS updated_at TIMESTAMPTZ NOT NULL DEFAULT now()"#)
            .execute(&self.pool)
            .await?;
        sqlx::query(r#"ALTER TABLE player_api_webhooks ADD COLUMN IF NOT EXISTS secret TEXT"#)
            .execute(&self.pool)
            .await?;
        sqlx::query(r#"ALTER TABLE player_api_webhooks ADD COLUMN IF NOT EXISTS server_ids UUID[] NOT NULL DEFAULT ARRAY[]::UUID[]"#)
            .execute(&self.pool)
            .await?;
        sqlx::query(r#"ALTER TABLE player_api_webhooks ADD COLUMN IF NOT EXISTS last_status TEXT"#)
            .execute(&self.pool)
            .await?;
        sqlx::query(r#"ALTER TABLE player_api_webhooks ADD COLUMN IF NOT EXISTS last_error TEXT"#)
            .execute(&self.pool)
            .await?;
        sqlx::query(r#"ALTER TABLE player_api_webhooks ADD COLUMN IF NOT EXISTS last_dispatched_at TIMESTAMPTZ"#)
            .execute(&self.pool)
            .await?;
        sqlx::query(
            r#"INSERT INTO player_api_config (id, max_api_count, interval_seconds)
               VALUES (true, 3, 30)
               ON CONFLICT (id) DO NOTHING"#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(r#"ALTER TABLE servers ALTER COLUMN status SET DEFAULT 'untested'"#)
            .execute(&self.pool)
            .await?;

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
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(r#"ALTER TABLE whitelist_requests ADD COLUMN IF NOT EXISTS nickname TEXT"#)
            .execute(&self.pool)
            .await?;
        sqlx::query(r#"ALTER TABLE whitelist_requests ADD COLUMN IF NOT EXISTS steamid64 TEXT"#)
            .execute(&self.pool)
            .await?;
        sqlx::query(r#"ALTER TABLE whitelist_requests ADD COLUMN IF NOT EXISTS steamid TEXT"#)
            .execute(&self.pool)
            .await?;
        sqlx::query(r#"ALTER TABLE whitelist_requests ADD COLUMN IF NOT EXISTS steamid3 TEXT"#)
            .execute(&self.pool)
            .await?;
        sqlx::query(r#"ALTER TABLE whitelist_requests ADD COLUMN IF NOT EXISTS profile_url TEXT"#)
            .execute(&self.pool)
            .await?;
        sqlx::query(r#"ALTER TABLE whitelist_requests ADD COLUMN IF NOT EXISTS steam_persona_name TEXT"#)
            .execute(&self.pool)
            .await?;
        sqlx::query(r#"ALTER TABLE whitelist_requests ADD COLUMN IF NOT EXISTS applied_at TIMESTAMPTZ"#)
            .execute(&self.pool)
            .await?;
        sqlx::query(r#"ALTER TABLE whitelist_requests ADD COLUMN IF NOT EXISTS approved_at TIMESTAMPTZ"#)
            .execute(&self.pool)
            .await?;
        sqlx::query(r#"ALTER TABLE whitelist_requests ADD COLUMN IF NOT EXISTS approved_by TEXT"#)
            .execute(&self.pool)
            .await?;
        sqlx::query(r#"ALTER TABLE whitelist_requests ADD COLUMN IF NOT EXISTS rejected_at TIMESTAMPTZ"#)
            .execute(&self.pool)
            .await?;
        sqlx::query(r#"ALTER TABLE whitelist_requests ADD COLUMN IF NOT EXISTS rejected_by TEXT"#)
            .execute(&self.pool)
            .await?;
        sqlx::query(r#"ALTER TABLE whitelist_requests ADD COLUMN IF NOT EXISTS rejection_reason TEXT"#)
            .execute(&self.pool)
            .await?;
        sqlx::query(r#"ALTER TABLE whitelist_requests ADD COLUMN IF NOT EXISTS revoked_at TIMESTAMPTZ"#)
            .execute(&self.pool)
            .await?;
        sqlx::query(r#"ALTER TABLE whitelist_requests ADD COLUMN IF NOT EXISTS revoked_by TEXT"#)
            .execute(&self.pool)
            .await?;
        sqlx::query(r#"ALTER TABLE whitelist_requests ADD COLUMN IF NOT EXISTS source TEXT"#)
            .execute(&self.pool)
            .await?;
        sqlx::query(r#"ALTER TABLE whitelist_requests ADD COLUMN IF NOT EXISTS updated_at TIMESTAMPTZ"#)
            .execute(&self.pool)
            .await?;

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
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(r#"UPDATE whitelist_requests SET steamid64 = steam_id WHERE steamid64 IS NULL AND steam_id ~ '^[0-9]{17}$'"#)
            .execute(&self.pool)
            .await?;
        sqlx::query(r#"UPDATE whitelist_requests SET steamid = steam_id WHERE steamid IS NULL AND steam_id IS NOT NULL"#)
            .execute(&self.pool)
            .await?;
        sqlx::query(r#"UPDATE whitelist_requests SET nickname = COALESCE(steamid64, steam_id, '未知玩家') WHERE nickname IS NULL OR btrim(nickname) = ''"#)
            .execute(&self.pool)
            .await?;
        sqlx::query(r#"UPDATE whitelist_requests SET source = 'public' WHERE source IS NULL OR btrim(source) = ''"#)
            .execute(&self.pool)
            .await?;
        sqlx::query(r#"UPDATE whitelist_requests SET updated_at = COALESCE(approved_at, rejected_at, revoked_at, applied_at) WHERE updated_at IS NULL"#)
            .execute(&self.pool)
            .await?;
        sqlx::query(r#"UPDATE whitelist_requests SET applied_at = COALESCE(updated_at, now()) WHERE applied_at IS NULL"#)
            .execute(&self.pool)
            .await?;

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
            .execute(&self.pool)
            .await?;

        // 玩家进服权限规则表
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

        // 审计日志表（全量审计）
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
        sqlx::query(r#"CREATE INDEX IF NOT EXISTS idx_audit_logs_created_at ON audit_logs (created_at DESC)"#)
            .execute(&self.pool)
            .await?;
        sqlx::query(r#"CREATE INDEX IF NOT EXISTS idx_audit_logs_server_id ON audit_logs (server_id)"#)
            .execute(&self.pool)
            .await?;
        sqlx::query(r#"CREATE INDEX IF NOT EXISTS idx_audit_logs_operation ON audit_logs (operation)"#)
            .execute(&self.pool)
            .await?;

        // 离线操作同步表（接收插件上传的离线操作）
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
        sqlx::query(r#"CREATE INDEX IF NOT EXISTS idx_offline_operations_server_id ON offline_operations (server_id)"#)
            .execute(&self.pool)
            .await?;
        sqlx::query(r#"CREATE INDEX IF NOT EXISTS idx_offline_operations_created_at ON offline_operations (created_at DESC)"#)
            .execute(&self.pool)
            .await?;

        // ========== 性能优化索引 ==========
        // 封禁记录查询优化 - 用于快速查找活跃封禁
        sqlx::query(r#"CREATE INDEX IF NOT EXISTS idx_ban_records_status_expires ON ban_records (status, expires_at) WHERE status = 'active'"#)
            .execute(&self.pool)
            .await?;
        sqlx::query(r#"CREATE INDEX IF NOT EXISTS idx_ban_records_steam_id ON ban_records (steam_id)"#)
            .execute(&self.pool)
            .await?;
        sqlx::query(r#"CREATE INDEX IF NOT EXISTS idx_ban_records_ip_address ON ban_records (ip_address) WHERE ip_address IS NOT NULL"#)
            .execute(&self.pool)
            .await?;
        // 封禁记录创建时间索引（用于列表排序）
        sqlx::query(r#"CREATE INDEX IF NOT EXISTS idx_ban_records_created_at ON ban_records (created_at DESC)"#)
            .execute(&self.pool)
            .await?;

        // 白名单查询优化
        sqlx::query(r#"CREATE INDEX IF NOT EXISTS idx_whitelist_requests_status ON whitelist_requests (status)"#)
            .execute(&self.pool)
            .await?;
        sqlx::query(r#"CREATE INDEX IF NOT EXISTS idx_whitelist_requests_steamid64 ON whitelist_requests (steamid64)"#)
            .execute(&self.pool)
            .await?;
        sqlx::query(r#"CREATE INDEX IF NOT EXISTS idx_whitelist_requests_status_steamid64 ON whitelist_requests (status, steamid64)"#)
            .execute(&self.pool)
            .await?;

        // 服务器查询优化 - 用于插件认证
        sqlx::query(r#"CREATE INDEX IF NOT EXISTS idx_servers_port ON servers (port)"#)
            .execute(&self.pool)
            .await?;
        sqlx::query(r#"CREATE INDEX IF NOT EXISTS idx_servers_token_port ON servers (report_token, port)"#)
            .execute(&self.pool)
            .await?;

        // 用户查询优化 - 用于解封权限检查
        sqlx::query(r#"CREATE INDEX IF NOT EXISTS idx_users_steamid64 ON users (steam_id) WHERE steam_id IS NOT NULL"#)
            .execute(&self.pool)
            .await?;

        // Session 清理优化 - logout_all_for_user 按 user_id 查询
        sqlx::query(r#"CREATE INDEX IF NOT EXISTS idx_sessions_user_id ON sessions (user_id)"#)
            .execute(&self.pool)
            .await?;
        // Session 过期清理优化
        sqlx::query(r#"CREATE INDEX IF NOT EXISTS idx_sessions_expires_at ON sessions (expires_at)"#)
            .execute(&self.pool)
            .await?;

        // 管理日志查询优化
        sqlx::query(r#"CREATE INDEX IF NOT EXISTS idx_admin_logs_operator_name ON admin_logs (operator_name)"#)
            .execute(&self.pool)
            .await?;
        sqlx::query(r#"CREATE INDEX IF NOT EXISTS idx_admin_logs_created_at ON admin_logs (created_at DESC)"#)
            .execute(&self.pool)
            .await?;

        // 封禁记录操作者查询优化
        sqlx::query(r#"CREATE INDEX IF NOT EXISTS idx_ban_records_operator_name ON ban_records (operator_name)"#)
            .execute(&self.pool)
            .await?;

        // 社区级访问限制
        sqlx::query(r#"ALTER TABLE communities ADD COLUMN IF NOT EXISTS min_rating INTEGER NOT NULL DEFAULT 0"#)
            .execute(&self.pool)
            .await?;
        sqlx::query(r#"ALTER TABLE communities ADD COLUMN IF NOT EXISTS min_steam_level INTEGER NOT NULL DEFAULT 0"#)
            .execute(&self.pool)
            .await?;
        sqlx::query(r#"ALTER TABLE communities ADD COLUMN IF NOT EXISTS whitelist_mode_enabled BOOLEAN NOT NULL DEFAULT false"#)
            .execute(&self.pool)
            .await?;
        sqlx::query(r#"ALTER TABLE servers ADD COLUMN IF NOT EXISTS use_custom_access BOOLEAN NOT NULL DEFAULT false"#)
            .execute(&self.pool)
            .await?;
        // 已有自定义设置的服务器保留原行为
        sqlx::query(r#"UPDATE servers SET use_custom_access = true WHERE access_restriction_enabled = true OR min_rating > 0 OR min_steam_level > 0"#)
            .execute(&self.pool)
            .await?;

        // ========== 外部服务器（RCON 查询） ==========
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
            .execute(&self.pool)
            .await?;

        sqlx::query(r#"ALTER TABLE external_servers ALTER COLUMN rcon_password DROP NOT NULL"#)
            .execute(&self.pool)
            .await?;

        sqlx::query(r#"ALTER TABLE external_servers ADD COLUMN IF NOT EXISTS poll_interval INTEGER NOT NULL DEFAULT 30"#)
            .execute(&self.pool)
            .await?;

        sqlx::query(r#"ALTER TABLE player_api_webhooks ADD COLUMN IF NOT EXISTS external_server_ids UUID[] NOT NULL DEFAULT ARRAY[]::UUID[]"#)
            .execute(&self.pool)
            .await?;

        sqlx::query(r#"ALTER TABLE player_api_webhooks ADD COLUMN IF NOT EXISTS enabled BOOLEAN NOT NULL DEFAULT true"#)
            .execute(&self.pool)
            .await?;

        sqlx::query(r#"ALTER TABLE player_api_webhooks ADD COLUMN IF NOT EXISTS public_access BOOLEAN NOT NULL DEFAULT true"#)
            .execute(&self.pool)
            .await?;

        sqlx::query(r#"ALTER TABLE player_api_webhooks ALTER COLUMN webhook_url DROP NOT NULL"#)
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
            tracing::info!(count = rows.len(), "migrated plaintext passwords to argon2 hashes");
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::Database;
    use crate::{config::Config, services::{dashboard_service, public_service, whitelist_service}};
    use sqlx::postgres::PgPoolOptions;
    use uuid::Uuid;

    fn schema_url(base_url: &str, schema: &str) -> String {
        let separator = if base_url.contains('?') { '&' } else { '?' };
        format!("{base_url}{separator}options=-csearch_path%3D{schema}")
    }

    async fn create_schema(base_url: &str, schema: &str) {
        let pool = PgPoolOptions::new()
            .max_connections(1)
            .connect(base_url)
            .await
            .unwrap();
        sqlx::query(&format!(r#"CREATE SCHEMA "{schema}""#))
            .execute(&pool)
            .await
            .unwrap();
        pool.close().await;
    }

    async fn drop_schema(base_url: &str, schema: &str) {
        let pool = PgPoolOptions::new()
            .max_connections(1)
            .connect(base_url)
            .await
            .unwrap();
        sqlx::query(&format!(r#"DROP SCHEMA IF EXISTS "{schema}" CASCADE"#))
            .execute(&pool)
            .await
            .unwrap();
        pool.close().await;
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
                r#"INSERT INTO users (id, username, display_name, password_hash, role)
                   VALUES ($1, 'admin_preview_admin', 'Admin One', 'pw', 'admin'),
                          ($2, 'admin_preview_dev', 'Dev One', 'pw', 'developer'),
                          ($3, 'admin_preview_normal', 'Normal One', 'pw', 'normal'),
                          ($4, 'admin_preview_guest', 'Guest One', 'pw', 'guest')"#,
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
            assert_eq!(preview_names, vec!["Admin One", "Dev One", "Normal One"]);
            assert!(metrics.admin_preview.iter().all(|item| item.status == "可用"));

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
            let names = webhook_columns.into_iter().map(|row| row.0).collect::<Vec<_>>();
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

            let items = whitelist_service::list_whitelist(&db).await?;
            assert_eq!(items.len(), 2);
            assert!(items.iter().any(|item| {
                item.nickname == "旧版被拒玩家"
                    && item.steamid64 == "76561198000000001"
                    && item.profile_url.as_deref() == Some("https://steamcommunity.com/profiles/76561198000000001")
                    && item.rejection_reason.as_deref() == Some("资料不完整")
                    && item.rejected_by.as_deref() == Some("Alex")
                    && item.rejected_at.is_some()
            }));
            assert!(items.iter().any(|item| {
                item.nickname == "旧版已通过玩家"
                    && item.steamid64 == "76561198000000002"
                    && item.approved_by.as_deref() == Some("DevAdmin")
                    && item.approved_at.is_some()
            }));

            let public_items = public_service::list_public_whitelist(&db, &crate::routes::ListQuery { search: None, status: None, page: None, page_size: None }).await?;
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
            let server_names = server_columns.into_iter().map(|row| row.0).collect::<Vec<_>>();
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

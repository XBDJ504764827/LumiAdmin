use super::Database;

impl Database {
    pub(super) async fn migrate_core_tables(&self) -> anyhow::Result<()> {
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
          cs_prime_enabled BOOLEAN NOT NULL DEFAULT false,
          created_at TIMESTAMPTZ NOT NULL DEFAULT now()
        )"#,
            r#"CREATE TABLE IF NOT EXISTS player_access_cache (
          steamid64 TEXT NOT NULL,
          rating INTEGER NOT NULL DEFAULT 0,
          steam_level INTEGER NOT NULL DEFAULT 0,
          rating_source TEXT NOT NULL DEFAULT 'legacy',
          kzt_data JSONB,
          skz_data JSONB,
          vnl_data JSONB,
          ovr_data JSONB,
          expires_at TIMESTAMPTZ NOT NULL,
          updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
          PRIMARY KEY (steamid64, rating_source)
        )"#,
            r#"CREATE TABLE IF NOT EXISTS whitelist_requests (
          id UUID PRIMARY KEY,
          steam_id TEXT NOT NULL,
          nickname TEXT NOT NULL,
          contact TEXT,
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
}

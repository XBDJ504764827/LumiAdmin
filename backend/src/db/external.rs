use super::Database;

impl Database {
pub(super) async fn migrate_external_servers_schema(&self) -> anyhow::Result<()> {
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

pub(super) async fn migrate_external_ban_api_schema(&self) -> anyhow::Result<()> {
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

pub(super) async fn migrate_ban_api_keys_schema(&self) -> anyhow::Result<()> {
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

}

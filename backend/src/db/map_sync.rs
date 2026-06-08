use super::Database;

impl Database {
pub(super) async fn migrate_map_tiers_table(&self) -> anyhow::Result<()> {
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

pub(super) async fn migrate_map_sync_schema(&self) -> anyhow::Result<()> {
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
}

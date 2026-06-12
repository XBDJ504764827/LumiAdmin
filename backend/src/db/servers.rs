use super::Database;

impl Database {
pub(super) async fn migrate_servers_schema(&self) -> anyhow::Result<()> {
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

    // player_access_cache 表 + 索引（扩展为通用 GOKZ 缓存）
    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS player_access_cache (
          steamid64 TEXT PRIMARY KEY,
          rating INTEGER NOT NULL DEFAULT 0,
          steam_level INTEGER NOT NULL DEFAULT 0,
          kzt_data JSONB,
          skz_data JSONB,
          vnl_data JSONB,
          ovr_data JSONB,
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

pub(super) async fn migrate_server_data(&self) -> anyhow::Result<()> {
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

// 白名单表字段扩充 + 旧数据迁移
}

impl Database {
/// 扩展 player_access_cache 表以支持 GOKZ 详细数据缓存
pub(super) async fn migrate_player_access_cache_extended(&self) -> anyhow::Result<()> {
    // 添加 JSONB 扩展列（如果不存在）
    let columns = [
        ("kzt_data", "JSONB"),
        ("skz_data", "JSONB"),
        ("vnl_data", "JSONB"),
        ("ovr_data", "JSONB"),
    ];
    for (col, ty) in columns {
        sqlx::query(&format!(
            r#"ALTER TABLE player_access_cache ADD COLUMN IF NOT EXISTS {} {}"#,
            col, ty
        ))
        .execute(&self.pool)
        .await?;
    }
    // rating 和 steam_level 设置默认值（兼容旧数据）
    sqlx::query(r#"ALTER TABLE player_access_cache ALTER COLUMN rating SET DEFAULT 0"#)
        .execute(&self.pool)
        .await?;
    sqlx::query(r#"ALTER TABLE player_access_cache ALTER COLUMN steam_level SET DEFAULT 0"#)
        .execute(&self.pool)
        .await?;

    // player_access_logs 进服记录表
    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS player_access_logs (
          id UUID PRIMARY KEY,
          steam_id64 TEXT NOT NULL,
          player_name TEXT,
          ip_address TEXT,
          server_id UUID NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
          server_name TEXT NOT NULL,
          server_port INTEGER NOT NULL,
          community_id UUID NOT NULL REFERENCES communities(id) ON DELETE CASCADE,
          community_name TEXT,
          access_method TEXT NOT NULL,
          rating INTEGER,
          steam_level INTEGER,
          created_at TIMESTAMPTZ NOT NULL DEFAULT now()
        )"#,
    )
    .execute(&self.pool)
    .await?;
    sqlx::query(r#"CREATE INDEX IF NOT EXISTS idx_player_access_logs_steamid64 ON player_access_logs (steam_id64)"#)
        .execute(&self.pool).await?;
    sqlx::query(r#"CREATE INDEX IF NOT EXISTS idx_player_access_logs_server_id ON player_access_logs (server_id)"#)
        .execute(&self.pool).await?;
    sqlx::query(r#"CREATE INDEX IF NOT EXISTS idx_player_access_logs_community_id ON player_access_logs (community_id)"#)
        .execute(&self.pool).await?;
    sqlx::query(r#"CREATE INDEX IF NOT EXISTS idx_player_access_logs_access_method ON player_access_logs (access_method)"#)
        .execute(&self.pool).await?;
    sqlx::query(r#"CREATE INDEX IF NOT EXISTS idx_player_access_logs_created_at ON player_access_logs (created_at DESC)"#)
        .execute(&self.pool).await?;

    Ok(())
}
}

use super::Database;

impl Database {
pub(super) async fn migrate_logs_operations_and_indexes(&self) -> anyhow::Result<()> {
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
          client_ip TEXT,
          details JSONB,
          idempotency_key TEXT UNIQUE,
          created_at TIMESTAMPTZ DEFAULT now()
        )"#,
    )
    .execute(&self.pool)
    .await?;
    sqlx::query(r#"ALTER TABLE audit_logs ADD COLUMN IF NOT EXISTS client_ip TEXT"#)
        .execute(&self.pool)
        .await?;
    sqlx::query(r#"ALTER TABLE audit_logs ADD COLUMN IF NOT EXISTS details JSONB"#)
        .execute(&self.pool)
        .await?;

    let audit_indexes = [
        r#"CREATE INDEX IF NOT EXISTS idx_audit_logs_created_at ON audit_logs (created_at DESC)"#,
        r#"CREATE INDEX IF NOT EXISTS idx_audit_logs_server_id ON audit_logs (server_id)"#,
        r#"CREATE INDEX IF NOT EXISTS idx_audit_logs_operation ON audit_logs (operation)"#,
        r#"CREATE INDEX IF NOT EXISTS idx_audit_logs_client_ip ON audit_logs (client_ip) WHERE client_ip IS NOT NULL"#,
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

}

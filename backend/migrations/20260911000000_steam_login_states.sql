-- Steam OpenID 登录 state 持久化：防登录 CSRF 的一次性 state。
-- 此前 state 存进程内存，重启即失效且多副本部署不可用；迁移到数据库后
-- 重启无影响，消费操作使用 DELETE ... RETURNING 保证原子性（用后即焚）。

CREATE TABLE IF NOT EXISTS steam_login_states (
  state TEXT PRIMARY KEY,
  frontend_base TEXT NOT NULL,
  expires_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_steam_login_states_expires
  ON steam_login_states (expires_at);
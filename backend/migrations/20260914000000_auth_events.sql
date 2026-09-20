-- ============================================================================
-- 授权事件同步（LumiAuth）：Control Plane -> Data Plane
--
-- 设计：
-- - auth_events：全局递增 version（SEQUENCE），server_id NULL = 广播（白名单/封禁），
--   非 NULL = 单服事件（server.config.update）。
-- - 业务写入与事件插入在同一事务内（Transactional Outbox），避免“DB 成功但事件丢失”。
-- - auth_server_state：记录每服 last_acked_version / 连接状态 / 上次同步时间，供后台 UI 展示。
-- ============================================================================

CREATE SEQUENCE IF NOT EXISTS auth_event_seq;

CREATE TABLE IF NOT EXISTS auth_events (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  event_id UUID NOT NULL UNIQUE DEFAULT gen_random_uuid(),
  version BIGINT NOT NULL UNIQUE DEFAULT nextval('auth_event_seq'),
  server_id UUID REFERENCES servers(id) ON DELETE CASCADE,
  event_type TEXT NOT NULL,
  payload JSONB NOT NULL DEFAULT '{}'::JSONB,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_auth_events_version ON auth_events (version);
CREATE INDEX IF NOT EXISTS idx_auth_events_server_version ON auth_events (server_id, version);
CREATE INDEX IF NOT EXISTS idx_auth_events_type_created ON auth_events (event_type, created_at DESC);

CREATE TABLE IF NOT EXISTS auth_server_state (
  server_id UUID PRIMARY KEY REFERENCES servers(id) ON DELETE CASCADE,
  last_acked_version BIGINT NOT NULL DEFAULT 0,
  connection_status TEXT NOT NULL DEFAULT 'disconnected',
  last_seen_at TIMESTAMPTZ,
  last_snapshot_at TIMESTAMPTZ,
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

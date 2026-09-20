-- 白名单低风险自动通过功能
--
-- 1. 设置表（单行）：控制自动通过开关与等待时长（小时）
-- 2. 待审核扫描索引：加速「pending 且 applied_at 超时」查询

CREATE TABLE IF NOT EXISTS whitelist_auto_approve_config (
  id BOOLEAN PRIMARY KEY DEFAULT true,
  enabled BOOLEAN NOT NULL DEFAULT true,
  hours INTEGER NOT NULL DEFAULT 3,
  updated_by TEXT,
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  CONSTRAINT whitelist_auto_approve_config_single_row CHECK (id)
);

INSERT INTO whitelist_auto_approve_config (id, enabled, hours)
VALUES (true, true, 3)
ON CONFLICT (id) DO NOTHING;

CREATE INDEX IF NOT EXISTS idx_whitelist_requests_pending_applied_at
  ON whitelist_requests (applied_at)
  WHERE status = 'pending';

-- 稳定保存审计操作人，并为异步外部副作用提供持久化队列。

ALTER TABLE audit_logs
  ADD COLUMN IF NOT EXISTS operator_id UUID;

DO $$
BEGIN
  IF NOT EXISTS (
    SELECT 1 FROM pg_constraint WHERE conname = 'fk_audit_logs_operator_id'
  ) THEN
    ALTER TABLE audit_logs
      ADD CONSTRAINT fk_audit_logs_operator_id
      FOREIGN KEY (operator_id) REFERENCES users(id) ON DELETE SET NULL;
  END IF;
END $$;

CREATE INDEX IF NOT EXISTS idx_audit_logs_operator_id_created
  ON audit_logs (operator_id, created_at DESC);

-- 将现有能够通过用户名/显示名/备注唯一匹配的历史记录补齐 operator_id。
UPDATE audit_logs al
SET operator_id = (
  SELECT u.id
  FROM users u
  WHERE u.username = al.operator_name
     OR u.display_name = al.operator_name
     OR NULLIF(u.remark, '') = al.operator_name
  ORDER BY CASE
    WHEN u.username = al.operator_name THEN 0
    WHEN u.display_name = al.operator_name THEN 1
    ELSE 2
  END, u.created_at ASC
  LIMIT 1
)
WHERE al.operator_id IS NULL;

CREATE TABLE IF NOT EXISTS external_sync_outbox (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  operation TEXT NOT NULL,
  ban_id UUID,
  target_id UUID,
  external_uuid TEXT,
  payload JSONB NOT NULL DEFAULT '{}'::JSONB,
  status TEXT NOT NULL DEFAULT 'pending',
  attempts INTEGER NOT NULL DEFAULT 0,
  next_attempt_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  locked_at TIMESTAMPTZ,
  locked_by TEXT,
  last_error TEXT,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  CONSTRAINT fk_external_sync_outbox_ban
    FOREIGN KEY (ban_id) REFERENCES ban_records(id) ON DELETE SET NULL,
  CONSTRAINT fk_external_sync_outbox_target
    FOREIGN KEY (target_id) REFERENCES external_ban_api_targets(id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_external_sync_outbox_pending
  ON external_sync_outbox (next_attempt_at, created_at)
  WHERE status = 'pending';
CREATE INDEX IF NOT EXISTS idx_external_sync_outbox_status
  ON external_sync_outbox (status, updated_at DESC);
DELETE FROM external_sync_outbox older
USING external_sync_outbox newer
WHERE older.status IN ('pending', 'processing')
  AND newer.status IN ('pending', 'processing')
  AND older.operation = newer.operation
  AND older.ban_id IS NOT DISTINCT FROM newer.ban_id
  AND older.target_id IS NOT DISTINCT FROM newer.target_id
  AND older.id < newer.id;
CREATE UNIQUE INDEX IF NOT EXISTS uq_external_sync_outbox_active
  ON external_sync_outbox (operation, ban_id, target_id)
  WHERE status IN ('pending', 'processing');

ALTER TABLE lumi_bot_event_queue
  ADD COLUMN IF NOT EXISTS next_attempt_at TIMESTAMPTZ NOT NULL DEFAULT now();
ALTER TABLE lumi_bot_event_queue
  ADD COLUMN IF NOT EXISTS locked_at TIMESTAMPTZ;
ALTER TABLE lumi_bot_event_queue
  ADD COLUMN IF NOT EXISTS locked_by TEXT;

CREATE INDEX IF NOT EXISTS idx_lumi_bot_event_queue_claimable
  ON lumi_bot_event_queue (next_attempt_at, queued_at)
  WHERE status = 'pending';

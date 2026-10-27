-- 系统自动通过白名单二次审核队列
--
-- 低风险自动通过不再直接获得永久白名单，而是先获得 3 天临时白名单，
-- 并进入二次审核队列：
--   auto_review_required = true  → 仍在队列中，等待管理员二次审核
--   管理员二次审核通过           → 转为永久白名单并清空标记
--   3 天内无人处理               → 到期任务置为 expired，玩家需重新申请
--
-- auto_approved_at 记录系统自动通过时间（历史记录通过 approved_via='auto' 回填），
-- auto_reviewed_at / auto_reviewed_by 记录二次审核处理人与时间，便于追溯。

ALTER TABLE whitelist_requests
  ADD COLUMN IF NOT EXISTS auto_review_required BOOLEAN NOT NULL DEFAULT false;
ALTER TABLE whitelist_requests
  ADD COLUMN IF NOT EXISTS auto_approved_at TIMESTAMPTZ;
ALTER TABLE whitelist_requests
  ADD COLUMN IF NOT EXISTS auto_reviewed_at TIMESTAMPTZ;
ALTER TABLE whitelist_requests
  ADD COLUMN IF NOT EXISTS auto_reviewed_by TEXT;

-- 回填历史自动通过记录：保留自动通过时间，仍在有效期内的进入二次审核队列。
UPDATE whitelist_requests
SET auto_approved_at = COALESCE(approved_at, updated_at, now())
WHERE approved_via = 'auto'
  AND auto_approved_at IS NULL;

UPDATE whitelist_requests
SET auto_review_required = true
WHERE approved_via = 'auto'
  AND status = 'approved'
  AND auto_review_required = false;

-- 历史自动通过记录原先按 7 天授予；统一收敛到新策略：自动通过时间 + 3 天。
-- 已超过 3 天的记录会由到期任务在下一轮置为 expired，玩家需重新申请。
UPDATE whitelist_requests
SET expires_at = LEAST(
      expires_at,
      COALESCE(auto_approved_at, approved_at, updated_at, now()) + interval '3 days'
    ),
    duration_days = 3
WHERE status = 'approved'
  AND auto_review_required = true
  AND expires_at IS NOT NULL
  AND expires_at > COALESCE(auto_approved_at, approved_at, updated_at, now()) + interval '3 days';

-- 队列查询：status='approved' AND auto_review_required=true，按最早到期优先。
CREATE INDEX IF NOT EXISTS idx_whitelist_requests_auto_review_queue
  ON whitelist_requests (expires_at ASC, approved_at DESC)
  WHERE auto_review_required = true AND status = 'approved';

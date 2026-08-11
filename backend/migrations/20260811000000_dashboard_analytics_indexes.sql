-- 仪表盘统计查询索引：
-- 白名单增长趋势 / 今日新增白名单按 approved_at 范围聚合，
-- 部分索引仅覆盖已审核记录，减少索引体积。
CREATE INDEX IF NOT EXISTS idx_whitelist_requests_approved_at
  ON whitelist_requests (approved_at)
  WHERE approved_at IS NOT NULL;

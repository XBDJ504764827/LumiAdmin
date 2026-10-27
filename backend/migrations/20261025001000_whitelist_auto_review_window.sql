-- 统一系统自动通过记录的二次审核窗口。
--
-- 历史自动通过记录存在 expires_at 为空（旧逻辑视为永久）、已被旧 7 天策略
-- 覆盖，或按 approved_at + 3 天计算已经过期的情况。为避免部署瞬间直接剥夺
-- 所有历史自动通过玩家的白名单，对部署时仍在队列中的记录统一给予 3 天处理
-- 窗口；此后新产生的自动通过记录仍按「自动通过时间 + 3 天」计算。

UPDATE whitelist_requests
SET expires_at = now() + interval '3 days',
    duration_days = 3,
    updated_at = now()
WHERE status = 'approved'
  AND auto_review_required = true;

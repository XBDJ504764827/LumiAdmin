-- 白名单 QQ 绑定健壮性加固
--
-- 1. 验证码「单个 Steam 仅一条活跃码」改为数据库唯一约束（此前仅为普通索引，
--    并发签发可能留下多条活跃码，放大重复绑定/换绑绕过风险）。
-- 2. 私聊化迁移清空旧群绑定后，同步清理 whitelist_requests 中残留的 QQ 快照，
--    避免管理员据过期 openid 私聊到错误对象。

-- 清理已存在的重复活跃码：仅保留每个 steamid64 最近一条，其余标记已消费
UPDATE whitelist_qq_verify_codes AS c
SET consumed_at = now()
WHERE c.consumed_at IS NULL
  AND c.expires_at > now()
  AND EXISTS (
    SELECT 1 FROM whitelist_qq_verify_codes AS newer
    WHERE newer.steamid64 = c.steamid64
      AND newer.consumed_at IS NULL
      AND newer.expires_at > now()
      AND (newer.created_at, newer.id) > (c.created_at, c.id)
  );

-- 单个 Steam 仅允许一条未消费且未过期的活跃码
CREATE UNIQUE INDEX IF NOT EXISTS idx_whitelist_qq_verify_codes_one_active
  ON whitelist_qq_verify_codes (steamid64)
  WHERE consumed_at IS NULL;

-- 清理旧群绑定迁移遗留的申请快照（openid 为群场景，无法用于私聊）
UPDATE whitelist_requests
SET qq_openid = NULL,
    qq_group_id = NULL,
    qq_username = NULL,
    qq_verified_at = NULL
WHERE qq_openid IS NOT NULL
  AND qq_group_id IS NOT NULL;

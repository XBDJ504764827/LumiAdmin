-- 删除「CS 优先账户（Prime）」进服模式
--
-- 该功能依赖游戏插件通过 Steam GameServer API 查询 Prime 状态并上报，
-- 实际不可用，予以移除。进服模式保留：白名单模式、进入限制（Rating/Steam 等级）、
-- 中高风险账号拦截（risk_block_enabled）。
--
-- 注意：旧迁移 20260910000000 在 servers 上创建了引用 cs_prime_enabled 的缓存通知
-- 触发器，必须先重建触发器（去掉该列）再删列，否则 DROP COLUMN 因依赖失败。

DROP TRIGGER IF EXISTS lumiadmin_server_cache_notify ON servers;
CREATE TRIGGER lumiadmin_server_cache_notify
AFTER INSERT OR UPDATE OF
  access_restriction_enabled, min_rating, min_steam_level,
  whitelist_mode_enabled, use_custom_access
OR DELETE
ON servers
FOR EACH ROW EXECUTE FUNCTION lumiadmin_notify_cache_change();

ALTER TABLE servers DROP COLUMN IF EXISTS cs_prime_enabled;
ALTER TABLE communities DROP COLUMN IF EXISTS cs_prime_enabled;

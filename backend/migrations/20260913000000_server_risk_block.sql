-- ============================================================================
-- 服务器级「中高风险账号拦截」开关
--
-- 背景：玩家满足服务器最低进入要求后，即使没有白名单也能进入。现在对
--   存在封禁类风险信号（自身有效本地/全球封禁、同 IP 关联账号有效封禁）的
--   中高风险账号，要求必须持有白名单才能进入。
--   该开关按服务器配置，默认开启。
--
-- 本迁移：
--   1. 仅在列缺失时补列（运行时 migrate_servers_schema 已负责常规创建，
--      这里保证纯 SQL 部署路径同样可用）；
--   2. 收窄服务器缓存通知触发器时把新列纳入监听，管理员在后台切换开关后
--      立即刷新服务器配置缓存（而不是等待 TTL）。
-- ============================================================================

ALTER TABLE servers
  ADD COLUMN IF NOT EXISTS risk_block_enabled BOOLEAN NOT NULL DEFAULT true;

DROP TRIGGER IF EXISTS lumiadmin_server_cache_notify ON servers;
CREATE TRIGGER lumiadmin_server_cache_notify
AFTER INSERT OR UPDATE OF
  access_restriction_enabled, min_rating, min_steam_level,
  whitelist_mode_enabled, cs_prime_enabled, use_custom_access,
  risk_block_enabled
OR DELETE
ON servers
FOR EACH ROW EXECUTE FUNCTION lumiadmin_notify_cache_change();

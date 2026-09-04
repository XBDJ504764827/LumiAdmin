-- ============================================================================
-- 数据库性能热路径优化（Phase 2）
--
-- 背景：游戏服插件每 30s 上报一次在线玩家/服务器状态（心跳），
--   UPDATE servers 命中 lumiadmin_server_cache_notify 触发器后，后端监听器会
--   全量刷新 ban/whitelist/server 缓存与访问控制快照，造成高并发全表扫描。
--
-- 本迁移：
--   1. 收窄 servers 触发器：心跳列（status/players/last_reported_at/max_players）
--      不再触发缓存通知，仅访问控制相关列变化时才通知（避免"心跳风暴"）；
--   2. 为进服检查 / IP 关联热路径补充索引；
--   3. 为插件封禁轮询与会话表保留清理补充索引。
-- ============================================================================

-- 1) 收窄服务器缓存通知触发器：只对访问控制相关列变化发通知。
--    注意：生产环境已应用过的历史触发器由本迁移替换，而不是修改旧迁移文件。
DROP TRIGGER IF EXISTS lumiadmin_server_cache_notify ON servers;
CREATE TRIGGER lumiadmin_server_cache_notify
AFTER INSERT OR UPDATE OF
  access_restriction_enabled, min_rating, min_steam_level,
  whitelist_mode_enabled, cs_prime_enabled, use_custom_access
OR DELETE
ON servers
FOR EACH ROW EXECUTE FUNCTION lumiadmin_notify_cache_change();

-- 2) 进服检查 / IP 关联热路径索引（evaluate_ip_ban_for_access 三路 UNION 的过滤列）
CREATE INDEX IF NOT EXISTS idx_player_access_logs_ip
  ON player_access_logs (ip_address)
  WHERE ip_address IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_server_online_players_ip
  ON server_online_players (ip)
  WHERE ip <> 'unknown';

CREATE INDEX IF NOT EXISTS idx_player_server_sessions_ip
  ON player_server_sessions (ip)
  WHERE ip <> 'unknown';

-- 3a) 会话表保留清理支持索引（按 created_at 分批 DELETE）
CREATE INDEX IF NOT EXISTS idx_player_server_sessions_created_at
  ON player_server_sessions (created_at);

-- 3b) 插件封禁轮询：ORDER BY created_at DESC 直接命中部分索引
CREATE INDEX IF NOT EXISTS idx_ban_records_active_created
  ON ban_records (created_at DESC)
  WHERE status = 'active';

-- 3c) 活跃封禁缓存/快照刷新：按 steam_id + ip 精确匹配
CREATE INDEX IF NOT EXISTS idx_ban_records_active_steam_ip
  ON ban_records (steam_id, ip_address)
  WHERE status = 'active';

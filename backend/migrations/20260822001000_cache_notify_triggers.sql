-- 业务数据发生变化时通知各后端实例立即刷新内存缓存。
CREATE OR REPLACE FUNCTION lumiadmin_notify_cache_change() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  IF TG_TABLE_NAME IN ('ban_records', 'global_bans') THEN
    PERFORM pg_notify('lumiadmin_cache', 'ban');
  ELSIF TG_TABLE_NAME = 'whitelist_requests' THEN
    PERFORM pg_notify('lumiadmin_cache', 'whitelist');
  ELSIF TG_TABLE_NAME IN ('servers', 'communities') THEN
    PERFORM pg_notify('lumiadmin_cache', 'server');
  ELSIF TG_TABLE_NAME = 'player_access_cache' THEN
    PERFORM pg_notify('lumiadmin_cache', 'snapshot');
  END IF;
  IF TG_OP = 'DELETE' THEN
    RETURN OLD;
  END IF;
  RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS lumiadmin_ban_cache_notify ON ban_records;
CREATE TRIGGER lumiadmin_ban_cache_notify
AFTER INSERT OR UPDATE OR DELETE ON ban_records
FOR EACH ROW EXECUTE FUNCTION lumiadmin_notify_cache_change();

DROP TRIGGER IF EXISTS lumiadmin_global_ban_cache_notify ON global_bans;
CREATE TRIGGER lumiadmin_global_ban_cache_notify
AFTER INSERT OR UPDATE OR DELETE ON global_bans
FOR EACH ROW EXECUTE FUNCTION lumiadmin_notify_cache_change();

DROP TRIGGER IF EXISTS lumiadmin_whitelist_cache_notify ON whitelist_requests;
CREATE TRIGGER lumiadmin_whitelist_cache_notify
AFTER INSERT OR UPDATE OR DELETE ON whitelist_requests
FOR EACH ROW EXECUTE FUNCTION lumiadmin_notify_cache_change();

DROP TRIGGER IF EXISTS lumiadmin_server_cache_notify ON servers;
CREATE TRIGGER lumiadmin_server_cache_notify
AFTER INSERT OR UPDATE OR DELETE ON servers
FOR EACH ROW EXECUTE FUNCTION lumiadmin_notify_cache_change();

DROP TRIGGER IF EXISTS lumiadmin_community_cache_notify ON communities;
CREATE TRIGGER lumiadmin_community_cache_notify
AFTER INSERT OR UPDATE OR DELETE ON communities
FOR EACH ROW EXECUTE FUNCTION lumiadmin_notify_cache_change();

DROP TRIGGER IF EXISTS lumiadmin_access_snapshot_notify ON player_access_cache;
CREATE TRIGGER lumiadmin_access_snapshot_notify
AFTER INSERT OR UPDATE OR DELETE ON player_access_cache
FOR EACH ROW EXECUTE FUNCTION lumiadmin_notify_cache_change();

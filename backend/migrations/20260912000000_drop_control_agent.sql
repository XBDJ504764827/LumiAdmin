-- ============================================================================
-- 移除 LGSM Control Agent 子系统
--
-- 网站不再使用 Agent 安装脚本（install.sh / control agent），相关后端代码
-- （control_service / routes::control）已删除。本迁移清理其数据库遗留：
--   1. 删除 servers 表上仅服务于 Agent 的三列
--   2. 删除 control_* 四张表（依赖顺序：jobs → discoveries → agents → tokens）
--
-- 注意：servers.control_agent_id 原有 REFERENCES control_agents(id) 外键，
-- 必须先删除引用列，再删除被引用表。
-- ============================================================================

ALTER TABLE servers DROP COLUMN IF EXISTS control_agent_id;
ALTER TABLE servers DROP COLUMN IF EXISTS control_last_seen_at;
ALTER TABLE servers DROP COLUMN IF EXISTS lgsm_instance;

DROP TABLE IF EXISTS control_jobs;
DROP TABLE IF EXISTS control_discoveries;
DROP TABLE IF EXISTS control_agents;
DROP TABLE IF EXISTS control_install_tokens;
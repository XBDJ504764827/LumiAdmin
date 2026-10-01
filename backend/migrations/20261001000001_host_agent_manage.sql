-- 宿主机管理：自定义备注名、停用开关。
-- 删除宿主机直接 DELETE host_agents，servers.host_agent_id 由外键置空。

ALTER TABLE host_agents ADD COLUMN IF NOT EXISTS display_name TEXT NOT NULL DEFAULT '';
ALTER TABLE host_agents ADD COLUMN IF NOT EXISTS disabled BOOLEAN NOT NULL DEFAULT FALSE;
CREATE INDEX IF NOT EXISTS idx_host_agents_disabled ON host_agents (disabled);

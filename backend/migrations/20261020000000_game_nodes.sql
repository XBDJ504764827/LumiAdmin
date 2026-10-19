-- 游戏节点（Game Node）：同一台游戏机（共享游戏文件 / addons 目录）上的所有服务器
-- 归为同一个节点，节点内所有服务器进程共用同一份本地同步状态与本地 SQLite 快照。
--
-- 目的：
-- 1. 同步从「每服一次」降为「每节点一次」，避免 N 台服重复拉取快照；
-- 2. 白名单/封禁变更一次同步到位，不再出现「部分服同步完成、部分服未完成」；
-- 3. 授权同步落后按节点维度统计，语义准确。
--
-- 节点按服务器 IP 自动归组（同名 IP = 同一台机器），后续可人工调整。

CREATE TABLE IF NOT EXISTS game_nodes (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  -- 展示名，默认取机器 IP
  name TEXT NOT NULL,
  -- 节点级同步令牌（当前用节点内任一服的 report_token 鉴权即可，保留字段供后续使用）
  token TEXT NOT NULL UNIQUE DEFAULT gen_random_uuid(),
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE servers ADD COLUMN IF NOT EXISTS node_id UUID REFERENCES game_nodes(id) ON DELETE SET NULL;
CREATE INDEX IF NOT EXISTS idx_servers_node_id ON servers (node_id);

-- 按 IP 自动建节点并绑定服务器（幂等）。
INSERT INTO game_nodes (name)
SELECT DISTINCT btrim(s.ip) FROM servers s
WHERE s.ip IS NOT NULL AND btrim(s.ip) <> ''
  AND NOT EXISTS (SELECT 1 FROM game_nodes g WHERE g.name = btrim(s.ip));

UPDATE servers s
SET node_id = g.id
FROM game_nodes g
WHERE s.node_id IS NULL AND s.ip IS NOT NULL AND g.name = btrim(s.ip);

-- 节点级同步状态（一节点一行）。
CREATE TABLE IF NOT EXISTS auth_node_state (
  node_id UUID PRIMARY KEY REFERENCES game_nodes(id) ON DELETE CASCADE,
  last_acked_version BIGINT NOT NULL DEFAULT 0,
  connection_status TEXT NOT NULL DEFAULT 'disconnected',
  last_seen_at TIMESTAMPTZ,
  last_snapshot_at TIMESTAMPTZ,
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- 授权事件挂到节点维度：node_id 为空 = 全局广播（白名单/封禁），
-- 非空 = 仅该节点可见（服务器/社区配置变更）。
ALTER TABLE auth_events ADD COLUMN IF NOT EXISTS node_id UUID REFERENCES game_nodes(id) ON DELETE CASCADE;
CREATE INDEX IF NOT EXISTS idx_auth_events_node_version ON auth_events (node_id, version);

-- 历史事件回填：原来挂 server_id 的事件，补上对应节点，
-- 否则切到节点轮询后这些事件对该节点不可见，会造成版本跳空。
UPDATE auth_events e
SET node_id = s.node_id
FROM servers s
WHERE e.node_id IS NULL AND e.server_id = s.id AND s.node_id IS NOT NULL;

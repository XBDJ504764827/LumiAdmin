-- Agent 心跳采样：每次心跳记一行，供 Agent控制 页心跳趋势图聚合。
-- 10s 间隔下单 Agent 每天约 8640 行，保留 30 天由后台任务清理。

CREATE TABLE IF NOT EXISTS agent_heartbeat_samples (
    agent_id UUID NOT NULL REFERENCES host_agents(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_heartbeat_samples_agent_time
    ON agent_heartbeat_samples (agent_id, created_at DESC);

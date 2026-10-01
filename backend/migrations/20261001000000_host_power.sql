-- 主机 Agent 电源控制：宿主机、安装口令、电源任务 + servers 绑定列。
-- 与已删除的 control_* 系列无名称冲突，本次为新建表。

CREATE TABLE IF NOT EXISTS host_agents (
    id UUID PRIMARY KEY,
    hostname TEXT NOT NULL DEFAULT '',
    lgsm_dir TEXT NOT NULL DEFAULT '',
    instances JSONB NOT NULL DEFAULT '[]'::JSONB,
    token_hash TEXT NOT NULL UNIQUE,
    last_seen_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS host_install_tokens (
    id UUID PRIMARY KEY,
    token_hash TEXT NOT NULL UNIQUE,
    created_by UUID REFERENCES users(id) ON DELETE SET NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    used_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_host_install_tokens_expires ON host_install_tokens (expires_at);

CREATE TABLE IF NOT EXISTS power_jobs (
    id UUID PRIMARY KEY,
    server_id UUID NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
    host_agent_id UUID REFERENCES host_agents(id) ON DELETE SET NULL,
    action TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    requested_by UUID REFERENCES users(id) ON DELETE SET NULL,
    exit_code INTEGER,
    timed_out BOOLEAN NOT NULL DEFAULT FALSE,
    output TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    started_at TIMESTAMPTZ,
    finished_at TIMESTAMPTZ
);
CREATE INDEX IF NOT EXISTS idx_power_jobs_server ON power_jobs (server_id);
CREATE INDEX IF NOT EXISTS idx_power_jobs_status ON power_jobs (status);
CREATE INDEX IF NOT EXISTS idx_power_jobs_created ON power_jobs (created_at DESC);

ALTER TABLE servers ADD COLUMN IF NOT EXISTS host_agent_id UUID REFERENCES host_agents(id) ON DELETE SET NULL;
ALTER TABLE servers ADD COLUMN IF NOT EXISTS lgsm_instance TEXT;
CREATE INDEX IF NOT EXISTS idx_servers_host_agent ON servers (host_agent_id);

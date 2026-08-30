-- Server control (LGSM) agent: install tokens, agents, discoveries, jobs

CREATE TABLE IF NOT EXISTS control_install_tokens (
    id UUID PRIMARY KEY,
    community_id UUID NOT NULL REFERENCES communities(id) ON DELETE CASCADE,
    token TEXT NOT NULL UNIQUE,
    created_by UUID REFERENCES users(id) ON DELETE SET NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    used_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_control_install_tokens_community ON control_install_tokens (community_id);
CREATE INDEX IF NOT EXISTS idx_control_install_tokens_expires ON control_install_tokens (expires_at);

CREATE TABLE IF NOT EXISTS control_agents (
    id UUID PRIMARY KEY,
    community_id UUID NOT NULL REFERENCES communities(id) ON DELETE CASCADE,
    token TEXT NOT NULL UNIQUE,
    hostname TEXT NOT NULL DEFAULT '',
    ip TEXT NOT NULL DEFAULT '',
    install_token_id UUID REFERENCES control_install_tokens(id) ON DELETE SET NULL,
    created_by UUID REFERENCES users(id) ON DELETE SET NULL,
    last_seen_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_control_agents_community ON control_agents (community_id);
CREATE INDEX IF NOT EXISTS idx_control_agents_token ON control_agents (token);

CREATE TABLE IF NOT EXISTS control_discoveries (
    id UUID PRIMARY KEY,
    community_id UUID NOT NULL REFERENCES communities(id) ON DELETE CASCADE,
    agent_id UUID NOT NULL REFERENCES control_agents(id) ON DELETE CASCADE,
    instance_name TEXT NOT NULL,
    instance_path TEXT NOT NULL DEFAULT '',
    ip TEXT NOT NULL DEFAULT '',
    port INTEGER,
    server_name TEXT,
    rcon_password TEXT,
    status TEXT NOT NULL DEFAULT 'pending',
    server_id UUID REFERENCES servers(id) ON DELETE SET NULL,
    raw JSONB NOT NULL DEFAULT '{}'::JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_control_discoveries_community ON control_discoveries (community_id);
CREATE INDEX IF NOT EXISTS idx_control_discoveries_agent ON control_discoveries (agent_id);
CREATE INDEX IF NOT EXISTS idx_control_discoveries_status ON control_discoveries (status);
CREATE UNIQUE INDEX IF NOT EXISTS idx_control_discoveries_unique_pending ON control_discoveries (community_id, ip, port, instance_name) WHERE status = 'pending';

CREATE TABLE IF NOT EXISTS control_jobs (
    id UUID PRIMARY KEY,
    server_id UUID NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
    agent_id UUID REFERENCES control_agents(id) ON DELETE SET NULL,
    community_id UUID NOT NULL REFERENCES communities(id) ON DELETE CASCADE,
    action TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    requested_by UUID REFERENCES users(id) ON DELETE SET NULL,
    output TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    started_at TIMESTAMPTZ,
    finished_at TIMESTAMPTZ
);
CREATE INDEX IF NOT EXISTS idx_control_jobs_server ON control_jobs (server_id);
CREATE INDEX IF NOT EXISTS idx_control_jobs_agent_status ON control_jobs (agent_id, status);
CREATE INDEX IF NOT EXISTS idx_control_jobs_status ON control_jobs (status);
CREATE INDEX IF NOT EXISTS idx_control_jobs_created ON control_jobs (created_at DESC);

-- Extend servers for LGSM control
ALTER TABLE servers ADD COLUMN IF NOT EXISTS lgsm_instance TEXT;
ALTER TABLE servers ADD COLUMN IF NOT EXISTS control_agent_id UUID REFERENCES control_agents(id) ON DELETE SET NULL;
ALTER TABLE servers ADD COLUMN IF NOT EXISTS control_last_seen_at TIMESTAMPTZ;
CREATE INDEX IF NOT EXISTS idx_servers_control_agent ON servers (control_agent_id);

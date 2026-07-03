CREATE TABLE IF NOT EXISTS player_server_sessions (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  server_id UUID NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
  server_name TEXT NOT NULL,
  server_port INTEGER NOT NULL,
  community_id UUID NOT NULL REFERENCES communities(id) ON DELETE CASCADE,
  community_name TEXT,
  steam_id64 TEXT NOT NULL,
  player_name TEXT,
  ip TEXT NOT NULL,
  first_seen_at TIMESTAMPTZ NOT NULL,
  last_seen_at TIMESTAMPTZ NOT NULL,
  left_at TIMESTAMPTZ,
  end_reason TEXT,
  last_ping INTEGER,
  last_map TEXT NOT NULL DEFAULT '',
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE player_server_sessions
  ADD COLUMN IF NOT EXISTS end_reason TEXT;

ALTER TABLE player_server_sessions
  ADD COLUMN IF NOT EXISTS last_map TEXT NOT NULL DEFAULT '';

CREATE UNIQUE INDEX IF NOT EXISTS idx_player_server_sessions_active_unique
  ON player_server_sessions (server_id, steam_id64)
  WHERE left_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_player_server_sessions_steamid64
  ON player_server_sessions (steam_id64, (COALESCE(left_at, last_seen_at)) DESC);

CREATE INDEX IF NOT EXISTS idx_player_server_sessions_server_id
  ON player_server_sessions (server_id);

CREATE INDEX IF NOT EXISTS idx_player_server_sessions_left_at
  ON player_server_sessions (left_at);

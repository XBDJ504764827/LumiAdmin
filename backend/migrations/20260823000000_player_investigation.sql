-- Player investigation workspace: tag catalog/assignments and immutable note history.

CREATE TABLE IF NOT EXISTS player_tag_definitions (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  name TEXT NOT NULL,
  color TEXT NOT NULL DEFAULT '#64748b',
  description TEXT NOT NULL DEFAULT '',
  created_by UUID REFERENCES users(id) ON DELETE SET NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX IF NOT EXISTS uq_player_tag_definitions_name
  ON player_tag_definitions (lower(name));

CREATE TABLE IF NOT EXISTS player_tag_assignments (
  steamid64 TEXT NOT NULL,
  tag_id UUID NOT NULL REFERENCES player_tag_definitions(id) ON DELETE CASCADE,
  assigned_by UUID REFERENCES users(id) ON DELETE SET NULL,
  assigned_by_name TEXT NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  PRIMARY KEY (steamid64, tag_id)
);

CREATE INDEX IF NOT EXISTS idx_player_tag_assignments_tag
  ON player_tag_assignments (tag_id, created_at DESC);

CREATE TABLE IF NOT EXISTS player_internal_note_history (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  steamid64 TEXT NOT NULL,
  note TEXT NOT NULL DEFAULT '',
  tags TEXT[] NOT NULL DEFAULT ARRAY[]::TEXT[],
  changed_by UUID REFERENCES users(id) ON DELETE SET NULL,
  changed_by_name TEXT NOT NULL,
  changed_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_player_internal_note_history_player
  ON player_internal_note_history (steamid64, changed_at DESC);

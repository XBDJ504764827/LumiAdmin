ALTER TABLE player_server_sessions
  ADD COLUMN IF NOT EXISTS end_detail TEXT;

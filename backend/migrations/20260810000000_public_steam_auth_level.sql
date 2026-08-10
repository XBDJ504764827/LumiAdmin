-- public_steam_auth_sessions: 追加 steam_level 字段
ALTER TABLE public_steam_auth_sessions ADD COLUMN IF NOT EXISTS steam_level INTEGER;

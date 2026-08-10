-- 公共页面 Steam OpenID 认证会话表
-- 用于白名单申请等公开页面验证用户 Steam 身份

CREATE TABLE IF NOT EXISTS public_steam_auth_sessions (
  id UUID PRIMARY KEY,
  steamid64 TEXT NOT NULL,
  steamid TEXT,
  steamid3 TEXT,
  profile_url TEXT,
  persona_name TEXT,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  expires_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_public_steam_auth_sessions_expires
  ON public_steam_auth_sessions (expires_at);

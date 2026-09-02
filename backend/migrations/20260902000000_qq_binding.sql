-- QQ 群绑定功能
--
-- 玩家通过 Steam 验证后，网站展示一次性绑定 UUID；
-- 玩家将该 UUID 发到 QQ 群 @机器人，机器人获取发送者 openid 调用绑定 API，
-- 建立 steamid64 ↔ qq_openid 的绑定，供管理员追溯联系方式。
--
-- 1. qq_bind_codes：一次性绑定码（pending -> bound / expired）
-- 2. player_qq_bindings：steamid64 ↔ qq_openid 持久绑定（支持解绑）

CREATE TABLE IF NOT EXISTS qq_bind_codes (
  id UUID PRIMARY KEY,
  steamid64 TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'pending',
  bound_openid TEXT,
  bound_at TIMESTAMPTZ,
  expires_at TIMESTAMPTZ NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  CONSTRAINT qq_bind_codes_status CHECK (status IN ('pending', 'bound', 'expired', 'revoked'))
);

CREATE INDEX IF NOT EXISTS idx_qq_bind_codes_steamid64
  ON qq_bind_codes (steamid64);

CREATE INDEX IF NOT EXISTS idx_qq_bind_codes_status_expires
  ON qq_bind_codes (status, expires_at);

CREATE TABLE IF NOT EXISTS player_qq_bindings (
  id UUID PRIMARY KEY,
  steamid64 TEXT NOT NULL UNIQUE,
  qq_openid TEXT NOT NULL,
  source TEXT NOT NULL DEFAULT 'group_code',
  bound_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  unbound_at TIMESTAMPTZ,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  CONSTRAINT player_qq_bindings_source CHECK (source IN ('group_code', 'manual'))
);

CREATE INDEX IF NOT EXISTS idx_player_qq_bindings_openid
  ON player_qq_bindings (qq_openid);

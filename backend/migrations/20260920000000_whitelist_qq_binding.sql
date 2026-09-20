-- 白名单两步验证：Steam 验证 + QQ 群验证码绑定
--
-- 玩家提交白名单前必须先在 QQ 群内 @机器人 发送网站生成的验证码，
-- 由 LumiBot 调 LumiAdmin 完成 SteamID64 与 QQ(openid) 的绑定，
-- 之后管理员才能通过绑定信息在群内找到玩家。
--
-- 1. whitelist_qq_config        单行配置：群号 / 加群链接 / 允许群 openid / 绑定上限 / 验证码有效期
-- 2. whitelist_qq_verify_codes  一次性验证码（默认 5 分钟有效，单个 Steam 仅保留一条活跃码）
-- 3. steam_qq_bindings          SteamID64 ↔ QQ openid 绑定（1 个 QQ 最多绑定 N 个 Steam）
-- 4. qq_mention_logs            管理员「群内 @玩家」操作记录（成功/失败均落库）
-- 5. whitelist_requests         新增 QQ 绑定快照列与 Steam 验证方式列

-- ─────────────────────────────────────────────────────────────────────────────
-- 1. QQ 群配置（单行）
-- ─────────────────────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS whitelist_qq_config (
  id BOOLEAN PRIMARY KEY DEFAULT true,
  -- 是否要求玩家完成 QQ 群绑定后才能提交白名单
  enabled BOOLEAN NOT NULL DEFAULT true,
  -- 展示用 QQ 群号
  group_number TEXT NOT NULL DEFAULT '275164688',
  -- 一键加群链接
  group_link TEXT,
  -- 允许绑定/发@的 QQ 群 openid 白名单（QQ API 使用的群 openid，非数字群号）；
  -- 为空表示不限制（首次绑定时会自动写入收到的群 openid，由管理员确认）
  group_openids TEXT[] NOT NULL DEFAULT ARRAY[]::TEXT[],
  -- 单个 QQ 最多绑定的 Steam 数量
  max_bindings INTEGER NOT NULL DEFAULT 5,
  -- 验证码有效期（秒）
  code_ttl_seconds INTEGER NOT NULL DEFAULT 300,
  updated_by TEXT,
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  CONSTRAINT whitelist_qq_config_single_row CHECK (id)
);

INSERT INTO whitelist_qq_config (id, enabled, group_number, group_link)
VALUES (true, true, '275164688', 'https://qm.qq.com/q/vRqmiKS6oE')
ON CONFLICT (id) DO NOTHING;

-- ─────────────────────────────────────────────────────────────────────────────
-- 2. 一次性验证码
-- ─────────────────────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS whitelist_qq_verify_codes (
  id UUID PRIMARY KEY,
  steamid64 TEXT NOT NULL,
  code TEXT NOT NULL UNIQUE,
  expires_at TIMESTAMPTZ NOT NULL,
  consumed_at TIMESTAMPTZ,
  consumed_by_qq_openid TEXT,
  -- 校验失败尝试次数（用于限制爆破）
  attempts INTEGER NOT NULL DEFAULT 0,
  created_ip TEXT,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- 单个 Steam 只保留最近一条活跃码：按 steamid64 查活跃码
CREATE INDEX IF NOT EXISTS idx_whitelist_qq_verify_codes_steam_active
  ON whitelist_qq_verify_codes (steamid64, expires_at DESC)
  WHERE consumed_at IS NULL;

-- ─────────────────────────────────────────────────────────────────────────────
-- 3. Steam ↔ QQ 绑定
-- ─────────────────────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS steam_qq_bindings (
  id UUID PRIMARY KEY,
  steamid64 TEXT NOT NULL UNIQUE,
  qq_openid TEXT NOT NULL,
  qq_group_id TEXT NOT NULL,
  qq_username TEXT,
  verified_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_steam_qq_bindings_qq_openid
  ON steam_qq_bindings (qq_openid);

-- ─────────────────────────────────────────────────────────────────────────────
-- 4. 管理员群内 @玩家 记录
-- ─────────────────────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS qq_mention_logs (
  id UUID PRIMARY KEY,
  steamid64 TEXT NOT NULL,
  qq_openid TEXT NOT NULL,
  qq_group_id TEXT NOT NULL,
  content TEXT NOT NULL,
  -- sent / failed
  status TEXT NOT NULL,
  error TEXT,
  operator_id UUID,
  operator_name TEXT,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_qq_mention_logs_steamid64
  ON qq_mention_logs (steamid64, created_at DESC);

-- ─────────────────────────────────────────────────────────────────────────────
-- 5. 白名单申请表：QQ 绑定快照 + Steam 验证方式
-- ─────────────────────────────────────────────────────────────────────────────
ALTER TABLE whitelist_requests ADD COLUMN IF NOT EXISTS qq_openid TEXT;
ALTER TABLE whitelist_requests ADD COLUMN IF NOT EXISTS qq_group_id TEXT;
ALTER TABLE whitelist_requests ADD COLUMN IF NOT EXISTS qq_username TEXT;
ALTER TABLE whitelist_requests ADD COLUMN IF NOT EXISTS qq_verified_at TIMESTAMPTZ;
-- Steam 是否通过 OpenID 登录验证（false = 玩家手动填写 Steam 标识）
ALTER TABLE whitelist_requests ADD COLUMN IF NOT EXISTS steam_verified BOOLEAN NOT NULL DEFAULT false;

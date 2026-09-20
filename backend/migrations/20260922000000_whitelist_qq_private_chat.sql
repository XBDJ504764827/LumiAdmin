-- 白名单 QQ 验证：从「QQ 群 @机器人」改为「添加机器人好友后私聊验证码」
--
-- 背景：QQ 官方平台对机器人「群内主动消息」有权限限制（40034105 主动消息无权限），
-- 导致绑定回执与管理员群内 @玩家 不稳定。改为私聊（C2C）后：
--   玩家私发验证码 → Bot 被动回复绑定结果（不受主动消息限制）；
--   管理员可在网站发起私聊（已开通主动消息权限）。
--
-- 1. whitelist_qq_config   QQ 群相关列替换为机器人 QQ 号（bot_qq）
-- 2. steam_qq_bindings     绑定来源标记 qq_scene（c2c/group），群绑定数据清理
-- 3. qq_chat_messages      管理员 ↔ 玩家聊天记录

-- ─────────────────────────────────────────────────────────────────────────────
-- 1. 配置表：移除群号/加群链接/群 openid 白名单，新增机器人 QQ 号
-- ─────────────────────────────────────────────────────────────────────────────
ALTER TABLE whitelist_qq_config ADD COLUMN IF NOT EXISTS bot_qq TEXT NOT NULL DEFAULT '3889010779';
ALTER TABLE whitelist_qq_config DROP COLUMN IF EXISTS group_number;
ALTER TABLE whitelist_qq_config DROP COLUMN IF EXISTS group_link;
ALTER TABLE whitelist_qq_config DROP COLUMN IF EXISTS group_openids;

-- whitelist_qq_config.bot_name 由上一迁移新增（默认 CNGOKZBOT），此处保证存在
ALTER TABLE whitelist_qq_config ADD COLUMN IF NOT EXISTS bot_name TEXT NOT NULL DEFAULT 'CNGOKZBOT';

-- ─────────────────────────────────────────────────────────────────────────────
-- 2. 绑定表：标记来源；清理旧的群绑定（改为私聊后需重新私聊验证）
-- ─────────────────────────────────────────────────────────────────────────────
ALTER TABLE steam_qq_bindings ADD COLUMN IF NOT EXISTS qq_scene TEXT NOT NULL DEFAULT 'c2c';
-- 私聊绑定没有群 ID，允许为空
ALTER TABLE steam_qq_bindings ALTER COLUMN qq_group_id DROP NOT NULL;
-- 旧的群绑定无法用于私聊聊天（openid 是群场景，与 C2C openid 不同命名空间），
-- 全部清空以强制玩家重新私聊验证；否则默认 qq_scene='c2c' 会误标为可用。
DELETE FROM steam_qq_bindings;

-- ─────────────────────────────────────────────────────────────────────────────
-- 3. 管理员 ↔ 玩家聊天记录
-- ─────────────────────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS qq_chat_messages (
  id UUID PRIMARY KEY,
  -- 关联 Steam（玩家私聊时若已绑定则回填；管理员发送时必须提供）
  steamid64 TEXT,
  -- 玩家 QQ openid（C2C 场景）
  qq_openid TEXT NOT NULL,
  -- 方向：admin_to_player（管理员发送）/ player_to_admin（玩家私聊）
  direction TEXT NOT NULL,
  content TEXT NOT NULL,
  -- 状态：sent（已送达）/ failed（发送失败）/ received（玩家发来）
  status TEXT NOT NULL DEFAULT 'sent',
  error TEXT,
  operator_id UUID,
  operator_name TEXT,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_qq_chat_messages_steam
  ON qq_chat_messages (steamid64, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_qq_chat_messages_openid
  ON qq_chat_messages (qq_openid, created_at DESC);

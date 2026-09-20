-- 白名单 QQ 绑定：引导文案使用的机器人名称（如 CNGOKZBOT），
-- 供公开申请页提示玩家在群内 @哪个机器人发送验证码。
ALTER TABLE whitelist_qq_config ADD COLUMN IF NOT EXISTS bot_name TEXT NOT NULL DEFAULT 'CNGOKZBOT';

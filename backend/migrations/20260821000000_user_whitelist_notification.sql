-- 网站用户白名单申请 QQ 通知开关。
-- 默认开启，保持已有“填写 openid 即接收通知”的行为；用户可在网站用户管理中关闭。
ALTER TABLE users
  ADD COLUMN IF NOT EXISTS whitelist_notification_enabled BOOLEAN NOT NULL DEFAULT true;

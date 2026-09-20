-- 账号维度登录失败记录：用于登录爆破防护（15 分钟窗口内失败 5 次锁定）。
-- 与 IP 限流互补，攻击者更换 IP 也无法慢速爆破同一账号。
-- 过期行由 session 清理循环定期删除。

CREATE TABLE IF NOT EXISTS login_failures (
  id BIGSERIAL PRIMARY KEY,
  username TEXT NOT NULL,
  failed_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_login_failures_username_failed_at
  ON login_failures (username, failed_at);
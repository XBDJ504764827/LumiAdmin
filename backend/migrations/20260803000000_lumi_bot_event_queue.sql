-- LumiBot（QQ 机器人事件接收中心）上报队列
--
-- 外部事件（目前为白名单新申请）先写入本表，由后台任务每
-- LUMI_BOT_SYNC_INTERVAL_SECS 秒集中通过 POST /api/v1/events 上报给 LumiBot。
-- 上报成功标记为 sent；失败保留为 pending 并累计 attempts，超过
-- LUMI_BOT_MAX_ATTEMPTS 后标记为 failed（不再自动重试，便于人工排查）。

CREATE TABLE IF NOT EXISTS lumi_bot_event_queue (
  id UUID PRIMARY KEY,
  -- 事件类型（对应 LumiBot HTTP API 的 event_type，如 WHITELIST_REQUEST_CREATED）
  event_type TEXT NOT NULL,
  -- 事件级别：info / warning / error / critical（对应 LumiBot level）
  level TEXT NOT NULL DEFAULT 'info',
  -- 通知标题（LumiBot 通知展示用）
  title TEXT,
  -- 事件描述
  message TEXT,
  -- 业务附加数据（任意 JSON，LumiBot 原样保留）
  data JSONB NOT NULL DEFAULT '{}'::JSONB,
  -- 队列状态：pending（待上报/待重试）/ sent（已上报成功）/ failed（重试耗尽）
  status TEXT NOT NULL DEFAULT 'pending',
  -- 已尝试上报次数
  attempts INTEGER NOT NULL DEFAULT 0,
  -- 最近一次上报失败原因
  last_error TEXT,
  -- 事件发生时间（作为 LumiBot 事件的 timestamp 字段上报）
  occurred_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  queued_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  sent_at TIMESTAMPTZ,
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_lumi_bot_event_queue_pending
  ON lumi_bot_event_queue (queued_at)
  WHERE status = 'pending';

CREATE INDEX IF NOT EXISTS idx_lumi_bot_event_queue_status
  ON lumi_bot_event_queue (status, queued_at);

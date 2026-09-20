/** 服务器详细弹窗的纯函数工具（便于单测与复用）。 */

export function formatAuthSyncTime(value, now = Date.now()) {
  if (!value) return '—';
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return '—';
  const diffSecs = Math.max(0, Math.floor((now - date.getTime()) / 1000));
  if (diffSecs < 60) return `${diffSecs} 秒前`;
  if (diffSecs < 3600) return `${Math.floor(diffSecs / 60)} 分钟前`;
  return date.toLocaleString();
}

/** 授权同步摘要：连接状态 / 版本 / 待同步 / 上次同步。 */
export function buildAuthSyncSummary(server, now = Date.now()) {
  const connected = server?.auth_connection === 'connected';
  const version = server?.auth_version ?? 0;
  const latest = server?.auth_latest_version ?? 0;
  const pending = server?.auth_pending_events ?? Math.max(0, latest - version);
  return {
    connected,
    version,
    latest,
    pending,
    lag: pending > 0,
    lastSyncText: formatAuthSyncTime(server?.auth_last_sync_at, now),
  };
}

/** 掩码 Token：仅保留末尾 4 位，短 Token 全掩码。 */
export function maskReportToken(token) {
  if (!token) return '••••••••••••';
  if (token.length <= 4) return '••••';
  return `${'•'.repeat(Math.min(12, token.length - 4))}${token.slice(-4)}`;
}

/** 在线人数摘要文本。 */
export function buildPlayerSummary(server) {
  const online = server?.online_player_count ?? server?.players?.length ?? 0;
  const max = server?.max_players ?? 0;
  return { online, max };
}

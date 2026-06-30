import { formatChinaDateTime } from '../../shared/time.js';

export function formatBanDuration(minutes) {
  if (!minutes) return '永久';
  if (minutes % 60 === 0) return `${minutes / 60} 小时`;
  return `${minutes} 分钟`;
}

export function formatBanSource(source) {
  switch (source) {
    case 'global_ban':
      return '全球封禁同步至网站';
    case 'game_plugin':
      return '游戏管理员手动封禁';
    case 'web':
    case 'manual':
      return '网站管理员手动封禁';
    case 'offline_sync':
      return '游戏离线操作同步封禁';
    case 'external_api':
      return '外部封禁 API 同步封禁';
    default:
      return source ? `其他来源封禁（${source}）` : '未知来源封禁';
  }
}

export function formatExpiresAt(value) {
  if (!value) return '永不过期';
  return formatChinaDateTime(value);
}

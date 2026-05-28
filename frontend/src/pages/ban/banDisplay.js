import { formatChinaDateTime } from '../../shared/time.js';

export function formatBanDuration(minutes) {
  if (!minutes) return '永久';
  if (minutes % 60 === 0) return `${minutes / 60} 小时`;
  return `${minutes} 分钟`;
}

export function formatBanSource(source, operatorName) {
  if (source === 'game_plugin') {
    return operatorName ? `游戏内 - ${operatorName}` : '游戏内命令';
  }
  return '网站手动';
}

export function formatExpiresAt(value) {
  if (!value) return '永不过期';
  return formatChinaDateTime(value);
}

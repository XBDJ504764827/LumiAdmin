import { formatChinaDateTime } from '../../shared/time.js';
import {
  eventSourceLabel,
  sessionReasonKind,
  sessionReasonLabel,
} from '../../shared/sessionReasons.js';

export {
  eventSourceLabel,
  sessionReasonKind,
  sessionReasonLabel,
};

const STATUS_LABELS = {
  active: '生效中',
  inactive: '已失效',
  pending: '待审核',
  approved: '已通过',
  rejected: '已驳回',
  revoked: '已撤销',
  resolved: '已解决',
  online: '在线',
  hibernating: '休眠/待上报',
  success: '成功',
  failed: '失败',
};

const ACCESS_METHOD_LABELS = {
  unrestricted: '无限制放行',
  whitelist: '白名单放行',
  restriction: 'Rating 限制通过',
  cs_prime: 'CS优先账户放行',
  custom_rule: '自定义规则放行',
  banned: '封禁拒绝',
  whitelist_rejected: '白名单拒绝',
  restriction_rejected: 'Rating/等级拒绝',
  cs_prime_rejected: '非CS优先账户拒绝',
  custom_rule_rejected: '自定义规则拒绝',
  snapshot_fallback: '快照回退',
};

const FAILURE_CODE_LABELS = {
  banned: '存在有效封禁',
  not_whitelisted: '未通过白名单',
  low_rating: 'Rating 不足',
  low_steam_level: 'Steam 等级不足',
  not_cs_prime: '非 CS 优先账户',
  custom_rule_rejected: '自定义规则拒绝',
  profile_fetch_failed: '无法获取玩家资料',
  prime_verification_failed: '无法验证 CS 优先账户',
  snapshot_unavailable: '访问控制服务不可用',
};

const CATEGORY_LABELS = {
  whitelist: '白名单',
  ban: '封禁',
  appeal: '申诉',
  report: '举报',
  online: '在线',
  session: '会话',
  access: '进服',
  admin: '后台操作',
  audit: '审计',
  evidence: '证据',
  map_feedback: '地图反馈',
};

export function stKind(status, category) {
  if (status === 'active' || category === 'ban') return status === 'inactive' ? 'offline' : 'danger';
  if (status === 'online' || status === 'approved' || status === 'success' || status === 'resolved') return 'success';
  if (status === 'pending') return 'warning';
  if (status === 'failed' || status === 'rejected') return 'danger';
  if (status === 'revoked' || status === 'inactive') return 'offline';
  return 'default';
}

export function stLabel(status) {
  return STATUS_LABELS[status] || status || '-';
}

export function durLabel(minutes) {
  const value = Number(minutes);
  if (!Number.isFinite(value)) return '-';
  if (value === 0) return '永久';
  if (value < 60) return `${value} 分钟`;
  if (value % 1440 === 0) return `${value / 1440} 天`;
  if (value % 60 === 0) return `${value / 60} 小时`;
  return `${value} 分钟`;
}

export function sessionDurationLabel(seconds) {
  const value = Number(seconds);
  if (!Number.isFinite(value) || value < 0) return '-';
  if (value < 60) return `${Math.round(value)} 秒`;
  const minutes = Math.floor(value / 60);
  if (minutes < 60) return `${minutes} 分钟`;
  const hours = Math.floor(minutes / 60);
  const restMinutes = minutes % 60;
  if (hours < 24) return restMinutes ? `${hours} 小时 ${restMinutes} 分钟` : `${hours} 小时`;
  const days = Math.floor(hours / 24);
  const restHours = hours % 24;
  return restHours ? `${days} 天 ${restHours} 小时` : `${days} 天`;
}

export function sessionEndLabel(item) {
  return item?.left_at ? formatChinaDateTime(item.left_at, { seconds: false }) : '仍在线';
}

export function tagsToText(tags = []) {
  return tags.join(', ');
}

export function textToTags(value) {
  return value
    .split(/[,，\n]/)
    .map((tag) => tag.trim().replace(/^#/, ''))
    .filter(Boolean)
    .filter((tag, index, all) => all.findIndex((item) => item.toLowerCase() === tag.toLowerCase()) === index);
}

export function feedbackTypeLabel(type) {
  const labels = { missing: '地图缺失', broken: '地图损坏', request: '地图请求' };
  return labels[type] || type || '-';
}

export function methodLabel(method) {
  return ACCESS_METHOD_LABELS[method] || method || '-';
}

export function failureLabel(item) {
  return FAILURE_CODE_LABELS[item?.failure_code] || item?.reject_reason || item?.failure_code || '-';
}

export function categoryLabel(category) {
  return CATEGORY_LABELS[category] || category || '事件';
}

export function categoryKind(category, status) {
  if (status === 'failed' || category === 'ban') return 'danger';
  if (status === 'pending') return 'warning';
  if (status === 'success' || status === 'approved' || status === 'online') return 'success';
  if (category === 'session') return status === 'online' ? 'success' : 'offline';
  if (category === 'access') return status === 'success' ? 'success' : 'danger';
  return 'default';
}

export function count(value) {
  return Number(value || 0);
}

export function countActiveGlobalBans(items = []) {
  return items.filter((item) => {
    const expires = item.ban?.expires_on;
    if (!expires || expires.startsWith('9999')) return true;
    const date = new Date(expires);
    return !Number.isNaN(date.getTime()) && date > new Date();
  }).length;
}

export function riskTone(action) {
  if (action === 'deny' || action === 'require_force') return 'danger';
  if (action === 'warn') return 'warning';
  return 'success';
}

export function riskLabel(action) {
  if (action === 'deny') return '建议拒绝';
  if (action === 'require_force') return '强制复核';
  if (action === 'warn') return '需要复核';
  return '正常处理';
}

export function latestItems(items = [], limit = 8) {
  return [...items]
    .sort((a, b) => new Date(b.occurred_at) - new Date(a.occurred_at))
    .slice(0, limit);
}

export function candidateMeta(candidate) {
  const parts = [];
  if (candidate.active_ban_count > 0) parts.push(`${candidate.active_ban_count} 条活跃封禁`);
  if (candidate.whitelist_status) parts.push(`白名单 ${stLabel(candidate.whitelist_status)}`);
  if (candidate.last_seen_at) parts.push(`最近 ${formatChinaDateTime(candidate.last_seen_at, { seconds: false })}`);
  return parts;
}

export function latestWhitelistContact(detail) {
  return (detail?.whitelist || [])
    .map((item) => item.contact)
    .find((value) => value && String(value).trim()) || null;
}

export function normalizeAdminPreviewRows(items = []) {
  return items.map((item) => {
    const displayName = item.display_name ?? '';

    return {
      displayName,
      role: item.role,
      roleLabel: item.role_label,
      status: item.status,
      initials: displayName.trim().charAt(0).toUpperCase() || '?',
    };
  });
}

// ═══════════════════════════════════════════════════════════
// Dashboard Analytics 数据规范化（纯函数，便于单元测试）
// ═══════════════════════════════════════════════════════════

const CHINA_TZ = 'Asia/Shanghai';

/** 千分位数字格式化，如 1284 → '1,284' */
export function formatNumber(value) {
  return new Intl.NumberFormat('zh-CN').format(Number(value) || 0);
}

/**
 * 环比变化百分比（保留 1 位小数）。
 * 上一周期为 0 或缺失时返回 null，表示无法计算。
 */
export function percentChange(current, previous) {
  const prev = Number(previous);
  if (!prev || prev <= 0) return null;
  return Math.round(((Number(current) - prev) / prev) * 1000) / 10;
}

/** 百分比展示，如 12.5 → '+12.5%'，-3 → '-3%'；null 原样返回 */
export function formatPercentChange(pct) {
  if (pct == null) return null;
  return `${pct > 0 ? '+' : ''}${pct}%`;
}

/** 将 Date 转为 Asia/Shanghai 时区的 'YYYY-MM-DD' 键 */
function chinaDateKey(value) {
  const parts = new Intl.DateTimeFormat('zh-CN', {
    timeZone: CHINA_TZ,
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
  }).formatToParts(value);
  const map = Object.fromEntries(parts.map((part) => [part.type, part.value]));
  return `${map.year}-${map.month}-${map.day}`;
}

/** 在 'YYYY-MM-DD' 键上偏移天数（按 UTC 计算避免时区干扰） */
function shiftDateKey(dateKey, offset) {
  const [year, month, day] = dateKey.split('-').map(Number);
  const date = new Date(Date.UTC(year, month - 1, day + offset));
  return date.toISOString().slice(0, 10);
}

/**
 * 将后端按日聚合的白名单新增补齐为完整窗口（无数据的日期补 0），
 * 返回图表可直接使用的 { labels, counts }。
 * 后端只返回有数据的日期，直接连线会跨越空缺日期。
 */
export function fillWhitelistTrendWindow(items = [], days, todayKey = chinaDateKey(new Date())) {
  const byDate = new Map(items.map((item) => [item.date, Number(item.count) || 0]));
  const labels = [];
  const counts = [];
  for (let offset = days - 1; offset >= 0; offset -= 1) {
    const key = shiftDateKey(todayKey, -offset);
    labels.push(key.slice(5)); // MM-DD
    counts.push(byDate.get(key) ?? 0);
  }
  return { labels, counts };
}

/** 活跃度时间点标签：hour → 'HH:00'，day → 'MM-DD'（Asia/Shanghai） */
export function formatActivityLabel(time, unit = 'hour') {
  if (!time) return '-';
  const parts = new Intl.DateTimeFormat('zh-CN', {
    timeZone: CHINA_TZ,
    hour12: false,
    hourCycle: 'h23',
    ...(unit === 'hour'
      ? { hour: '2-digit', minute: '2-digit' }
      : { month: '2-digit', day: '2-digit' }),
  }).formatToParts(new Date(time));
  const map = Object.fromEntries(parts.map((part) => [part.type, part.value]));
  return unit === 'hour' ? `${map.hour}:${map.minute}` : `${map.month}-${map.day}`;
}

/** 服务器活跃度趋势数据 → 图表数据集（活跃玩家 / 会话数） */
export function buildServerActivityData(items = [], unit = 'hour') {
  return {
    labels: items.map((item) => formatActivityLabel(item.time, unit)),
    activePlayers: items.map((item) => Number(item.active_players) || 0),
    sessions: items.map((item) => Number(item.sessions) || 0),
  };
}

/** 服务器状态展示文案（与后端 status 字段一一对应） */
export const SERVER_STATUS_META = {
  online: { label: '在线' },
  hibernating: { label: '休眠' },
  untested: { label: '未测试' },
  offline: { label: '离线' },
};

/** 服务器状态分布 → 环形图数据（含总数） */
export function buildServerStatusData(items = []) {
  const mapped = items.map((item) => ({
    status: item.status,
    label: SERVER_STATUS_META[item.status]?.label ?? item.status,
    count: Number(item.count) || 0,
  }));
  const total = mapped.reduce((sum, item) => sum + item.count, 0);
  return { items: mapped, total };
}

/** 服务器活跃度排行 → 横向条形图数据 */
export function buildServerRankingData(items = []) {
  return {
    names: items.map((item) => item.server_name ?? '-'),
    activePlayers: items.map((item) => Number(item.active_players) || 0),
    sessions: items.map((item) => Number(item.sessions) || 0),
    playtimeSeconds: items.map((item) => Number(item.playtime_seconds) || 0),
  };
}

/** 秒 → 人类可读时长（用于排行 Tooltip） */
export function formatPlaytime(seconds) {
  const s = Number(seconds) || 0;
  if (s <= 0) return '-';
  if (s < 3600) return `${Math.max(1, Math.round(s / 60))} 分钟`;
  const hours = s / 3600;
  if (hours < 24) return `${Math.round(hours * 10) / 10} 小时`;
  return `${Math.round((hours / 24) * 10) / 10} 天`;
}


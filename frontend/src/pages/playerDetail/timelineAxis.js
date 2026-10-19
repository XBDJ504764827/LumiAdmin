/**
 * 坐标轴时间线的纯计算逻辑（不依赖 DOM / React），便于单元测试。
 */

export const HOUR_MS = 3600 * 1000;
export const DAY_MS = 24 * HOUR_MS;

/** 单个泳道内每行圆点高度 */
export const AXIS_LANE_ROW_HEIGHT = 34;
/** 同一行相邻圆点的最小像素间距，小于该值则换行，避免重叠 */
export const AXIS_DOT_MIN_GAP = 18;
/** 每个刻度之间期望的最小像素宽度 */
const MIN_TICK_GAP_PX = 84;

export const AXIS_RANGES = [
  { key: 'auto', label: '自动聚焦' },
  { key: 'all', label: '全部' },
  { key: '1y', label: '近 1 年', ms: 365 * DAY_MS },
  { key: '90d', label: '近 90 天', ms: 90 * DAY_MS },
  { key: '30d', label: '近 30 天', ms: 30 * DAY_MS },
  { key: '7d', label: '近 7 天', ms: 7 * DAY_MS },
  { key: '1d', label: '近 24 小时', ms: DAY_MS },
];

const TICK_STEPS = [
  HOUR_MS, 3 * HOUR_MS, 6 * HOUR_MS, 12 * HOUR_MS, DAY_MS, 2 * DAY_MS,
  7 * DAY_MS, 14 * DAY_MS, 30 * DAY_MS, 90 * DAY_MS, 180 * DAY_MS, 365 * DAY_MS,
];

/** 按缩放比例挑选刻度间隔，保证刻度标签不会挤在一起。 */
export function pickTickStep(pxPerMs, minGapPx = MIN_TICK_GAP_PX) {
  for (const step of TICK_STEPS) {
    if (step * pxPerMs >= minGapPx) return step;
  }
  return TICK_STEPS[TICK_STEPS.length - 1];
}

/** 生成时间轴刻度（毫秒时间戳数组），首尾都落在 [start, end] 内。 */
export function buildTicks(start, end, pxPerMs) {
  const step = pickTickStep(pxPerMs);
  if (!Number.isFinite(start) || !Number.isFinite(end) || end <= start) {
    return { step, items: [] };
  }
  const first = Math.ceil(start / step) * step;
  const items = [];
  for (let value = first; value <= end; value += step) items.push(value);
  return { step, items };
}

/**
 * 按分类分泳道，并在每条泳道内把时间相近的点分行，避免圆点互相覆盖。
 *
 * @returns {Array<{category: string, rows: number, items: Array<{event: object, x: number, row: number}>}>}
 */
export function buildLanes(events, { start, pxPerMs, insetX = 0, order = [], rowHeight = AXIS_LANE_ROW_HEIGHT, minGap = AXIS_DOT_MIN_GAP }) {
  const byCategory = new Map();
  for (const event of events) {
    const time = new Date(event.occurred_at).getTime();
    if (!Number.isFinite(time)) continue;
    if (!byCategory.has(event.category)) byCategory.set(event.category, []);
    byCategory.get(event.category).push({ event, time });
  }

  const rank = (category) => {
    const index = order.indexOf(category);
    return index < 0 ? order.length : index;
  };
  const categories = [...byCategory.keys()].sort((a, b) => rank(a) - rank(b) || a.localeCompare(b));

  return categories.map((category) => {
    const sorted = [...byCategory.get(category)].sort((a, b) => a.time - b.time);
    const rowEnds = [];
    const items = sorted.map(({ event, time }) => {
      const x = insetX + Math.max(0, (time - start) * pxPerMs);
      let row = rowEnds.findIndex((lastX) => x - lastX >= minGap);
      if (row === -1) {
        row = rowEnds.length;
        rowEnds.push(x);
      } else {
        rowEnds[row] = x;
      }
      return { event, x, row };
    });
    return { category, rows: Math.max(1, rowEnds.length), items, rowHeight };
  });
}

/**
 * 单轴模式：所有事件放在同一条时间轴上，时间相近的点自动分多层。
 * @returns {{rows: number, items: Array<{event: object, x: number, row: number}>}}
 */
export function buildSingleLane(events, { start, pxPerMs, insetX = 0, minGap = AXIS_DOT_MIN_GAP }) {
  const sorted = events
    .map((event) => ({ event, time: new Date(event.occurred_at).getTime() }))
    .filter((item) => Number.isFinite(item.time))
    .sort((a, b) => a.time - b.time);
  const rowEnds = [];
  const items = sorted.map(({ event, time }) => {
    const x = insetX + Math.max(0, (time - start) * pxPerMs);
    let row = rowEnds.findIndex((lastX) => x - lastX >= minGap);
    if (row === -1) {
      row = rowEnds.length;
      rowEnds.push(x);
    } else {
      rowEnds[row] = x;
    }
    return { event, x, row };
  });
  return { rows: Math.max(1, rowEnds.length), items };
}

/** 时间轴泳道总高度（含每条泳道底部留白）。 */
export function axisTotalHeight(lanes, rowHeight = AXIS_LANE_ROW_HEIGHT, padding = 14) {
  return lanes.reduce((sum, lane) => sum + lane.rows * rowHeight + padding, 0);
}

/** 计算所有事件的完整时间边界（用于总览条与平移钳制）。 */
export function buildBounds(events, fallbackNow = 0) {
  let min = Infinity;
  let max = -Infinity;
  for (const event of events) {
    const time = new Date(event.occurred_at).getTime();
    if (!Number.isFinite(time)) continue;
    if (time < min) min = time;
    if (time > max) max = time;
  }
  if (!Number.isFinite(min) || !Number.isFinite(max)) {
    min = fallbackNow - DAY_MS;
    max = fallbackNow;
  }
  if (max - min < HOUR_MS) {
    min -= HOUR_MS;
    max += HOUR_MS;
  }
  return { min, max };
}

/**
 * 自动聚焦：找出时间轴上最大的空档，取最近的一段作为默认视野。
 * 典型场景：玩家 2021 年有一条全球封禁、2026 年才活跃，默认直接聚焦最近的活跃期，
 * 避免所有点挤在坐标轴两端。
 */
export function buildAutoWindow(events, { paddingRatio = 0.08 } = {}) {
  const times = events
    .map((event) => new Date(event.occurred_at).getTime())
    .filter((time) => Number.isFinite(time))
    .sort((a, b) => a - b);
  if (times.length < 2) return null;

  const total = times[times.length - 1] - times[0];
  // 所有事件集中在 180 天内则无需聚焦
  if (total <= 180 * DAY_MS) return null;

  let gapIndex = -1;
  let gap = 0;
  for (let i = 1; i < times.length; i += 1) {
    const current = times[i] - times[i - 1];
    if (current > gap) { gap = current; gapIndex = i; }
  }
  // 最大空档不明显或空档后没有足够事件时，回退到全部
  if (gap < total * 0.35 || gapIndex <= 0) return null;

  let start = times[gapIndex];
  let end = times[times.length - 1];
  // 同一时刻的事件很多时避免窗口宽度为 0
  if (end - start < DAY_MS) {
    start = end - DAY_MS;
  }
  const padding = Math.max((end - start) * paddingRatio, 6 * HOUR_MS);
  return { start: start - padding, end: end + padding };
}

/**
 * 计算时间轴的可见区间。
 * @param {Array} events 事件列表
 * @param {string} rangeKey AXIS_RANGES 中的 key
 * @param {number} fallbackNow 无事件时使用的时间基准
 * @param {number|null} anchorEnd 预设范围的结束锚点（用于平移），null 表示最新事件
 */
export function buildDomain(events, rangeKey = 'auto', fallbackNow = 0, anchorEnd = null) {
  const { min, max } = buildBounds(events, fallbackNow);
  const range = AXIS_RANGES.find((item) => item.key === rangeKey);

  if (rangeKey === 'auto') {
    const window = buildAutoWindow(events);
    if (window) {
      return {
        start: Math.max(min, window.start),
        end: Math.min(max, window.end),
      };
    }
    return { start: min, end: max };
  }

  if (!range?.ms) return { start: min, end: max };

  const span = Math.min(range.ms, max - min);
  let end = anchorEnd == null ? max : Math.min(Math.max(anchorEnd, min + span), max);
  let start = end - span;
  if (start < min) { start = min; end = min + span; }
  return { start, end };
}

/** 刻度标签格式：跨度 <1 天显示时分，<30 天显示月日，否则显示年月。 */
export function formatTick(value, step, timeZone = 'Asia/Shanghai') {
  const parts = new Intl.DateTimeFormat('zh-CN', {
    timeZone,
    hour12: false,
    ...(step < DAY_MS
      ? { month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit' }
      : step < 30 * DAY_MS
        ? { month: '2-digit', day: '2-digit' }
        : { year: 'numeric', month: '2-digit' }),
  }).formatToParts(new Date(value));
  const get = (type) => parts.find((part) => part.type === type)?.value ?? '';
  const pad = (value) => String(value).padStart(2, '0');
  if (step < DAY_MS) return `${pad(get('month'))}-${pad(get('day'))} ${pad(get('hour'))}:${pad(get('minute'))}`;
  if (step < 30 * DAY_MS) return `${pad(get('month'))}-${pad(get('day'))}`;
  return `${get('year')}-${pad(get('month'))}`;
}

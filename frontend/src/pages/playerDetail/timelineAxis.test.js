import test from 'node:test';
import assert from 'node:assert/strict';
import {
  AXIS_DOT_MIN_GAP,
  AXIS_LANE_ROW_HEIGHT,
  DAY_MS,
  HOUR_MS,
  axisTotalHeight,
  buildAutoWindow,
  buildBounds,
  buildDomain,
  buildLanes,
  buildSingleLane,
  buildTicks,
  formatTick,
  pickTickStep,
} from './timelineAxis.js';

const ev = (occurred_at, category = 'session', extra = {}) => ({
  event_type: `${category}_x`,
  category,
  title: 't',
  occurred_at,
  ...extra,
});

test('pickTickStep 保证刻度间距不小于最小像素宽度', () => {
  const pxPerMs = 1 / (DAY_MS * 2); // 两天占 1px
  const step = pickTickStep(pxPerMs);
  assert.ok(step * pxPerMs >= 84);
});

test('buildTicks 生成落在区间内的等距刻度', () => {
  const start = Date.UTC(2026, 0, 1, 0, 0, 0);
  const end = start + 10 * DAY_MS;
  const pxPerMs = 1 / HOUR_MS; // 每小时 1px
  const { step, items } = buildTicks(start, end, pxPerMs);
  assert.ok(step >= 3 * HOUR_MS);
  assert.ok(items.length > 0);
  assert.ok(items[0] >= start && items[items.length - 1] <= end);
  for (let i = 1; i < items.length; i += 1) assert.equal(items[i] - items[i - 1], step);
});

test('buildTicks 对非法区间返回空刻度', () => {
  assert.deepEqual(buildTicks(Number.NaN, 10, 1).items, []);
  assert.deepEqual(buildTicks(20, 20, 1).items, []);
});

test('buildLanes 按分类分组并按时间升序排列', () => {
  const start = Date.UTC(2026, 0, 1);
  const events = [
    ev(start + 3 * HOUR_MS, 'session'),
    ev(start + 1 * HOUR_MS, 'ban'),
    ev(start + 2 * HOUR_MS, 'session'),
  ];
  const lanes = buildLanes(events, { start, end: start + DAY_MS, pxPerMs: 1 / HOUR_MS, order: ['session', 'ban'] });
  assert.deepEqual(lanes.map((lane) => lane.category), ['session', 'ban']);
  // 未指定顺序时按分类名排序，保证渲染稳定
  const unordered = buildLanes(events, { start, end: start + DAY_MS, pxPerMs: 1 / HOUR_MS });
  assert.deepEqual(unordered.map((lane) => lane.category), ['ban', 'session']);
  assert.deepEqual(lanes[0].items.map((item) => Math.round(item.x)), [2, 3]);
});

test('buildLanes / buildSingleLane 支持两端留白（insetX）', () => {
  const events = [
    ev(new Date(1000).toISOString()),
    ev(new Date(2000).toISOString()),
  ];
  const lanes = buildLanes(events, { start: 1000, pxPerMs: 1, insetX: 18 });
  assert.equal(lanes[0].items[0].x, 18);
  assert.equal(lanes[0].items[1].x, 1018);
  const single = buildSingleLane(events, { start: 1000, pxPerMs: 1, insetX: 18 });
  assert.equal(single.items[0].x, 18);
  assert.equal(single.items[1].x, 1018);
});

test('buildLanes 对时间过近的点换行避免重叠', () => {
  const start = 0;
  const events = [
    ev(new Date(1000).toISOString(), 'session'),
    ev(new Date(1010).toISOString(), 'session'),
    ev(new Date(1100).toISOString(), 'session'),
  ];
  const lanes = buildLanes(events, { start, end: 2000, pxPerMs: 1, minGap: AXIS_DOT_MIN_GAP });
  const lane = lanes[0];
  assert.equal(lane.items[0].row, 0);
  assert.equal(lane.items[1].row, 1); // 10px < 14px，换行
  assert.equal(lane.items[2].row, 0); // 100px 后回到第一行
  assert.equal(lane.rows, 2);
  assert.equal(axisTotalHeight(lanes), 2 * AXIS_LANE_ROW_HEIGHT + 14);
});

test('buildLanes 忽略非法时间并保留分类顺序', () => {
  const events = [
    ev('not-a-date', 'ban'),
    ev(new Date(DAY_MS).toISOString(), 'ban'),
    ev(new Date(DAY_MS).toISOString(), 'whitelist'),
  ];
  const lanes = buildLanes(events, { start: 0, end: 2 * DAY_MS, pxPerMs: 1 / DAY_MS, order: ['whitelist', 'ban'] });
  assert.deepEqual(lanes.map((lane) => lane.category), ['whitelist', 'ban']);
  assert.equal(lanes[1].items.length, 1);
});

test('buildSingleLane 把所有分类放到同一条轴上并分层避让', () => {
  const lane = buildSingleLane([
    ev(new Date(1000).toISOString(), 'session'),
    ev(new Date(1005).toISOString(), 'ban'),
    ev(new Date(2000).toISOString(), 'qq'),
  ], { start: 0, pxPerMs: 1, minGap: AXIS_DOT_MIN_GAP });
  assert.equal(lane.items.length, 3);
  assert.deepEqual(lane.items.map((item) => item.row), [0, 1, 0]);
  assert.equal(lane.rows, 2);
});

test('buildSingleLane 忽略非法时间', () => {
  const lane = buildSingleLane([ev('bad'), ev(new Date(1000).toISOString())], { start: 0, pxPerMs: 1 });
  assert.equal(lane.items.length, 1);
  assert.equal(lane.rows, 1);
});

test('buildDomain 计算全量区间并在极窄区间上补足余量', () => {
  const events = [ev(new Date(1000).toISOString()), ev(new Date(2000).toISOString())];
  const domain = buildDomain(events, 'all', 0);
  assert.ok(domain.end - domain.start >= HOUR_MS);
});

test('buildDomain 按范围裁剪并限制不超出最早事件', () => {
  const now = Date.UTC(2026, 5, 1);
  const events = [ev(new Date(now - 400 * DAY_MS).toISOString()), ev(new Date(now).toISOString())];
  const domain = buildDomain(events, '30d', now);
  assert.equal(domain.end, now);
  assert.equal(domain.start, now - 30 * DAY_MS);
  // 单条事件时区间会向两侧各扩展 1 小时，再按 30 天范围裁剪
  const clipped = buildDomain([ev(new Date(now - 3 * DAY_MS).toISOString())], '30d', now);
  assert.equal(clipped.start, now - 3 * DAY_MS - HOUR_MS);
  assert.equal(clipped.end, now - 3 * DAY_MS + HOUR_MS);
  assert.ok(clipped.start >= now - 30 * DAY_MS);
});

test('buildDomain 无事件时使用兜底时间基准', () => {
  const fallback = Date.UTC(2026, 0, 10);
  const domain = buildDomain([], 'all', fallback);
  assert.equal(domain.end, fallback);
  assert.equal(domain.end - domain.start, DAY_MS);
});

test('buildBounds 返回所有事件的最小/最大时间并处理空数据', () => {
  const a = Date.UTC(2026, 0, 1);
  const b = Date.UTC(2026, 2, 1);
  const bounds = buildBounds([ev(new Date(a).toISOString()), ev(new Date(b).toISOString())], 0);
  assert.equal(bounds.min, a);
  assert.equal(bounds.max, b);
  const empty = buildBounds([], a);
  assert.equal(empty.max, a);
  assert.equal(empty.min, a - DAY_MS);
});

test('buildAutoWindow 跳过最大空档、聚焦最近活跃期', () => {
  const old = Date.UTC(2021, 0, 1);
  const recent = Date.UTC(2026, 8, 1);
  const events = [
    ev(new Date(old).toISOString()),
    ev(new Date(old + 2 * DAY_MS).toISOString()),
    ev(new Date(recent).toISOString()),
    ev(new Date(recent + 10 * DAY_MS).toISOString()),
  ];
  const window = buildAutoWindow(events);
  assert.ok(window);
  assert.ok(window.start > old + 30 * DAY_MS);
  assert.ok(window.end >= recent + 10 * DAY_MS);
});

test('buildAutoWindow 对集中事件或单事件返回 null', () => {
  const base = Date.UTC(2026, 0, 1);
  assert.equal(buildAutoWindow([ev(new Date(base).toISOString())]), null);
  assert.equal(buildAutoWindow([
    ev(new Date(base).toISOString()),
    ev(new Date(base + 30 * DAY_MS).toISOString()),
  ]), null);
});

test('buildDomain auto 模式聚焦最近活跃期，all 模式返回全量', () => {
  const old = Date.UTC(2021, 0, 1);
  const recent = Date.UTC(2026, 8, 1);
  const events = [
    ev(new Date(old).toISOString()),
    ev(new Date(old + 2 * DAY_MS).toISOString()),
    ev(new Date(recent).toISOString()),
    ev(new Date(recent + 10 * DAY_MS).toISOString()),
  ];
  const auto = buildDomain(events, 'auto', recent);
  const all = buildDomain(events, 'all', recent);
  assert.ok(auto.start > old);
  assert.equal(all.start, old);
});

test('buildDomain 预设范围支持 anchorEnd 平移并钳制在边界内', () => {
  const now = Date.UTC(2026, 5, 1);
  const events = [ev(new Date(now - 100 * DAY_MS).toISOString()), ev(new Date(now).toISOString())];
  const panned = buildDomain(events, '30d', now, now - 20 * DAY_MS);
  assert.equal(panned.end, now - 20 * DAY_MS);
  assert.equal(panned.start, now - 50 * DAY_MS);
  // 超出最早事件时钳制
  const clamped = buildDomain(events, '30d', now, now - 365 * DAY_MS);
  assert.equal(clamped.start, now - 100 * DAY_MS);
  assert.equal(clamped.end, now - 70 * DAY_MS);
});

test('formatTick 按刻度跨度选择精度', () => {
  const value = Date.UTC(2026, 2, 5, 4, 30, 0);
  assert.match(formatTick(value, HOUR_MS), /^03-05 12:30$/); // UTC+8
  assert.match(formatTick(value, DAY_MS), /^03-05$/);
  assert.match(formatTick(value, 90 * DAY_MS), /^2026-03$/);
});

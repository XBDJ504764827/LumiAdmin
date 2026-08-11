import test from 'node:test';
import assert from 'node:assert/strict';
import {
  normalizeAdminPreviewRows,
  formatNumber,
  percentChange,
  formatPercentChange,
  fillWhitelistTrendWindow,
  formatActivityLabel,
  buildServerActivityData,
  buildServerStatusData,
  SERVER_STATUS_META,
  buildServerRankingData,
  formatPlaytime,
} from './dashboardData.js';

test('normalizeAdminPreviewRows maps backend admin preview rows', () => {
  const rows = normalizeAdminPreviewRows([
    { display_name: 'Alex', role: 'admin', role_label: '系统管理员', status: '可用' },
    { display_name: 'DevAdmin', role: 'developer', role_label: '开发管理员', status: '可用' },
    { display_name: 'James', role: 'normal', role_label: '普通管理员', status: '可用' },
  ]);

  assert.deepEqual(rows, [
    { displayName: 'Alex', role: 'admin', roleLabel: '系统管理员', status: '可用', initials: 'A' },
    { displayName: 'DevAdmin', role: 'developer', roleLabel: '开发管理员', status: '可用', initials: 'D' },
    { displayName: 'James', role: 'normal', roleLabel: '普通管理员', status: '可用', initials: 'J' },
  ]);
});

test('normalizeAdminPreviewRows does not provide mock fallback rows', () => {
  assert.deepEqual(normalizeAdminPreviewRows(), []);
});

// ── 统计卡片 ──

test('formatNumber adds thousands separators and handles missing values', () => {
  assert.equal(formatNumber(1284), '1,284');
  assert.equal(formatNumber(0), '0');
  assert.equal(formatNumber(undefined), '0');
  assert.equal(formatNumber(null), '0');
});

test('percentChange computes rounded day-over-day change', () => {
  assert.equal(percentChange(115, 100), 15);
  assert.equal(percentChange(50, 100), -50);
  assert.equal(percentChange(105, 100), 5);
  assert.equal(percentChange(103, 100), 3);
});

test('percentChange returns null when previous period is empty', () => {
  assert.equal(percentChange(5, 0), null);
  assert.equal(percentChange(0, 0), null);
  assert.equal(percentChange(3, undefined), null);
});

test('formatPercentChange adds sign and suffix', () => {
  assert.equal(formatPercentChange(12.5), '+12.5%');
  assert.equal(formatPercentChange(-3), '-3%');
  assert.equal(formatPercentChange(0), '0%');
  assert.equal(formatPercentChange(null), null);
});

// ── 白名单趋势 ──

test('fillWhitelistTrendWindow fills missing days with zero and keeps window size', () => {
  // 固定“今天”= 2026-08-11，窗口 7 天：08-05 ~ 08-11
  const items = [
    { date: '2026-08-11', count: 3 },
    { date: '2026-08-08', count: 2 },
  ];
  const { labels, counts } = fillWhitelistTrendWindow(items, 7, '2026-08-11');

  assert.deepEqual(labels, ['08-05', '08-06', '08-07', '08-08', '08-09', '08-10', '08-11']);
  assert.deepEqual(counts, [0, 0, 0, 2, 0, 0, 3]);
});

test('fillWhitelistTrendWindow ignores dates outside the window', () => {
  const items = [{ date: '2026-07-01', count: 9 }];
  const { counts } = fillWhitelistTrendWindow(items, 7, '2026-08-11');
  assert.deepEqual(counts, [0, 0, 0, 0, 0, 0, 0]);
});

test('fillWhitelistTrendWindow handles empty input', () => {
  const { labels, counts } = fillWhitelistTrendWindow([], 30, '2026-08-11');
  assert.equal(labels.length, 30);
  assert.equal(counts.length, 30);
  assert.ok(counts.every((count) => count === 0));
});

// ── 服务器活跃度 ──

test('formatActivityLabel renders hour and day labels in Asia/Shanghai', () => {
  // 2026-08-11T06:00:00Z = 14:00 北京时间
  assert.equal(formatActivityLabel('2026-08-11T06:00:00Z', 'hour'), '14:00');
  assert.equal(formatActivityLabel('2026-08-11T06:00:00Z', 'day'), '08-11');
  assert.equal(formatActivityLabel(null, 'hour'), '-');
});

test('buildServerActivityData maps backend buckets to chart datasets', () => {
  const items = [
    { time: '2026-08-11T06:00:00Z', active_players: 4, sessions: 7 },
    { time: '2026-08-11T07:00:00Z', active_players: 2, sessions: 3 },
  ];
  const data = buildServerActivityData(items, 'hour');

  assert.deepEqual(data.labels, ['14:00', '15:00']);
  assert.deepEqual(data.activePlayers, [4, 2]);
  assert.deepEqual(data.sessions, [7, 3]);
});

// ── 服务器状态分布 ──

test('SERVER_STATUS_META covers all backend statuses', () => {
  assert.deepEqual(Object.keys(SERVER_STATUS_META).sort(), ['hibernating', 'offline', 'online', 'untested']);
});

test('buildServerStatusData maps status labels and computes total', () => {
  const data = buildServerStatusData([
    { status: 'online', count: 3 },
    { status: 'hibernating', count: 1 },
    { status: 'offline', count: 2 },
  ]);

  assert.deepEqual(data.items, [
    { status: 'online', label: '在线', count: 3 },
    { status: 'hibernating', label: '休眠', count: 1 },
    { status: 'offline', label: '离线', count: 2 },
  ]);
  assert.equal(data.total, 6);
});

test('buildServerStatusData falls back to raw status for unknown values', () => {
  const data = buildServerStatusData([{ status: 'weird', count: 1 }]);
  assert.equal(data.items[0].label, 'weird');
  assert.equal(data.total, 1);
});

// ── 服务器活跃度排行 ──

test('buildServerRankingData maps ranking rows', () => {
  const data = buildServerRankingData([
    { server_name: '排行服A', active_players: 8, sessions: 12, playtime_seconds: 172800 },
    { server_name: '排行服B', active_players: 3, sessions: 4, playtime_seconds: 5400 },
  ]);

  assert.deepEqual(data.names, ['排行服A', '排行服B']);
  assert.deepEqual(data.activePlayers, [8, 3]);
  assert.deepEqual(data.sessions, [12, 4]);
  assert.deepEqual(data.playtimeSeconds, [172800, 5400]);
});

test('formatPlaytime renders readable durations', () => {
  assert.equal(formatPlaytime(172800), '2 天');
  assert.equal(formatPlaytime(5400), '1.5 小时');
  assert.equal(formatPlaytime(300), '5 分钟');
  assert.equal(formatPlaytime(0), '-');
  assert.equal(formatPlaytime(undefined), '-');
});

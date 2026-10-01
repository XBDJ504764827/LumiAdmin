import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  HOST_AGENT_RANGES,
  mockHeartbeatPoints,
  mockPowerTaskPoints,
} from './hostAgentCharts.js';

test('同 range 多次生成结果一致（渲染稳定）', () => {
  assert.deepEqual(mockHeartbeatPoints('7d'), mockHeartbeatPoints('7d'));
  assert.deepEqual(mockPowerTaskPoints('24h'), mockPowerTaskPoints('24h'));
});

test('点数与标签数一致：24h=24 点，7d=7 点，30d=30 点', () => {
  for (const range of ['24h', '7d', '30d']) {
    const heartbeat = mockHeartbeatPoints(range);
    const tasks = mockPowerTaskPoints(range);
    const count = range === '24h' ? 24 : range === '7d' ? 7 : 30;
    assert.equal(heartbeat.labels.length, count);
    assert.equal(heartbeat.online.length, count);
    assert.equal(tasks.labels.length, count);
    assert.equal(tasks.restart.length, count);
  }
});

test('心跳在线数在演示规模内，心跳总数为正', () => {
  const { online, heartbeats } = mockHeartbeatPoints('24h');
  assert.ok(online.every((v) => v === 1 || v === 2));
  assert.ok(heartbeats.every((v) => v > 0));
  assert.ok(online.some((v) => v === 1), '应含离线波动点以演示在线率变化');
});

test('电源任务为稀疏事件且非负', () => {
  const { restart, start, stop } = mockPowerTaskPoints('7d');
  const total = [...restart, ...start, ...stop].reduce((a, b) => a + b, 0);
  assert.ok(total > 0 && total < 7 * 3, `实际总数 ${total}`);
});

test('范围切换器含 24 小时/7 天/30 天', () => {
  assert.deepEqual(HOST_AGENT_RANGES.map((r) => r.value), ['24h', '7d', '30d']);
});

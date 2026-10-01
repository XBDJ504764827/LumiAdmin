import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  HEARTBEAT_RANGES,
  POWER_TASK_RANGES,
  allZero,
  buildHeartbeatChartData,
  buildPowerTaskChartData,
  summarize,
} from './hostAgentTrend.js';

test('心跳构造器映射桶数据并容错空值', () => {
  const data = buildHeartbeatChartData([
    { bucket: '10:00', online: 2, heartbeats: 700 },
    { bucket: '11:00' },
  ]);
  assert.deepEqual(data.labels, ['10:00', '11:00']);
  assert.deepEqual(data.online, [2, 0]);
  assert.deepEqual(data.heartbeats, [700, 0]);
  assert.deepEqual(buildHeartbeatChartData(null).labels, []);
});

test('电源任务构造器映射三动作', () => {
  const data = buildPowerTaskChartData([{ bucket: '09-30', restart: 1, start: 0, stop: 2 }]);
  assert.deepEqual(data.restart, [1]);
  assert.deepEqual(data.start, [0]);
  assert.deepEqual(data.stop, [2]);
});

test('空数据判定与汇总', () => {
  assert.equal(allZero([]), true);
  assert.equal(allZero([0, 0]), true);
  assert.equal(allZero([0, 1]), false);
  assert.deepEqual(summarize([1, 3, 2]), { total: 6, peak: 3 });
});

test('范围切换器档位正确', () => {
  assert.deepEqual(HEARTBEAT_RANGES.map((r) => r.value), ['24h', '7d', '30d']);
  assert.deepEqual(POWER_TASK_RANGES.map((r) => r.value), ['7d', '30d']);
});

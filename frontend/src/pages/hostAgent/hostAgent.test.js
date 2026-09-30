import { test } from 'node:test';
import assert from 'node:assert/strict';
import { MOCK_POWER_LOGS, POWER_LOG_RESULT_META } from './hostAgent.js';

test('演示操作日志字段齐全且动作合法', () => {
  const actions = ['重启服务器', '强制重启服务器', '开启服务器', '关闭服务器'];
  assert.ok(MOCK_POWER_LOGS.length >= 4);
  for (const log of MOCK_POWER_LOGS) {
    assert.ok(log.id && log.operator && log.target && log.ip && log.time);
    assert.equal(log.module, '服务器控制');
    assert.ok(actions.includes(log.action), `未知动作：${log.action}`);
    assert.ok(POWER_LOG_RESULT_META[log.result], `未知结果：${log.result}`);
  }
});

test('演示日志覆盖成功与拦截两种结果', () => {
  const results = new Set(MOCK_POWER_LOGS.map((log) => log.result));
  assert.ok(results.has('success'));
  assert.ok(results.has('blocked'));
});

import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  POWER_COUNTDOWN_SECS,
  buildStartConfirmText,
  buildStopConfirmText,
  countdownLabel,
  isPoweredOn,
  isPowerJobTerminal,
  powerAvailability,
  restartVariant,
  POWER_ACTION_LABEL,
  POWER_JOB_STATUS_TEXT,
  RESTART_EMPTY_TEXT,
  RESTART_FINAL_TEXT,
  RESTART_HAS_PLAYERS_TEXT,
  FORCE_RESTART_CONFIRM_TEXT,
  FORCE_RESTART_FINAL_TEXT,
  serverPlayerCount,
} from './communityPower.js';

test('serverPlayerCount 兼容 online_player_count 与 players 数组', () => {
  assert.equal(serverPlayerCount({ online_player_count: 3 }), 3);
  assert.equal(serverPlayerCount({ players: ['a', 'b'] }), 2);
  assert.equal(serverPlayerCount({}), 0);
  assert.equal(serverPlayerCount(null), 0);
});

test('isPoweredOn 在线与休眠视为开机，其余视为关机', () => {
  assert.equal(isPoweredOn({ status: 'online' }), true);
  assert.equal(isPoweredOn({ status: 'hibernating' }), true);
  assert.equal(isPoweredOn({ status: 'offline' }), false);
  assert.equal(isPoweredOn({ status: 'untested' }), false);
  assert.equal(isPoweredOn(null), false);
});

test('restartVariant 按服内人数区分两种二次确认', () => {
  assert.equal(restartVariant({ online_player_count: 2 }), 'has-players');
  assert.equal(restartVariant({ players: [] }), 'empty');
  assert.ok(RESTART_HAS_PLAYERS_TEXT.includes('存在玩家无法进行重启'));
  assert.ok(RESTART_EMPTY_TEXT.includes('确认服务器内没有玩家后再继续下一步'));
  assert.ok(RESTART_FINAL_TEXT.includes('强制阅读 5 秒后才可执行'));
  assert.ok(FORCE_RESTART_CONFIRM_TEXT.includes('无视服内玩家'));
  assert.ok(FORCE_RESTART_FINAL_TEXT.includes('强制阅读 5 秒后才可执行'));
});

test('powerAvailability 开关机互斥置灰，强制重启常亮', () => {
  const on = powerAvailability({ status: 'online' });
  assert.equal(on.restart.enabled, true);
  assert.equal(on.forceRestart.enabled, true);
  assert.equal(on.start.enabled, false);
  assert.equal(on.stop.enabled, true);

  const off = powerAvailability({ status: 'offline' });
  assert.equal(off.start.enabled, true);
  assert.equal(off.stop.enabled, false);
  assert.ok(off.stop.disabledReason.includes('关机'));
});

test('休眠服允许开启（僵尸状态兜底），强制开通常亮', () => {
  const hibernating = powerAvailability({ status: 'hibernating' });
  assert.equal(hibernating.start.enabled, true);
  assert.equal(hibernating.stop.enabled, true);
  assert.equal(hibernating.forceStart.enabled, true);
  assert.equal(powerAvailability({ status: 'offline' }).forceStart.enabled, true);
});

test('确认文案携带服务器名', () => {
  assert.ok(buildStartConfirmText('1服').includes('1服'));
  assert.ok(buildStopConfirmText('2服').includes('2服'));
});

test('countdownLabel 倒计时结束恢复原文字', () => {
  assert.equal(POWER_COUNTDOWN_SECS, 5);
  assert.equal(countdownLabel('确认重启', 5), '确认重启（5s）');
  assert.equal(countdownLabel('确认重启', 0), '确认重启');
});

test('任务终态判定与文案映射', () => {
  assert.equal(isPowerJobTerminal('success'), true);
  assert.equal(isPowerJobTerminal('failed'), true);
  assert.equal(isPowerJobTerminal('timeout'), true);
  assert.equal(isPowerJobTerminal('pending'), false);
  assert.equal(isPowerJobTerminal('running'), false);
  assert.equal(POWER_ACTION_LABEL['force-restart'], '强制重启服务器');
  assert.equal(POWER_JOB_STATUS_TEXT.success, '执行成功');
});

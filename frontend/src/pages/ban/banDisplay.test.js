import test from 'node:test';
import assert from 'node:assert/strict';
import { formatBanDuration, formatBanSource, formatExpiresAt } from './banDisplay.js';

test('formatBanDuration treats zero as permanent', () => {
  assert.equal(formatBanDuration(0), '永久');
});

test('formatBanDuration formats minutes and hours', () => {
  assert.equal(formatBanDuration(30), '30 分钟');
  assert.equal(formatBanDuration(120), '2 小时');
});

test('formatBanSource maps known ban sources to detailed labels', () => {
  assert.equal(formatBanSource('global_ban'), '全球封禁自动封禁');
  assert.equal(formatBanSource('game_plugin', 'Admin'), '游戏管理员手动封禁');
  assert.equal(formatBanSource('game_plugin'), '游戏管理员手动封禁');
  assert.equal(formatBanSource('manual'), '网站管理员手动封禁');
  assert.equal(formatBanSource('web'), '网站管理员手动封禁');
  assert.equal(formatBanSource('offline_sync', 'Admin'), '游戏离线操作同步封禁');
  assert.equal(formatBanSource('external_api'), '外部封禁 API 同步封禁');
  assert.equal(formatBanSource('custom_source'), '其他来源封禁（custom_source）');
  assert.equal(formatBanSource(null), '未知来源封禁');
});

test('formatExpiresAt handles permanent ban', () => {
  assert.equal(formatExpiresAt(null), '永不过期');
});

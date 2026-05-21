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

test('formatBanSource maps plugin source with operator name', () => {
  assert.equal(formatBanSource('game_plugin', 'Admin'), '游戏内 - Admin');
  assert.equal(formatBanSource('game_plugin'), '游戏内命令');
  assert.equal(formatBanSource('manual'), '网站手动');
});

test('formatExpiresAt handles permanent ban', () => {
  assert.equal(formatExpiresAt(null), '永不过期');
});

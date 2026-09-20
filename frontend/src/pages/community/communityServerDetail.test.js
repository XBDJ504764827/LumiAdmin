import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  buildAuthSyncSummary,
  buildPlayerSummary,
  formatAuthSyncTime,
  maskReportToken,
} from './communityServerDetail.js';

test('maskReportToken keeps only the last 4 characters', () => {
  assert.equal(maskReportToken('abcdefghijkl'), '••••••••ijkl');
  assert.equal(maskReportToken('abcd'), '••••');
  assert.equal(maskReportToken(''), '••••••••••••');
  assert.equal(maskReportToken(null), '••••••••••••');
});

test('buildAuthSyncSummary derives pending from version gap', () => {
  const summary = buildAuthSyncSummary({
    auth_connection: 'connected',
    auth_version: 10,
    auth_latest_version: 13,
  });
  assert.equal(summary.connected, true);
  assert.equal(summary.pending, 3);
  assert.equal(summary.lag, true);
});

test('buildAuthSyncSummary trusts explicit pending count', () => {
  const summary = buildAuthSyncSummary({
    auth_connection: 'disconnected',
    auth_version: 5,
    auth_latest_version: 5,
    auth_pending_events: 2,
  });
  assert.equal(summary.connected, false);
  assert.equal(summary.pending, 2);
  assert.equal(summary.lag, true);
});

test('buildAuthSyncSummary is not lagging when up to date', () => {
  const summary = buildAuthSyncSummary({
    auth_connection: 'connected',
    auth_version: 7,
    auth_latest_version: 7,
    auth_pending_events: 0,
  });
  assert.equal(summary.lag, false);
});

test('formatAuthSyncTime renders relative and missing values', () => {
  const now = Date.UTC(2026, 8, 20, 12, 0, 0);
  assert.equal(formatAuthSyncTime(null, now), '—');
  assert.equal(formatAuthSyncTime('not-a-date', now), '—');
  assert.equal(formatAuthSyncTime(new Date(now - 5_000).toISOString(), now), '5 秒前');
  assert.equal(formatAuthSyncTime(new Date(now - 120_000).toISOString(), now), '2 分钟前');
});

test('buildPlayerSummary falls back to players array length', () => {
  assert.deepEqual(buildPlayerSummary({ players: ['a', 'b'], max_players: 32 }), { online: 2, max: 32 });
  assert.deepEqual(buildPlayerSummary({ online_player_count: 5, max_players: 10 }), { online: 5, max: 10 });
  assert.deepEqual(buildPlayerSummary(null), { online: 0, max: 0 });
});

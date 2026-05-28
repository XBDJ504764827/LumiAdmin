import test from 'node:test';
import assert from 'node:assert/strict';
import {
  formatChinaDate,
  formatChinaDateTime,
  formatChinaMonthDayTime,
  formatChinaToday,
  getChinaHour,
} from './time.js';

test('formats UTC timestamps in China time', () => {
  assert.equal(formatChinaDateTime('2026-05-28T00:05:06Z'), '2026-05-28 08:05:06');
  assert.equal(formatChinaDateTime('2026-05-28T00:05:06Z', { seconds: false }), '2026-05-28 08:05');
});

test('formats compact China date variants', () => {
  assert.equal(formatChinaDate('2026-12-31T16:30:00Z'), '2027-01-01');
  assert.equal(formatChinaMonthDayTime('2026-12-31T16:30:00Z'), '01-01 00:30');
  assert.equal(formatChinaToday(new Date('2026-12-31T16:30:00Z')), '2027年1月1日');
  assert.equal(getChinaHour(new Date('2026-12-31T16:30:00Z')), 0);
});

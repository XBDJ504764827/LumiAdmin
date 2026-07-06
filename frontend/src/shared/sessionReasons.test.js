import test from 'node:test';
import assert from 'node:assert/strict';
import {
  eventSourceLabel,
  sessionReasonKind,
  sessionReasonLabel,
} from './sessionReasons.js';

test('session reason helpers format known disconnect reasons', () => {
  assert.equal(sessionReasonLabel('admin_kicked'), '管理员踢出');
  assert.equal(eventSourceLabel('banned_kicked'), '封禁踢出');
  assert.equal(sessionReasonKind('access_rejected'), 'danger');
  assert.equal(sessionReasonKind('player_quit'), 'success');
});

test('session reason helpers preserve unknown backend values', () => {
  assert.equal(sessionReasonLabel('custom_reason'), 'custom_reason');
  assert.equal(eventSourceLabel(''), '-');
  assert.equal(sessionReasonKind('custom_reason'), 'default');
});

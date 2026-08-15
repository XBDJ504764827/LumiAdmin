import test from 'node:test';
import assert from 'node:assert/strict';
import { buildCreateUserPayload, buildUpdateUserPayload, validateCreateUserForm } from './userForm.js';

test('buildCreateUserPayload converts empty steamid to null', () => {
  const payload = buildCreateUserPayload({
    username: 'alex',
    password: 'secret',
    role: 'normal',
    steam_id: '   ',
    remark: 'note',
    openid: '12345',
  });

  assert.equal(payload.steam_id, null);
  assert.equal(payload.remark, 'note');
  assert.equal(payload.openid, '12345');
});

test('buildCreateUserPayload converts empty openid to null', () => {
  const payload = buildCreateUserPayload({
    username: 'alex',
    password: 'secret',
    role: 'normal',
    steam_id: '',
    remark: '',
    openid: '   ',
  });

  assert.equal(payload.openid, null);
});

test('buildUpdateUserPayload converts empty steamid to null', () => {
  const payload = buildUpdateUserPayload({
    username: 'alex',
    role: 'admin',
    steam_id: '',
    remark: '',
    openid: '',
  }, true);

  assert.equal(payload.steam_id, null);
  assert.equal(payload.remark, null);
  assert.equal(payload.openid, null);
});

test('validateCreateUserForm does not require steamid', () => {
  assert.equal(validateCreateUserForm({ username: 'alex', password: 'secret', steam_id: '' }), '');
});

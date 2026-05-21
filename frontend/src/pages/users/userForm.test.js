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
  });

  assert.equal(payload.steam_id, null);
  assert.equal(payload.remark, 'note');
});

test('buildUpdateUserPayload converts empty steamid to null', () => {
  const payload = buildUpdateUserPayload({
    username: 'alex',
    role: 'admin',
    steam_id: '',
    remark: '',
  }, true);

  assert.equal(payload.steam_id, null);
  assert.equal(payload.remark, null);
});

test('validateCreateUserForm does not require steamid', () => {
  assert.equal(validateCreateUserForm({ username: 'alex', password: 'secret', steam_id: '' }), '');
});

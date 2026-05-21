import test from 'node:test';
import assert from 'node:assert/strict';
import { readStoredToken, normalizeSession, apiErrorMessage, defaultSessionFromToken } from './authClient.js';

test('readStoredToken returns null when storage has no token', () => {
  const storage = {
    getItem() {
      return null;
    },
  };

  assert.equal(readStoredToken(storage), null);
});

test('defaultSessionFromToken returns null when token is missing', () => {
  assert.equal(defaultSessionFromToken(null), null);
});

test('defaultSessionFromToken builds pending session only when token exists', () => {
  assert.deepEqual(defaultSessionFromToken('abc'), {
    token: 'abc',
    userId: null,
    displayName: '',
    role: 'guest',
    roleLabel: '未登录',
  });
});

test('normalizeSession maps backend session payload to frontend shape', () => {
  const session = normalizeSession({
    session: {
      token: 'abc',
      user_id: 'u-1',
      display_name: 'Alex',
      role: 'admin',
      role_label: '系统管理员',
    },
  });

  assert.deepEqual(session, {
    token: 'abc',
    userId: 'u-1',
    displayName: 'Alex',
    role: 'admin',
    roleLabel: '系统管理员',
  });
});

test('apiErrorMessage returns correct message for each 401 case', () => {
  assert.equal(apiErrorMessage(401, { error: 'invalid credentials' }), '用户名或密码错误。');
  assert.equal(apiErrorMessage(401, {}), '请先登录后再操作。');
});

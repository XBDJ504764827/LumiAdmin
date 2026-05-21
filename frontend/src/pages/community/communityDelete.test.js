import test from 'node:test';
import assert from 'node:assert/strict';
import {
  buildDeleteGroupConfirmMessage,
  buildDeleteGroupFailureMessage,
  buildDeleteGroupSuccessMessage,
} from './communityDelete.js';

test('buildDeleteGroupConfirmMessage omits server warning when group has no servers', () => {
  const message = buildDeleteGroupConfirmMessage({
    name: '空社区组',
    servers: [],
  });

  assert.equal(message, '确定删除社区组“空社区组”吗？');
});

test('buildDeleteGroupConfirmMessage includes server count when group has servers', () => {
  const message = buildDeleteGroupConfirmMessage({
    name: '像素方块',
    servers: [{ id: 's1' }, { id: 's2' }],
  });

  assert.equal(message, '确定删除社区组“像素方块”吗？删除后将同时删除其下 2 个服务器。');
});

test('buildDeleteGroupSuccessMessage reports successful deletion', () => {
  assert.equal(buildDeleteGroupSuccessMessage('像素方块'), '社区组“像素方块”删除成功。');
});

test('buildDeleteGroupFailureMessage reports failed deletion reason', () => {
  assert.equal(buildDeleteGroupFailureMessage('社区组不存在'), '社区组删除失败：社区组不存在');
});


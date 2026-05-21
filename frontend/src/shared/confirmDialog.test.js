import test from 'node:test';
import assert from 'node:assert/strict';
import { createAlertOptions, createConfirmOptions } from './confirmDialog.js';

test('createConfirmOptions builds danger confirmation defaults', () => {
  assert.deepEqual(createConfirmOptions({ message: '确定删除这个游戏服务器吗？' }), {
    title: '确认操作',
    message: '确定删除这个游戏服务器吗？',
    confirmText: '确认删除',
    cancelText: '取消',
    tone: 'danger',
  });
});

test('createConfirmOptions allows custom title and confirm text', () => {
  assert.deepEqual(createConfirmOptions({ title: '重置 Token', message: '确定重置吗？', confirmText: '确认重置' }), {
    title: '重置 Token',
    message: '确定重置吗？',
    confirmText: '确认重置',
    cancelText: '取消',
    tone: 'danger',
  });
});

test('createAlertOptions builds information dialog defaults', () => {
  assert.deepEqual(createAlertOptions({ message: '服务器上报 Token 已复制。' }), {
    title: '提示',
    message: '服务器上报 Token 已复制。',
    confirmText: '知道了',
    tone: 'info',
  });
});

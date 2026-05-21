import test from 'node:test';
import assert from 'node:assert/strict';
import {
  buildReportTokenConfigLine,
  buildResetReportTokenConfirmMessage,
  buildResetReportTokenSuccessMessage,
  canManageServerReportToken,
  normalizeReportTokenResponse,
} from './communityToken.js';

test('buildReportTokenConfigLine formats token as plugin config line', () => {
  assert.equal(
    buildReportTokenConfigLine('abc123'),
    'manger_report_token "abc123"',
  );
});

test('canManageServerReportToken allows admin and developer only', () => {
  assert.equal(canManageServerReportToken('admin'), true);
  assert.equal(canManageServerReportToken('developer'), true);
  assert.equal(canManageServerReportToken('normal'), false);
  assert.equal(canManageServerReportToken(null), false);
});

test('normalizeReportTokenResponse extracts token string', () => {
  assert.equal(
    normalizeReportTokenResponse({ token: { report_token: 'abc123' } }),
    'abc123',
  );
});

test('normalizeReportTokenResponse rejects missing token', () => {
  assert.throws(
    () => normalizeReportTokenResponse({ token: {} }),
    /服务器上报 Token 返回为空/,
  );
});

test('buildResetReportTokenConfirmMessage warns about plugin config invalidation', () => {
  assert.equal(
    buildResetReportTokenConfirmMessage('一号服'),
    '确定重置“一号服”的上报 Token 吗？旧 Token 会立即失效，需要更新插件配置文件。',
  );
});

test('buildResetReportTokenSuccessMessage includes next action', () => {
  assert.equal(
    buildResetReportTokenSuccessMessage('一号服'),
    '一号服 的上报 Token 已重置，请复制新 Token 并写入插件配置文件。',
  );
});

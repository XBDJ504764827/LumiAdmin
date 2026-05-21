import test from 'node:test';
import assert from 'node:assert/strict';
import { normalizeAdminPreviewRows } from './dashboardData.js';

test('normalizeAdminPreviewRows maps backend admin preview rows', () => {
  const rows = normalizeAdminPreviewRows([
    { display_name: 'Alex', role: 'admin', role_label: '系统管理员', status: '可用' },
    { display_name: 'DevAdmin', role: 'developer', role_label: '开发管理员', status: '可用' },
    { display_name: 'James', role: 'normal', role_label: '普通管理员', status: '可用' },
  ]);

  assert.deepEqual(rows, [
    { displayName: 'Alex', role: 'admin', roleLabel: '系统管理员', status: '可用', initials: 'A' },
    { displayName: 'DevAdmin', role: 'developer', roleLabel: '开发管理员', status: '可用', initials: 'D' },
    { displayName: 'James', role: 'normal', roleLabel: '普通管理员', status: '可用', initials: 'J' },
  ]);
});

test('normalizeAdminPreviewRows does not provide mock fallback rows', () => {
  assert.deepEqual(normalizeAdminPreviewRows(), []);
});

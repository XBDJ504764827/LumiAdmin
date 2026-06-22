import test from 'node:test';
import assert from 'node:assert/strict';
import { ADMIN_ROLES, PUBLIC_ROLES, ROLE, ROUTE_ROLES, STAFF_ROLES } from './roles.js';

test('admin roles match backend privileged roles', () => {
  assert.deepEqual(ADMIN_ROLES, [ROLE.admin, ROLE.developer]);
  assert.equal(ADMIN_ROLES.includes(ROLE.normal), false);
});

test('staff and public role groups are explicit supersets', () => {
  assert.deepEqual(STAFF_ROLES, [ROLE.admin, ROLE.developer, ROLE.normal]);
  assert.deepEqual(PUBLIC_ROLES, [ROLE.guest, ROLE.admin, ROLE.developer, ROLE.normal]);
  assert.equal(ROUTE_ROLES.admin, ADMIN_ROLES);
  assert.equal(ROUTE_ROLES.staff, STAFF_ROLES);
  assert.equal(ROUTE_ROLES.public, PUBLIC_ROLES);
});

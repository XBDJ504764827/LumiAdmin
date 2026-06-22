export const ROLE = Object.freeze({
  developer: 'developer',
  admin: 'admin',
  normal: 'normal',
  guest: 'guest',
});

export const ADMIN_ROLES = Object.freeze([ROLE.admin, ROLE.developer]);
export const STAFF_ROLES = Object.freeze([ROLE.admin, ROLE.developer, ROLE.normal]);
export const PUBLIC_ROLES = Object.freeze([ROLE.guest, ...STAFF_ROLES]);

export const ROUTE_ROLES = Object.freeze({
  admin: ADMIN_ROLES,
  staff: STAFF_ROLES,
  public: PUBLIC_ROLES,
});

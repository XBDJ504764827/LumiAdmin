import { ROLE } from '../routes/roles.js';

export function hasPermission(session, permission) {
  if (!session) return false;
  if (session.permissions?.includes(permission)) return true;
  // 兼容旧的登录响应：未返回 permissions 时仍保持原有角色行为。
  if (session.role === ROLE.developer) return true;
  if (session.role === ROLE.admin) {
    return !['users.manage', 'users.role.manage', 'rcon.unrestricted', 'system.manage'].includes(permission);
  }
  if (session.role === ROLE.normal) {
    return ['whitelist.view', 'whitelist.review', 'ban.view', 'audit.view', 'community.view', 'community.rcon', 'users.view_self', 'player_internal.view'].includes(permission);
  }
  return false;
}

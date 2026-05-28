import { apiErrorMessage } from '../shared/authClient.js';

const API_BASE = import.meta.env.VITE_API_BASE ?? '';

function buildQueryString(params = {}) {
  const entries = Object.entries(params).filter(([, v]) => v !== undefined && v !== null && v !== '');
  return entries.length > 0 ? '?' + entries.map(([k, v]) => `${encodeURIComponent(k)}=${encodeURIComponent(v)}`).join('&') : '';
}

function handleUnauthorized() {
  if (typeof window !== 'undefined') {
    window.localStorage.removeItem('manger_token');
    window.dispatchEvent(new CustomEvent('manger:unauthorized'));
  }
}

async function request(path, options = {}) {
  const { headers: optionHeaders, ...restOptions } = options;
  const isFormData = typeof FormData !== 'undefined' && restOptions.body instanceof FormData;

  const response = await fetch(`${API_BASE}${path}`, {
    cache: 'no-store',
    ...restOptions,
    headers: {
      ...(isFormData ? {} : { 'Content-Type': 'application/json' }),
      ...(optionHeaders ?? {}),
    },
  });

  if (response.status === 401 && !path.startsWith('/api/public/')) {
    handleUnauthorized();
    throw new Error('登录已过期，请重新登录。');
  }

  if (!response.ok) {
    const payload = await response.json().catch(() => ({}));
    throw new Error(apiErrorMessage(response.status, payload));
  }

  if (response.status === 204) {
    return null;
  }

  return response.json();
}

function withAuth(token) {
  return token ? { Authorization: `Bearer ${token}` } : {};
}

export const api = {
  health: () => request('/health'),
  login: (body) => request('/api/auth/login', { method: 'POST', body: JSON.stringify(body) }),
  logout: (token) => request('/api/auth/logout', { method: 'POST', headers: withAuth(token) }),
  logoutAllDevices: (currentToken) => request('/api/auth/logout-all', { method: 'POST', body: JSON.stringify({ current_token: currentToken }) }),
  me: (token) => request('/api/auth/me', { headers: withAuth(token) }),
  dashboard: (token) => request('/api/dashboard', { headers: withAuth(token) }),
  servers: (token) => request('/api/community/servers', { headers: withAuth(token) }),
  createCommunityGroup: (token, body) => request('/api/community/groups', { method: 'POST', headers: withAuth(token), body: JSON.stringify(body) }),
  deleteCommunityGroup: (token, groupId) => request(`/api/community/groups/${groupId}`, { method: 'DELETE', headers: withAuth(token) }),
  updateCommunityAccess: (token, groupId, body) => request(`/api/community/groups/${groupId}/access`, { method: 'PUT', headers: withAuth(token), body: JSON.stringify(body) }),
  createCommunityServer: (token, groupId, body) => request(`/api/community/groups/${groupId}/servers`, { method: 'POST', headers: withAuth(token), body: JSON.stringify(body) }),
  updateCommunityServer: (token, serverId, body) => request(`/api/community/servers/${serverId}`, { method: 'PUT', headers: withAuth(token), body: JSON.stringify(body) }),
  deleteCommunityServer: (token, serverId) => request(`/api/community/servers/${serverId}`, { method: 'DELETE', headers: withAuth(token) }),
  testCommunityServerRcon: (token, body) => request('/api/community/servers/test-rcon', { method: 'POST', headers: withAuth(token), body: JSON.stringify(body) }),
  communityServerPlayers: (token, serverId) => request(`/api/community/servers/${serverId}/players`, { headers: withAuth(token) }),
  playerApiPlayers: (token) => request('/api/player-api/players', { headers: withAuth(token) }),
  playerApiConfig: (token) => request('/api/player-api/config', { headers: withAuth(token) }),
  updatePlayerApiConfig: (token, body) => request('/api/player-api/config', { method: 'PUT', headers: withAuth(token), body: JSON.stringify(body) }),
  externalServers: (token) => request('/api/external-servers', { headers: withAuth(token) }),
  createExternalServer: (token, body) => request('/api/external-servers', { method: 'POST', headers: withAuth(token), body: JSON.stringify(body) }),
  updateExternalServer: (token, id, body) => request(`/api/external-servers/${id}`, { method: 'PUT', headers: withAuth(token), body: JSON.stringify(body) }),
  deleteExternalServer: (token, id) => request(`/api/external-servers/${id}`, { method: 'DELETE', headers: withAuth(token) }),
  testExternalServer: (token, id) => request(`/api/external-servers/${id}/test`, { method: 'POST', headers: withAuth(token), body: JSON.stringify({}) }),
  externalBanApiTargets: (token) => request('/api/external-ban-api/targets', { headers: withAuth(token) }),
  createExternalBanApiTarget: (token, body) => request('/api/external-ban-api/targets', { method: 'POST', headers: withAuth(token), body: JSON.stringify(body) }),
  updateExternalBanApiTarget: (token, id, body) => request(`/api/external-ban-api/targets/${id}`, { method: 'PUT', headers: withAuth(token), body: JSON.stringify(body) }),
  deleteExternalBanApiTarget: (token, id) => request(`/api/external-ban-api/targets/${id}`, { method: 'DELETE', headers: withAuth(token) }),
  testExternalBanApiTarget: (token, id) => request(`/api/external-ban-api/targets/${id}/test`, { method: 'POST', headers: withAuth(token), body: JSON.stringify({}) }),
  serverReportToken: (token, serverId) => request(`/api/community/servers/${serverId}/report-token`, { headers: withAuth(token) }),
  resetServerReportToken: (token, serverId) => request(`/api/community/servers/${serverId}/report-token/reset`, { method: 'POST', headers: withAuth(token), body: JSON.stringify({}) }),
  executeRcon: (token, serverId, body) => request(`/api/community/servers/${serverId}/rcon`, { method: 'POST', headers: withAuth(token), body: JSON.stringify(body) }),
  whitelist: (token, params = {}) => request(`/api/whitelist${buildQueryString(params)}`, { headers: withAuth(token) }),
  createManualWhitelist: (token, body) => request('/api/whitelist/manual', { method: 'POST', headers: withAuth(token), body: JSON.stringify(body) }),
  approveWhitelist: (token, id, body = {}) => request(`/api/whitelist/${id}/approve`, { method: 'POST', headers: withAuth(token), body: JSON.stringify(body) }),
  rejectWhitelist: (token, id, body) => request(`/api/whitelist/${id}/reject`, { method: 'POST', headers: withAuth(token), body: JSON.stringify(body) }),
  restoreWhitelist: (token, id, body = {}) => request(`/api/whitelist/${id}/restore`, { method: 'POST', headers: withAuth(token), body: JSON.stringify(body) }),
  revokeWhitelist: (token, id, body = {}) => request(`/api/whitelist/${id}/revoke`, { method: 'POST', headers: withAuth(token), body: JSON.stringify(body) }),
  refreshSingleSteamName: (token, id) => request(`/api/whitelist/${id}/refresh-steam-name`, { method: 'POST', headers: withAuth(token), body: JSON.stringify({}) }),
  refreshAllSteamNames: (token, status = null) => {
    const body = status ? { status } : {};
    return request('/api/whitelist/refresh-steam-names', { method: 'POST', headers: withAuth(token), body: JSON.stringify(body) });
  },
  bans: (token, params = {}) => request(`/api/bans${buildQueryString(params)}`, { headers: withAuth(token) }),
  getBan: (token, id) => request(`/api/bans/${id}`, { headers: withAuth(token) }),
  createBan: (token, body) => request('/api/bans', { method: 'POST', headers: withAuth(token), body: JSON.stringify(body) }),
  updateBan: (token, id, body) => request(`/api/bans/${id}`, { method: 'PUT', headers: withAuth(token), body: JSON.stringify(body) }),
  deleteBan: (token, id) => request(`/api/bans/${id}`, { method: 'DELETE', headers: withAuth(token) }),
  unban: (token, id) => request(`/api/bans/${id}/unban`, { method: 'POST', headers: withAuth(token), body: JSON.stringify({}) }),
  syncExternalBan: (token, id) => request(`/api/bans/${id}/sync-external`, { method: 'POST', headers: withAuth(token), body: JSON.stringify({}) }),
  syncExternalBanToTarget: (token, banId, targetId) => request(`/api/bans/${banId}/sync-external/${targetId}`, { method: 'POST', headers: withAuth(token), body: JSON.stringify({}) }),
  uploadBanFiles: (token, banId, formData) => request(`/api/bans/${banId}/files`, { method: 'POST', headers: withAuth(token), body: formData }),
  listBanFiles: (token, banId) => request(`/api/bans/${banId}/files`, { headers: withAuth(token) }),
  getBanFileUrl: (token, fileId) => request(`/api/bans/files/${fileId}/url`, { headers: withAuth(token) }),
  banApiKeys: (token) => request('/api/ban-api/keys', { headers: withAuth(token) }),
  createBanApiKey: (token, body) => request('/api/ban-api/keys', { method: 'POST', headers: withAuth(token), body: JSON.stringify(body) }),
  deleteBanApiKey: (token, id) => request(`/api/ban-api/keys/${id}`, { method: 'DELETE', headers: withAuth(token) }),
  users: (token, params = {}) => request(`/api/users${buildQueryString(params)}`, { headers: withAuth(token) }),
  createUser: (token, body) => request('/api/users', { method: 'POST', headers: withAuth(token), body: JSON.stringify(body) }),
  updateUser: (token, id, body) => request(`/api/users/${id}`, { method: 'PUT', headers: withAuth(token), body: JSON.stringify(body) }),
  updateUserPassword: (token, id, body) => request(`/api/users/${id}/password`, { method: 'PUT', headers: withAuth(token), body: JSON.stringify(body) }),
  toggleUserEnabled: (token, id) => request(`/api/users/${id}/toggle-enabled`, { method: 'POST', headers: withAuth(token) }),
  deleteUser: (token, id) => request(`/api/users/${id}`, { method: 'DELETE', headers: withAuth(token) }),
  revokeUserSessions: (token, userId) => request(`/api/auth/users/${userId}/sessions`, { method: 'DELETE', headers: withAuth(token) }),
  logs: (token, params = {}) => request(`/api/logs${buildQueryString(params)}`, { headers: withAuth(token) }),
  docsEndpoints: (token) => request('/api/docs/endpoints', { headers: withAuth(token) }),
  playerAccessRules: (token) => request('/api/player-access/rules', { headers: withAuth(token) }),
  createPlayerAccessRule: (token, body) => request('/api/player-access/rules', { method: 'POST', headers: withAuth(token), body: JSON.stringify(body) }),
  updatePlayerAccessRule: (token, id, body) => request(`/api/player-access/rules/${id}`, { method: 'PUT', headers: withAuth(token), body: JSON.stringify(body) }),
  deletePlayerAccessRule: (token, id) => request(`/api/player-access/rules/${id}`, { method: 'DELETE', headers: withAuth(token) }),
  publicWhitelist: (params = {}) => request(`/api/public/whitelist${buildQueryString(params)}`),
  publicBans: (params = {}) => request(`/api/public/bans${buildQueryString(params)}`),
  submitWhitelist: (body) => request('/api/public/whitelist', { method: 'POST', body: JSON.stringify(body) }),
  resolveSteam: (body) => request('/api/public/steam/resolve', { method: 'POST', body: JSON.stringify(body) }),
  queryActiveBans: (body) => request('/api/public/bans/query', { method: 'POST', body: JSON.stringify(body) }),
  submitBanAppeal: (body) => request('/api/public/ban-appeals/submit', { method: 'POST', body: JSON.stringify(body) }),
  uploadAppealFiles: (appealId, formData) => request(`/api/public/ban-appeals/${appealId}/files`, { method: 'POST', headers: {}, body: formData }),
  submitPlayerReport: (body) => request('/api/public/player-reports', { method: 'POST', body: JSON.stringify(body) }),
  uploadPlayerReportFiles: (reportId, formData) => request(`/api/public/player-reports/${reportId}/files`, { method: 'POST', headers: {}, body: formData }),
  banAppeals: (token, params = {}) => request(`/api/ban-appeals${buildQueryString(params)}`, { headers: withAuth(token) }),
  playerReports: (token, params = {}) => request(`/api/player-reports${buildQueryString(params)}`, { headers: withAuth(token) }),
  reviewPlayerReport: (token, id, body = {}) => request(`/api/player-reports/${id}/review`, { method: 'POST', headers: withAuth(token), body: JSON.stringify(body) }),
  banPlayerReport: (token, id, body = {}) => request(`/api/player-reports/${id}/ban`, { method: 'POST', headers: withAuth(token), body: JSON.stringify(body) }),
  listPlayerReportFiles: (token, reportId) => request(`/api/player-reports/${reportId}/files`, { headers: withAuth(token) }),
  listAdminAppealFiles: (token, appealId) => request(`/api/ban-appeals/${appealId}/files`, { headers: withAuth(token) }),
  getAppealFileUrl: (token, fileId) => request(`/api/ban-appeals/files/${fileId}/url`, { headers: withAuth(token) }),
  approveBanAppeal: (token, id, body = {}) => request(`/api/ban-appeals/${id}/approve`, { method: 'POST', headers: withAuth(token), body: JSON.stringify(body) }),
  rejectBanAppeal: (token, id, body = {}) => request(`/api/ban-appeals/${id}/reject`, { method: 'POST', headers: withAuth(token), body: JSON.stringify(body) }),
  auditLogs: (token, params = {}) => request(`/api/audit/logs${buildQueryString(params)}`, { headers: withAuth(token) }),
  // Notifications
  notifications: (token, params = {}) => request(`/api/notifications${buildQueryString(params)}`, { headers: withAuth(token) }),
  notificationUnreadCount: (token) => request('/api/notifications/unread-count', { headers: withAuth(token) }),
  markNotificationRead: (token, id) => request(`/api/notifications/${id}/read`, { method: 'POST', headers: withAuth(token) }),
  markAllNotificationsRead: (token) => request('/api/notifications/read-all', { method: 'POST', headers: withAuth(token) }),
};

export function useAppApi() {
  return api;
}

import { buildQueryString, request } from './apiClient.js';

export const publicApi = {
  publicWhitelist: (params = {}) => request(`/api/public/whitelist${buildQueryString(params)}`),
  publicBans: (params = {}) => request(`/api/public/bans${buildQueryString(params)}`),
  submitWhitelist: (body) => request('/api/public/whitelist', { method: 'POST', body: JSON.stringify(body) }),
  resolveSteam: (body) => request('/api/public/steam/resolve', { method: 'POST', body: JSON.stringify(body) }),
  queryActiveBans: (body) => request('/api/public/bans/query', { method: 'POST', body: JSON.stringify(body) }),
  preloadGokzStats: (steamid64s) => request('/api/public/gokz/player-stats/preload', { method: 'POST', body: JSON.stringify({ steamid64s }) }),
  getGokzPlayerStatsBatch: (steamid64) => request('/api/public/gokz/player-stats/batch', { method: 'POST', body: JSON.stringify({ steamid64 }) }),
  // Steam OpenID 认证
  getSteamLoginInfo: () => {
    // 显式把前端实际访问的 origin 传给后端，保证 Steam 回调地址与用户访问地址一致
    const origin = typeof window !== 'undefined' ? window.location.origin : '';
    return request(`/api/public/steam/auth/login${origin ? `?origin=${encodeURIComponent(origin)}` : ''}`);
  },
  getSteamSession: (token) => request(`/api/public/steam/auth/session?token=${encodeURIComponent(token)}`),
};

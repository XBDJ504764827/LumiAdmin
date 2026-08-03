import { buildQueryString, request } from './apiClient.js';

export const publicApi = {
  publicWhitelist: (params = {}) => request(`/api/public/whitelist${buildQueryString(params)}`),
  publicBans: (params = {}) => request(`/api/public/bans${buildQueryString(params)}`),
  submitWhitelist: (body) => request('/api/public/whitelist', { method: 'POST', body: JSON.stringify(body) }),
  resolveSteam: (body) => request('/api/public/steam/resolve', { method: 'POST', body: JSON.stringify(body) }),
  queryActiveBans: (body) => request('/api/public/bans/query', { method: 'POST', body: JSON.stringify(body) }),
  preloadGokzStats: (steamid64s) => request('/api/public/gokz/player-stats/preload', { method: 'POST', body: JSON.stringify({ steamid64s }) }),
};

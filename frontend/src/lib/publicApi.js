import { buildQueryString, request } from './apiClient.js';

export const publicApi = {
  publicWhitelist: (params = {}) => request(`/api/public/whitelist${buildQueryString(params)}`),
  publicBans: (params = {}) => request(`/api/public/bans${buildQueryString(params)}`),
  submitWhitelist: (body) => request('/api/public/whitelist', { method: 'POST', body: JSON.stringify(body) }),
  resolveSteam: (body) => request('/api/public/steam/resolve', { method: 'POST', body: JSON.stringify(body) }),
  queryActiveBans: (body) => request('/api/public/bans/query', { method: 'POST', body: JSON.stringify(body) }),
  queryAppealStatus: (body) => request('/api/public/ban-appeals/query', { method: 'POST', body: JSON.stringify(body) }),
  submitBanAppeal: (body) => request('/api/public/ban-appeals/submit', { method: 'POST', body: JSON.stringify(body) }),
  uploadAppealFiles: (appealId, formData) => request(`/api/public/ban-appeals/${appealId}/files`, { method: 'POST', headers: {}, body: formData }),
  submitPlayerReport: (body) => request('/api/public/player-reports', { method: 'POST', body: JSON.stringify(body) }),
  queryPlayerReportStatus: (body) => request('/api/public/player-reports/query', { method: 'POST', body: JSON.stringify(body) }),
  uploadPlayerReportFiles: (reportId, formData) => request(`/api/public/player-reports/${reportId}/files`, { method: 'POST', headers: {}, body: formData }),
  submitMapFeedback: (body) => request('/api/public/map-feedback', { method: 'POST', body: JSON.stringify(body) }),
  queryMapFeedbackStatus: (body) => request('/api/public/map-feedback/query', { method: 'POST', body: JSON.stringify(body) }),
  preloadGokzStats: (steamid64s) => request('/api/public/gokz/player-stats/preload', { method: 'POST', body: JSON.stringify({ steamid64s }) }),
};

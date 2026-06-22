import { apiErrorMessage } from '../shared/authClient.js';

const API_BASE = import.meta.env.VITE_API_BASE ?? '';

export function buildQueryString(params = {}) {
  const entries = Object.entries(params).filter(([, v]) => v !== undefined && v !== null && v !== '');
  return entries.length > 0 ? '?' + entries.map(([k, v]) => `${encodeURIComponent(k)}=${encodeURIComponent(v)}`).join('&') : '';
}

function handleUnauthorized() {
  if (typeof window !== 'undefined') {
    window.localStorage.removeItem('manger_token');
    window.dispatchEvent(new CustomEvent('manger:unauthorized'));
  }
}

export async function request(path, options = {}) {
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

export function withAuth(token) {
  return token ? { Authorization: `Bearer ${token}` } : {};
}

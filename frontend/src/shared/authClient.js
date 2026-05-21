export function readStoredToken(storage = globalThis?.localStorage) {
  if (!storage) return null;
  const token = storage.getItem('manger_token');
  return token && token.trim() ? token : null;
}

export function defaultSessionFromToken(token) {
  if (!token) return null;
  return {
    token,
    userId: null,
    displayName: '',
    role: 'guest',
    roleLabel: '未登录',
  };
}

export function normalizeSession(payload) {
  const session = payload?.session;
  if (!session) return null;

  return {
    token: session.token,
    userId: session.user_id,
    displayName: session.display_name,
    role: session.role,
    roleLabel: session.role_label,
  };
}

export function apiErrorMessage(status, payload = {}) {
  if (status === 401) {
    const msg = payload.error ?? '';
    if (msg === 'invalid credentials') return '用户名或密码错误。';
    return '请先登录后再操作。';
  }
  return payload.error ?? `Request failed: ${status}`;
}

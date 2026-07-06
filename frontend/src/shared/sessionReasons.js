export const SESSION_END_LABELS = {
  admin_kicked: '管理员踢出',
  player_quit: '玩家自行退出',
  access_rejected: '进入门槛拦截',
  banned_kicked: '封禁踢出',
  snapshot_missing: '在线快照消失',
  server_stale: '服务器上报中断',
};

export function sessionReasonLabel(reason) {
  return SESSION_END_LABELS[reason] || reason || '-';
}

export function sessionReasonKind(reason) {
  if (reason === 'banned_kicked' || reason === 'access_rejected') return 'danger';
  if (reason === 'admin_kicked') return 'warning';
  if (reason === 'player_quit') return 'success';
  return 'default';
}

export function eventSourceLabel(source) {
  return SESSION_END_LABELS[source] || source || '-';
}

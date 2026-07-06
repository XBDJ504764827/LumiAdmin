export function serverStatusMeta(status) {
  if (status === 'online') return { label: '在线', className: 'pill-online', online: true };
  if (status === 'hibernating') return { label: '休眠/待上报', className: 'pill-warning', online: false };
  if (status === 'untested') return { label: '未测试', className: 'pill-default', online: false };
  return { label: '离线', className: 'pill-offline', online: false };
}

export function rconServerStatusText(status) {
  if (status === 'online') return { option: '● 在线', detail: '命令可执行', color: 'var(--success-text)' };
  if (status === 'hibernating') return { option: '◐ 休眠/待上报', detail: 'RCON 可尝试执行', color: 'var(--warning-text)' };
  return { option: '○ 离线', detail: '命令可能无法执行', color: 'var(--danger-text)' };
}

const icon = (children) => (
  <svg viewBox="0 0 16 16" fill="none" width="16" height="16" stroke="currentColor" strokeWidth="1.5">
    {children}
  </svg>
);

const icons = {
  grid: icon(<><rect x="1" y="1" width="6" height="6" rx="1.5" /><rect x="9" y="1" width="6" height="6" rx="1.5" /><rect x="1" y="9" width="6" height="6" rx="1.5" /><rect x="9" y="9" width="6" height="6" rx="1.5" /></>),
  community: icon(<><path d="M12 2H4a1 1 0 00-1 1v10a1 1 0 001 1h8a1 1 0 001-1V3a1 1 0 00-1-1z" /><path d="M5 6h6M5 9h4" /></>),
  whitelist: icon(<><path d="M2 12L5 8l3 2 3-5 3 3" /><rect x="1" y="1" width="14" height="14" rx="2" /></>),
  ban: icon(<><circle cx="8" cy="8" r="6" /><path d="M5 5L11 11M11 5L5 11" /></>),
  users: icon(<><circle cx="8" cy="5" r="3" /><path d="M2 14c0-3.314 2.686-5 6-5s6 1.686 6 5" /></>),
  logs: icon(<><path d="M2 2h4v5H2zM10 2h4v3h-4zM10 9h4v5h-4zM2 11h4v3H2zM6 5h4v11M6 2h4v3" /></>),
  apply: icon(<><path d="M4 2v12l4-2 4 2V2z" /></>),
  table: icon(<><path d="M1 4h14M1 8h14M1 12h14" /></>),
  banPublic: icon(<><path d="M8 1v14M1 8h14" transform="rotate(45 8 8)" /></>),
};

export function sidebarSections(role) {
  return [
    { label: '核心管理', items: [{ path: '/dashboard', label: '仪表盘', icon: icons.grid }, { path: '/community', label: '社区组管理', icon: icons.community }, { path: '/whitelist', label: '白名单管理', icon: icons.whitelist }, { path: '/ban', label: '封禁管理', icon: icons.ban }] },
    { label: '系统功能', items: role === 'developer' ? [{ path: '/users', label: '网站用户', icon: icons.users }, { path: '/logs', label: '操作日志', icon: icons.logs }] : [{ path: '/logs', label: '操作日志', icon: icons.logs }] },
    { label: '公共展示页 (Public)', items: [{ path: '/public/apply', label: '白名单申请', icon: icons.apply }, { path: '/public/whitelist', label: '白名单公示', icon: icons.table }, { path: '/public/ban', label: '封禁公示', icon: icons.banPublic }] },
  ];
}

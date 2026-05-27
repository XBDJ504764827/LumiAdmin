import React from 'react';

const icon = (children) => (
  <svg viewBox="0 0 16 16" fill="none" width="16" height="16" stroke="currentColor" strokeWidth="1.5">
    {children}
  </svg>
);

const icons = {
  grid: icon(<><rect x="1" y="1" width="6" height="6" rx="1.5" /><rect x="9" y="1" width="6" height="6" rx="1.5" /><rect x="1" y="9" width="6" height="6" rx="1.5" /><rect x="9" y="9" width="6" height="6" rx="1.5" /></>),
  community: icon(<><path d="M12 2H4a1 1 0 00-1 1v10a1 1 0 001 1h8a1 1 0 001-1V3a1 1 0 00-1-1z" /><path d="M5 6h6M5 9h4" /></>),
  rcon: icon(<><rect x="2" y="2" width="20" height="8" rx="2" /><rect x="2" y="14" width="20" height="8" rx="2" /><circle cx="6" cy="6" r="1" /><circle cx="6" cy="18" r="1" /><path d="M12 6h6M12 18h6" strokeLinecap="round" /></>),
  whitelist: icon(<><path d="M2 12L5 8l3 2 3-5 3 3" /><rect x="1" y="1" width="14" height="14" rx="2" /></>),
  ban: icon(<><circle cx="8" cy="8" r="6" /><path d="M5 5L11 11M11 5L5 11" /></>),
  banAppeal: icon(<><path d="M8 1.5C5.5 1.5 3.5 3.5 3 6c0 2.5-1 3.5-2.5 5h15C14 9.5 13 8.5 13 6c-.5-2.5-2.5-4.5-5-4.5z" /><path d="M6 12.5c.5 1 1 1.5 2 1.5s1.5-.5 2-1.5" strokeLinecap="round" /><path d="M3 1.5L13 11.5" strokeLinecap="round" /></>),
  playerReport: icon(<><path d="M8 1.5l6 2.5v4.5c0 3.5-2.4 5.5-6 6-3.6-.5-6-2.5-6-6V4l6-2.5z" /><path d="M8 5v4" strokeLinecap="round" /><path d="M8 12h.01" strokeLinecap="round" /></>),
  users: icon(<><circle cx="8" cy="5" r="3" /><path d="M2 14c0-3.314 2.686-5 6-5s6 1.686 6 5" /></>),
  playerAccess: icon(<><path d="M11 2v4m0 0h4m-4 0L15 2M5 14v-4m0 0H1m4 0l-4 4M2 8a6 6 0 1 0 12 0A6 6 0 0 0 2 8z" strokeLinecap="round" strokeLinejoin="round" /></>),
  audit: icon(<><path d="M8 2l6 3v5c0 3.5-2.5 6.5-6 8-3.5-1.5-6-4.5-6-8V5l6-3z" /><path d="M6 8l2 2 4-4" strokeLinecap="round" strokeLinejoin="round" /></>),
  notification: icon(<><path d="M8 1.5C5.5 1.5 3.5 3.5 3 6c0 2.5-1 3.5-2.5 5h15C14 9.5 13 8.5 13 6c-.5-2.5-2.5-4.5-5-4.5z" /><path d="M6 12.5c.5 1 1 1.5 2 1.5s1.5-.5 2-1.5" strokeLinecap="round" /></>),
  logs: icon(<><path d="M2 2h4v5H2zM10 2h4v3h-4zM10 9h4v5h-4zM2 11h4v3H2zM6 5h4v11M6 2h4v3" /></>),
  api: icon(<><path d="M4 6l-3 2 3 2m8-4l3 2-3 2m-5 4l2-10" strokeLinecap="round" strokeLinejoin="round" /></>),
  playerApi: icon(<><rect x="2" y="3" width="12" height="10" rx="2" /><path d="M6 8h4M6 12h4M2 7h12" /></>),
  externalServer: icon(<><circle cx="8" cy="8" r="6" /><path d="M8 2v3M8 11v3M2 8h3M11 8h3" strokeLinecap="round" /></>),
  apply: icon(<><path d="M4 2v12l4-2 4 2V2z" /></>),
  table: icon(<><path d="M1 4h14M1 8h14M1 12h14" /></>),
  banPublic: icon(<><path d="M8 1v14M1 8h14" transform="rotate(45 8 8)" /></>),
  // 二级菜单父级图标
  logFolder: icon(<><path d="M2 3h5l2 2h5a1 1 0 011 1v6a1 1 0 01-1 1H2a1 1 0 01-1-1V4a1 1 0 011-1z" /></>),
  globe: icon(<><circle cx="8" cy="8" r="6" /><path d="M2 8h12M8 2c1.5 3 1.5 9 0 12" /></>),
};

export function sidebarSections(role) {
  const canSeeUserManagement = ['developer', 'admin', 'normal'].includes(role);
  const canSeeLogs = ['developer', 'admin'].includes(role);
  const canReviewReports = ['developer', 'admin'].includes(role);

  const systemItems = canSeeLogs ? [
    { path: '/player-access', label: '玩家进服设置', icon: icons.playerAccess },
    { path: '/notifications', label: '通知中心', icon: icons.notification },
    {
      label: '日志与审计',
      icon: icons.logFolder,
      children: [
        { path: '/audit', label: '审计日志', icon: icons.audit },
        { path: '/logs', label: '操作日志', icon: icons.logs },
      ],
    },
    {
      label: 'API 管理',
      icon: icons.api,
      children: [
        { path: '/docs/api', label: 'API 接口列表', icon: icons.api },
        { path: '/player-api', label: '玩家信息 API', icon: icons.playerApi },
      ],
    },
    { path: '/external-servers', label: '外部服务器', icon: icons.externalServer },
  ] : [
    { path: '/player-access', label: '玩家进服设置', icon: icons.playerAccess },
    { path: '/notifications', label: '通知中心', icon: icons.notification },
    { path: '/audit', label: '审计日志', icon: icons.audit },
    { path: '/external-servers', label: '外部服务器', icon: icons.externalServer },
  ];

  return [
    {
      label: '核心管理',
      items: [
        { path: '/dashboard', label: '仪表盘', icon: icons.grid },
        { path: '/community', label: '社区组管理', icon: icons.community },
        { path: '/whitelist', label: '白名单管理', icon: icons.whitelist },
        { path: '/ban', label: '封禁管理', icon: icons.ban },
        ...(canReviewReports ? [
          { path: '/ban-appeal', label: '封禁申诉', icon: icons.banAppeal },
          { path: '/player-reports', label: '玩家举报', icon: icons.playerReport },
        ] : []),
        ...(canSeeUserManagement ? [{ path: '/users', label: '网站用户管理', icon: icons.users }] : []),
      ],
    },
    {
      label: '系统功能',
      items: systemItems,
    },
    {
      label: '公共展示',
      items: [
        {
          label: 'Public 公开页面',
          icon: icons.globe,
          children: [
            { path: '/public/apply', label: '白名单申请', icon: icons.apply },
            { path: '/public/whitelist', label: '白名单公示', icon: icons.table },
            { path: '/public/ban', label: '封禁公示', icon: icons.banPublic },
            { path: '/public/ban-appeal', label: '封禁申诉', icon: icons.banAppeal },
            { path: '/public/player-report', label: '玩家举报', icon: icons.playerReport },
          ],
        },
      ],
    },
  ].filter((section) => section.items.length > 0);
}

import React from 'react';

/**
 * 共享图标组件库
 * 所有 SVG 尺寸统一为 16x16 / 20x20 / 24x24 三种规格，
 * 均支持 className、size、color 等通用属性。
 *
 * aria-hidden 确保装饰性图标对屏幕阅读器不可见；
 * 对于有语义的图标（如操作按钮），请在父元素上添加 aria-label。
 */

function createIcon(viewBox, paths) {
  return function Icon({ size = 16, className = '', ...props }) {
    return (
      <svg
        viewBox={viewBox}
        width={size}
        height={size}
        fill="none"
        stroke="currentColor"
        strokeWidth="1.5"
        strokeLinecap="round"
        strokeLinejoin="round"
        className={className}
        aria-hidden="true"
        {...props}
      >
        {paths}
      </svg>
    );
  };
}

/* ── 通用 / 导航 ── */
export const IconGrid = createIcon('0 0 16 16', (
  <><rect x="1" y="1" width="6" height="6" rx="1.5" /><rect x="9" y="1" width="6" height="6" rx="1.5" /><rect x="1" y="9" width="6" height="6" rx="1.5" /><rect x="9" y="9" width="6" height="6" rx="1.5" /></>
));

export const IconUsers = createIcon('0 0 24 24', (
  <><circle cx="9" cy="7" r="4" /><path d="M2 21v-2a4 4 0 0 1 4-4h6a4 4 0 0 1 4 4v2" /><path d="M23 21v-2a4 4 0 0 0-3-3.87" /><path d="M16 3.13a4 4 0 0 1 0 7.75" /></>
));

export const IconBell = createIcon('0 0 24 24', (
  <><path d="M18 8A6 6 0 0 0 6 8c0 7-3 9-3 9h18s-3-2-3-9" /><path d="M13.73 21a2 2 0 0 1-3.46 0" /></>
));

export const IconSearch = createIcon('0 0 24 24', (
  <><circle cx="11" cy="11" r="8" /><path d="M21 21l-4.35-4.35" /></>
));

export const IconHome = createIcon('0 0 24 24', (
  <><path d="M3 9l9-7 9 7v11a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" /><polyline points="9 22 9 12 15 12 15 22" /></>
));

export const IconGlobe = createIcon('0 0 24 24', (
  <><circle cx="12" cy="12" r="10" /><line x1="2" y1="12" x2="22" y2="12" /><path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z" /></>
));

/* ── 服务器 / 社区 ── */
export const IconServer = createIcon('0 0 24 24', (
  <><rect x="2" y="2" width="20" height="8" rx="2" ry="2" /><rect x="2" y="14" width="20" height="8" rx="2" ry="2" /><circle cx="6" cy="6" r="1" fill="currentColor" /><circle cx="6" cy="18" r="1" fill="currentColor" /></>
));

export const IconCpu = createIcon('0 0 24 24', (
  <><rect x="4" y="4" width="16" height="16" rx="2" /><rect x="9" y="9" width="6" height="6" /><line x1="9" y1="1" x2="9" y2="4" /><line x1="15" y1="1" x2="15" y2="4" /><line x1="9" y1="20" x2="9" y2="23" /><line x1="15" y1="20" x2="15" y2="23" /><line x1="20" y1="9" x2="23" y2="9" /><line x1="20" y1="14" x2="23" y2="14" /><line x1="1" y1="9" x2="4" y2="9" /><line x1="1" y1="14" x2="4" y2="14" /></>
));

/* ── 操作 ── */
export const IconEdit = createIcon('0 0 24 24', (
  <><path d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7" /><path d="M18.5 2.5a2.121 2.121 0 0 1 3 3L12 15l-4 1 1-4 9.5-9.5z" /></>
));

export const IconTrash = createIcon('0 0 24 24', (
  <><polyline points="3 6 5 6 21 6" /><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" /></>
));

export const IconPlus = createIcon('0 0 24 24', (
  <><line x1="12" y1="5" x2="12" y2="19" /><line x1="5" y1="12" x2="19" y2="12" /></>
));

export const IconX = createIcon('0 0 24 24', (
  <><line x1="18" y1="6" x2="6" y2="18" /><line x1="6" y1="6" x2="18" y2="18" /></>
));

export const IconCheck = createIcon('0 0 24 24', (
  <><polyline points="20 6 9 17 4 12" /></>
));

/* ── 安全 / 权限 ── */
export const IconShield = createIcon('0 0 24 24', (
  <><path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z" /></>
));

export const IconLock = createIcon('0 0 24 24', (
  <><rect x="3" y="11" width="18" height="11" rx="2" ry="2" /><path d="M7 11V7a5 5 0 0 1 10 0v4" /></>
));

export const IconKey = createIcon('0 0 24 24', (
  <><path d="M21 2l-2 2m-7.61 7.61a5.5 5.5 0 1 1-7.778 7.778 5.5 5.5 0 0 1 7.777-7.777zm0 0L15.5 7.5m0 0l3 3L22 7l-3-3m-3.5 3.5L19 4" /></>
));

export const IconEye = createIcon('0 0 24 24', (
  <><path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z" /><circle cx="12" cy="12" r="3" /></>
));

/* ── 媒体 / 文件 ── */
export const IconPlay = createIcon('0 0 24 24', (
  <><polygon points="5 3 19 12 5 21 5 3" /></>
));

export const IconFile = createIcon('0 0 24 24', (
  <><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" /><polyline points="14 2 14 8 20 8" /></>
));

export const IconImage = createIcon('0 0 24 24', (
  <><rect x="3" y="3" width="18" height="18" rx="2" ry="2" /><circle cx="8.5" cy="8.5" r="1.5" /><polyline points="21 15 16 10 5 21" /></>
));

export const IconVideo = createIcon('0 0 24 24', (
  <><polygon points="23 7 16 12 23 17 23 7" /><rect x="1" y="5" width="15" height="14" rx="2" ry="2" /></>
));

export const IconMusic = createIcon('0 0 24 24', (
  <><path d="M9 18V5l12-2v13" /><circle cx="6" cy="18" r="3" /><circle cx="18" cy="16" r="3" /></>
));

/* ── 状态 ── */
export const IconClock = createIcon('0 0 24 24', (
  <><circle cx="12" cy="12" r="10" /><polyline points="12 6 12 12 16 14" /></>
));

export const IconAlertTriangle = createIcon('0 0 24 24', (
  <><path d="M10.29 3.86L1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z" /><line x1="12" y1="9" x2="12" y2="13" /><line x1="12" y1="17" x2="12.01" y2="17" /></>
));

export const IconRefresh = createIcon('0 0 24 24', (
  <><polyline points="23 4 23 10 17 10" /><path d="M20.49 15a9 9 0 1 1-2.12-9.36L23 10" /></>
));

/* ── 方向 ── */
export const IconChevronLeft = createIcon('0 0 24 24', (
  <polyline points="15 18 9 12 15 6" />
));

export const IconChevronRight = createIcon('0 0 24 24', (
  <polyline points="9 18 15 12 9 6" />
));

export const IconChevronDown = createIcon('0 0 24 24', (
  <polyline points="6 9 12 15 18 9" />
));

export const IconChevronUp = createIcon('0 0 24 24', (
  <polyline points="18 15 12 9 6 15" />
));

export const IconExternalLink = createIcon('0 0 24 24', (
  <><path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6" /><polyline points="15 3 21 3 21 9" /><line x1="10" y1="14" x2="21" y2="3" /></>
));

/* ── 太阳 / 月亮（主题切换） ── */
export const IconSun = createIcon('0 0 24 24', (
  <><circle cx="12" cy="12" r="5" /><line x1="12" y1="1" x2="12" y2="3" /><line x1="12" y1="21" x2="12" y2="23" /><line x1="4.22" y1="4.22" x2="5.64" y2="5.64" /><line x1="18.36" y1="18.36" x2="19.78" y2="19.78" /><line x1="1" y1="12" x2="3" y2="12" /><line x1="21" y1="12" x2="23" y2="12" /><line x1="4.22" y1="19.78" x2="5.64" y2="18.36" /><line x1="18.36" y1="5.64" x2="19.78" y2="4.22" /></>
));

export const IconMoon = createIcon('0 0 24 24', (
  <path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z" />
));

/* ── 仪表盘 / 统计 ── */
export const IconActivity = createIcon('0 0 24 24', (
  <polyline points="22 12 18 12 15 21 9 3 6 12 2 12" />
));

export const IconBarChart = createIcon('0 0 24 24', (
  <><line x1="18" y1="20" x2="18" y2="10" /><line x1="12" y1="20" x2="12" y2="4" /><line x1="6" y1="20" x2="6" y2="14" /></>
));

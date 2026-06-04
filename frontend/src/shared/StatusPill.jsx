import React from 'react';

export function StatusPill({ kind = 'muted', className = '', children }) {
  // 同时输出 status-pill（兼容大部分页面的选择器）和 pill-* 变体
  return <span className={`status-pill pill-${kind} ${className}`.trim()}>{children}</span>;
}

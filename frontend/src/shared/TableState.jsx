/**
 * 表格状态组件
 * 统一所有数据表格的加载中、错误、空数据三种状态展示。
 * 替代各页面中分散的 <tr><td colSpan> 内联样式写法。
 */

import { IconSearch } from './Icons.jsx';

/**
 * 表格内嵌的加载状态行
 * 用法：<TableLoading colSpan={8} />
 */
export function TableLoading({ colSpan, text = '正在加载数据...' }) {
  return (
    <tr className="table-state-row">
      <td colSpan={colSpan} className="table-state-cell">
        <div className="table-state-inner">
          <div className="table-state-spinner" />
          <span className="table-state-text">{text}</span>
        </div>
      </td>
    </tr>
  );
}

/**
 * 表格内嵌的错误状态行
 * 用法：<TableError colSpan={8} message="加载失败" />
 */
export function TableError({ colSpan, message = '加载失败，请稍后重试' }) {
  return (
    <tr className="table-state-row">
      <td colSpan={colSpan} className="table-state-cell">
        <div className="table-state-inner table-state-inner--error">
          <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <circle cx="12" cy="12" r="10" />
            <line x1="12" y1="8" x2="12" y2="12" />
            <line x1="12" y1="16" x2="12.01" y2="16" />
          </svg>
          <span className="table-state-text">{message}</span>
        </div>
      </td>
    </tr>
  );
}

/**
 * 表格内嵌的空数据状态行
 * 用法：<TableEmpty colSpan={7} text="暂无封禁记录" icon={IconShield} />
 */
export function TableEmpty({ colSpan, text = '暂无数据', icon: Icon }) {
  return (
    <tr className="table-state-row">
      <td colSpan={colSpan} className="table-state-cell">
        <div className="table-state-inner table-state-inner--empty">
          <div className="table-state-icon">
            {Icon ? <Icon size={20} /> : (
              <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                <path d="M13 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V9z" />
                <polyline points="13 2 13 9 20 9" />
              </svg>
            )}
          </div>
          <span className="table-state-text">{text}</span>
        </div>
      </td>
    </tr>
  );
}

/**
 * 非表格场景的空状态区块（类似 player-detail-empty）
 * 用法：<EmptyBlock icon={IconShield}>暂无封禁记录</EmptyBlock>
 */
export function EmptyBlock({ children, icon: Icon }) {
  return (
    <div className="empty-state-v2">
      <div className="empty-state-v2-icon">
        {Icon ? <Icon size={22} /> : <IconSearch size={22} />}
      </div>
      <div className="empty-state-v2-text">{children}</div>
    </div>
  );
}

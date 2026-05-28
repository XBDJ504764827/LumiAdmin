import React, { useCallback, useMemo, useState } from 'react';
import { useLocation, useNavigate } from 'react-router-dom';
import { useAuth } from '../../state/auth.jsx';
import { ThemeToggle } from '../../shared/ThemeToggle.jsx';
import { PendingReviewProvider, usePendingReviewData } from '../../hooks/usePendingReviewIndicators.js';
import { sidebarSections } from './sidebarSections.jsx';

function hasActiveChild(children, pathname) {
  return children.some((child) => child.path === pathname || (child.children && hasActiveChild(child.children, pathname)));
}

export function AppShell({ children }) {
  const [collapsed, setCollapsed] = useState(false);
  const { session, logout } = useAuth();
  const location = useLocation();
  const navigate = useNavigate();
  const sections = useMemo(() => sidebarSections(session?.role ?? 'guest'), [session?.role]);
  const pendingReviews = usePendingReviewData();
  const pendingCounts = pendingReviews.counts;
  const avatar = (session?.displayName ?? 'AL').slice(0, 2).toUpperCase();

  // 跟踪展开的父级菜单（用 label 作为 key）
  const [expanded, setExpanded] = useState(() => {
    // 初始化时自动展开包含当前路径的父级
    const initial = new Set();
    sections.forEach((section) => {
      section.items.forEach((item) => {
        if (item.children && hasActiveChild(item.children, location.pathname)) {
          initial.add(item.label);
        }
      });
    });
    return initial;
  });

  const toggleExpand = useCallback((label) => {
    if (collapsed) {
      setCollapsed(false);
      // 延迟展开子菜单，等待侧边栏动画完成
      setTimeout(() => {
        setExpanded((prev) => {
          const next = new Set(prev);
          next.add(label);
          return next;
        });
      }, 150);
      return;
    }
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(label)) {
        next.delete(label);
      } else {
        next.add(label);
      }
      return next;
    });
  }, [collapsed]);

  // 渲染单个导航项
  function renderNavItem(item, isSubItem = false) {
    const isActive = location.pathname === item.path;
    const pendingCount = item.pendingKey ? pendingCounts[item.pendingKey] ?? 0 : 0;
    const hasPending = pendingCount > 0;
    return (
      <button
        key={item.path}
        className={`nav-item ${isSubItem ? 'nav-sub-item' : ''} ${isActive ? 'active' : ''}`}
        onClick={() => navigate(item.path)}
        title={collapsed ? item.label : undefined}
      >
        <span className="nav-icon">{item.icon}</span>
        <span className="nav-text-wrap">
          <span className="nav-text">{item.label}</span>
          {hasPending ? (
            <span
              className="nav-pending-dot"
              aria-label={`${item.label}有待审核内容`}
              title={`${item.label}有 ${pendingCount} 条待审核`}
            />
          ) : null}
        </span>
      </button>
    );
  }

  // 渲染可折叠的父级菜单
  function renderCollapsibleItem(item) {
    const isExpanded = expanded.has(item.label);
    const isChildActive = item.children && hasActiveChild(item.children, location.pathname);

    return (
      <div key={item.label} className="nav-collapsible-group">
        <button
          className={`nav-item nav-parent-item ${isChildActive ? 'child-active' : ''}`}
          onClick={() => toggleExpand(item.label)}
          title={collapsed ? item.label : undefined}
        >
          <span className="nav-icon">{item.icon}</span>
          <span className="nav-text">{item.label}</span>
          <span className={`nav-chevron ${isExpanded ? 'expanded' : ''}`}>
            <svg viewBox="0 0 10 10" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round">
              <path d="M2.5 3.5L5 6L7.5 3.5" />
            </svg>
          </span>
        </button>
        {isExpanded && (
          <div className="nav-sub-menu">
            {item.children.map((child) => renderNavItem(child, true))}
          </div>
        )}
      </div>
    );
  }

  return (
    <PendingReviewProvider value={pendingReviews}>
      <div className={`app-shell ${collapsed ? 'sidebar-collapsed' : ''}`}>
      <aside className={`sidebar ${collapsed ? 'collapsed' : ''}`} id="sidebar">
        <div className="sidebar-logo">
          <div className="logo-mark">
            <svg viewBox="0 0 24 24" fill="none">
              <path d="M12 3l8 5v8l-8 5-8-5V8l8-5z" stroke="white" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" />
              <path d="M12 8v8M8 10v4M16 10v4" stroke="white" strokeWidth="1.4" strokeLinecap="round" opacity="0.7" />
            </svg>
          </div>
          {!collapsed && <div className="logo-text">Lumi<span>Admin</span></div>}
        </div>
        <button
          className="sidebar-collapse-btn"
          onClick={() => setCollapsed((v) => !v)}
          title={collapsed ? '展开侧边栏' : '折叠侧边栏'}
        >
          <svg viewBox="0 0 10 10" fill="none" stroke="currentColor"><path d="M6.5 2L3.5 5L6.5 8" strokeLinecap="round" strokeLinejoin="round" /></svg>
        </button>

        <nav className="nav-section">
          {sections.map((section) => (
            <div key={section.label} className="nav-group-block">
              <div className="nav-label">{section.label}</div>
              {section.items.map((item) => {
                if (item.children) {
                  return renderCollapsibleItem(item);
                }
                return renderNavItem(item);
              })}
            </div>
          ))}
        </nav>

        <div className="sidebar-footer">
          <div className="user-card">
            <div className="avatar avatar-accent" title={session?.displayName}>{avatar}</div>
            <div className="user-info">
              <div className="user-name">{session?.displayName ?? 'Alex 管理员'}</div>
              <div className="user-role">{session?.roleLabel ?? '系统管理员'}</div>
            </div>
            <button className="logout-btn" onClick={logout} title="退出登录">
              <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="logout-icon">
                <path d="M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4" />
                <polyline points="16 17 21 12 16 7" />
                <line x1="21" y1="12" x2="9" y2="12" />
              </svg>
              <span className="logout-text">退出</span>
            </button>
          </div>
        </div>
      </aside>

      <div className="main" id="main">
        <header className="topbar">
          <div className="search-box">
            <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5"><circle cx="7" cy="7" r="4.5" /><path d="M10.5 10.5L14 14" /></svg>
            <input placeholder="搜索玩家、服务器或配置..." />
            <span className="search-shortcut">⌘ K</span>
          </div>
          <div className="topbar-actions">
            <ThemeToggle compact />
            <button className="icon-btn" type="button" onClick={() => navigate('/public/apply')} title="公开页面">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                <path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6" />
                <polyline points="15 3 21 3 21 9" />
                <line x1="10" y1="14" x2="21" y2="3" />
              </svg>
            </button>
            <div className="topbar-divider" />
            <button className="topbar-avatar" type="button">{avatar}</button>
          </div>
        </header>
        {children}
      </div>
      </div>
    </PendingReviewProvider>
  );
}

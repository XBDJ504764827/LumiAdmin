import React, { useMemo, useState } from 'react';
import { useLocation, useNavigate } from 'react-router-dom';
import { useAuth } from '../../state/auth.jsx';
import { ThemeToggle } from '../../shared/ThemeToggle.jsx';
import { NotificationBell } from '../NotificationBell.jsx';
import { sidebarSections } from './sidebarSections.jsx';

export function AppShell({ children }) {
  const [collapsed, setCollapsed] = useState(false);
  const { session, logout } = useAuth();
  const location = useLocation();
  const navigate = useNavigate();
  const sections = useMemo(() => sidebarSections(session?.role ?? 'guest'), [session?.role]);
  const avatar = (session?.displayName ?? 'AL').slice(0, 2).toUpperCase();

  return (
    <div className={`app-shell ${collapsed ? 'sidebar-collapsed' : ''}`}>
      <aside className={`sidebar ${collapsed ? 'collapsed' : ''}`} id="sidebar">
        <div className="sidebar-logo">
          <div className="logo-mark"><svg viewBox="0 0 16 16" fill="none"><path d="M2 8L6 4L10 8L14 4" stroke="white" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" /><path d="M2 12L6 8L10 12L14 8" stroke="white" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" opacity="0.6" /></svg></div>
          {!collapsed && <div className="logo-text">Lumi<span>Admin</span></div>}
        </div>
        <div className="sidebar-collapse-btn" onClick={() => setCollapsed((v) => !v)}><svg viewBox="0 0 10 10" fill="none" stroke="currentColor"><path d="M6.5 2L3.5 5L6.5 8" strokeLinecap="round" /></svg></div>

        <div className="nav-section">
          {sections.map((section) => (
            <div key={section.label}>
              <div className="nav-label">{section.label}</div>
              {section.items.map((item) => (
                <button key={item.path} className={`nav-item ${location.pathname === item.path ? 'active' : ''}`} onClick={() => navigate(item.path)}>
                  {item.icon}
                  {!collapsed && <span className="nav-text">{item.label}</span>}
                </button>
              ))}
            </div>
          ))}
        </div>

        <div className="sidebar-footer">
          <div className="user-card">
            <div className="avatar avatar-accent">{avatar}</div>
            {!collapsed && (
              <div className="nav-text" style={{ flex: 1, minWidth: 0, overflow: 'hidden' }}>
                <div style={{ fontSize: 13, fontWeight: 500, whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>{session?.displayName ?? 'Alex 管理员'}</div>
                <div style={{ fontSize: 11.5, color: 'var(--text3)' }}>{session?.roleLabel ?? '系统管理员'}</div>
              </div>
            )}
            {!collapsed && (
              <button className="logout-btn" onClick={logout} title="退出登录">
                退出
              </button>
            )}
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
            <NotificationBell />
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
  );
}

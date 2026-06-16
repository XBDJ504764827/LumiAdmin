import { useMemo, useState } from 'react';
import { Link, useLocation, useNavigate } from 'react-router-dom';
import { useAuth } from '../state/store.js';

export function AppLayout({ navItems, children }) {
  const [collapsed, setCollapsed] = useState(false);
  const location = useLocation();
  const navigate = useNavigate();
  const { session, logout } = useAuth();

  const grouped = useMemo(() => {
    return navItems.reduce((acc, item) => {
      (acc[item.group] ||= []).push(item);
      return acc;
    }, {});
  }, [navItems]);

  return (
    <div className={`app-shell ${collapsed ? 'sidebar-collapsed' : ''}`}>
      <aside className="sidebar">
        <div className="brand">
          <div className="brand-mark">L</div>
          {!collapsed && <div className="brand-text">Lumi<span>Admin</span></div>}
          <button className="collapse-btn" onClick={() => setCollapsed((v) => !v)}>‹</button>
        </div>
        <nav className="nav-group">
          {Object.entries(grouped).map(([group, items]) => (
            <div key={group}>
              <div className="nav-label">{group}{group === '公共展示页' ? ' (Public)' : ''}</div>
              {items.map((item) => (
                <Link key={item.path} to={item.path} className={`nav-item ${location.pathname === item.path ? 'active' : ''}`}>
                  <span className="nav-icon" />
                  {!collapsed && <span>{item.label}</span>}
                </Link>
              ))}
            </div>
          ))}
        </nav>
        <div className="sidebar-footer">
          <div className="user-chip">
            <div className="avatar">{session?.displayName?.slice(0, 2).toUpperCase() ?? 'AL'}</div>
            {!collapsed && (
              <div className="user-info">
                <div className="user-name">{session?.displayName ?? 'Alex 管理员'}</div>
                <div className="user-role">{session?.roleLabel ?? '系统管理员'}</div>
              </div>
            )}
            {!collapsed && (
              <button
                className="logout-btn"
                onClick={logout}
                title="退出登录"
                style={{ display: 'block' }}
              >
                退出
              </button>
            )}
          </div>
        </div>
      </aside>
      <div className="app-main">
        <header className="topbar">
          <div className="searchbar">
            <input placeholder="搜索玩家、服务器或配置..." />
            <span>⌘ K</span>
          </div>
          <div className="topbar-actions">
            <button className="icon-btn">🔔</button>
            <button className="icon-btn" onClick={() => navigate('/public/apply')}>↗</button>
            <div className="topbar-avatar">{session?.displayName?.slice(0, 2).toUpperCase() ?? 'AL'}</div>
          </div>
        </header>
        {children}
      </div>
    </div>
  );
}

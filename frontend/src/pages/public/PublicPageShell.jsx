import { useLocation, useNavigate } from 'react-router-dom';
import { ThemeToggle } from '../../shared/ThemeToggle.jsx';

const navItems = [
  { path: '/public/apply', label: '白名单申请', icon: (
    <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
      <path d="M9 11l3 3L22 4" /><path d="M21 12v7a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11" />
    </svg>
  )},
  { path: '/public/whitelist', label: '白名单公示', icon: (
    <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
      <path d="M17 21v-2a4 4 0 0 0-4-4H5a4 4 0 0 0-4 4v2" /><circle cx="9" cy="7" r="4" /><path d="M23 21v-2a4 4 0 0 0-3-3.87" /><path d="M16 3.13a4 4 0 0 1 0 7.75" />
    </svg>
  )},
  { path: '/public/ban', label: '封禁公示', icon: (
    <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
      <circle cx="12" cy="12" r="10" /><line x1="4.93" y1="4.93" x2="19.07" y2="19.07" />
    </svg>
  )},
  { path: '/public/ban-appeal', label: '封禁申诉', icon: (
    <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
      <path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z" />
    </svg>
  )},
  { path: '/public/player-report', label: '玩家举报', icon: (
    <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
      <path d="M10.29 3.86L1.82 18a2 2 0 001.71 3h16.94a2 2 0 001.71-3L13.71 3.86a2 2 0 00-3.42 0z" />
      <path d="M12 9v4" /><path d="M12 17h.01" />
    </svg>
  )},
];

export function PublicPageShell({ children }) {
  const location = useLocation();
  const navigate = useNavigate();

  return (
    <div className="public-shell">
      {/* 顶部导航栏 */}
      <header className="public-nav">
        <div className="public-nav-inner">
          <div className="public-brand" onClick={() => navigate('/public/apply')}>
            <div className="public-brand-icon">
              <svg width="18" height="18" viewBox="0 0 16 16" fill="none">
                <path d="M2 8L6 4L10 8L14 4" stroke="white" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" />
                <path d="M2 12L6 8L10 12L14 8" stroke="white" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" opacity="0.6" />
              </svg>
            </div>
            <span className="public-brand-text">Lumi<span className="text-accent">Admin</span></span>
          </div>
          <nav className="public-nav-links">
            {navItems.map((item) => (
              <button
                key={item.path}
                className={`public-nav-link ${location.pathname === item.path ? 'active' : ''}`}
                onClick={() => navigate(item.path)}
              >
                {item.icon}
                <span>{item.label}</span>
              </button>
            ))}
          </nav>
          <div className="public-nav-actions">
            <ThemeToggle compact />
          </div>
        </div>
      </header>

      {/* 页面内容 */}
      <main className="public-main">
        {children}
      </main>

      {/* 底部 */}
      <footer className="public-footer">
        <span>Powered by LumiAdmin</span>
      </footer>
    </div>
  );
}

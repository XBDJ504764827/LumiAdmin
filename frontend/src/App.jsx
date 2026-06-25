import { useEffect, useState } from 'react';
import { Navigate, Route, Routes, useNavigate } from 'react-router-dom';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { useAuth, useAuthStore } from './state/store.js';
import { ToastProvider } from './shared/Toast.jsx';
import { AppShell } from './components/layout/AppShell.jsx';
import { publicRoutes, protectedRoutes } from './routes/routeConfig.jsx';
import { NotFoundPage } from './pages/notFound/NotFoundPage.jsx';

// 创建 React Query 客户端，配置默认选项
const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 5 * 60 * 1000, // 5分钟内数据视为新鲜
      gcTime: 10 * 60 * 1000, // 10分钟后清理缓存
      refetchOnWindowFocus: false, // 窗口聚焦时不自动刷新
      retry: 1, // 失败时只重试一次
    },
  },
});

export default function App() {
  return (
    <QueryClientProvider client={queryClient}>
      <ToastProvider>
        <AppBootstrap />
        <AppRoutes />
      </ToastProvider>
    </QueryClientProvider>
  );
}

function AppBootstrap() {
  useEffect(() => {
    useAuthStore.getState().initialize();
  }, []);

  return null;
}

function GuardedRoute({ route, session }) {
  return route.roles.includes(session?.role) ? route.element : <Navigate to="/dashboard" replace />;
}

function LoginScreen() {
  const { login } = useAuth();
  const [form, setForm] = useState({ username: '', password: '' });
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState('');

  async function handleSubmit(event) {
    event.preventDefault();
    if (!form.username.trim()) {
      setError('请输入用户名。');
      return;
    }
    if (!form.password.trim()) {
      setError('请输入密码。');
      return;
    }

    try {
      setSubmitting(true);
      setError('');
      await login({
        username: form.username.trim(),
        password: form.password,
      });
    } catch (requestError) {
      setError(requestError.message);
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <div className="login-page">
      <div className="login-bg-shapes">
        <div className="login-shape login-shape-1" />
        <div className="login-shape login-shape-2" />
        <div className="login-shape login-shape-3" />
      </div>

      <div className="login-container">
        <div className="login-brand">
          <div className="login-brand-icon">
            <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="#fff" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <path d="M12 2L2 7l10 5 10-5-10-5z" />
              <path d="M2 17l10 5 10-5" />
              <path d="M2 12l10 5 10-5" />
            </svg>
          </div>
          <div className="login-brand-text">Manger</div>
        </div>

        <div className="login-card">
          <div className="login-card-header">
            <h1 className="login-card-title">管理员登录</h1>
            <p className="login-card-sub">请输入账号和密码登录后台管理系统</p>
          </div>

          <form onSubmit={handleSubmit} className="login-card-body">
            <div className="login-field">
              <label className="login-label">用户名</label>
              <div className="login-input-wrap">
                <svg className="login-input-icon" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                  <path d="M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2" />
                  <circle cx="12" cy="7" r="4" />
                </svg>
                <input
                  className="login-input"
                  placeholder="请输入用户名"
                  value={form.username}
                  onChange={(event) => setForm((prev) => ({ ...prev, username: event.target.value }))}
                />
              </div>
            </div>

            <div className="login-field">
              <label className="login-label">密码</label>
              <div className="login-input-wrap">
                <svg className="login-input-icon" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                  <rect x="3" y="11" width="18" height="11" rx="2" ry="2" />
                  <path d="M7 11V7a5 5 0 0 1 10 0v4" />
                </svg>
                <input
                  type="password"
                  className="login-input"
                  placeholder="请输入密码"
                  value={form.password}
                  onChange={(event) => setForm((prev) => ({ ...prev, password: event.target.value }))}
                />
              </div>
            </div>

            <button className="login-btn" type="submit" disabled={submitting}>
              {submitting ? (
                <span className="login-btn-loading">
                  <span className="login-spinner" />
                  登录中...
                </span>
              ) : '登 录'}
            </button>
          </form>
        </div>

        <div className="login-footer">Manger Admin Panel</div>
      </div>

      {error && (
        <div className="login-error-overlay" onClick={() => setError('')}>
          <div className="login-error-modal" onClick={(e) => e.stopPropagation()}>
            <div className="login-error-icon">
              <svg width="28" height="28" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                <circle cx="12" cy="12" r="10" />
                <line x1="15" y1="9" x2="9" y2="15" />
                <line x1="9" y1="9" x2="15" y2="15" />
              </svg>
            </div>
            <div className="login-error-title">登录失败</div>
            <div className="login-error-message">{error}</div>
            <button className="login-error-btn" onClick={() => setError('')}>知道了</button>
          </div>
        </div>
      )}
    </div>
  );
}

function AppRoutes() {
  const { session, bootstrapLoading, logout } = useAuth();
  const navigate = useNavigate();

  useEffect(() => {
    function handleUnauthorizedEvent() {
      logout();
      navigate('/login', { replace: true });
    }
    window.addEventListener('manger:unauthorized', handleUnauthorizedEvent);
    return () => window.removeEventListener('manger:unauthorized', handleUnauthorizedEvent);
  }, [logout, navigate]);

  if (bootstrapLoading) {
    return <div style={{ minHeight: '100vh', display: 'grid', placeItems: 'center', background: 'var(--bg)', color: 'var(--text3)', fontFamily: "'Inter','Noto Sans SC',sans-serif" }}>正在加载登录状态...</div>;
  }

  return (
    <Routes>
      {publicRoutes.map((route) => (<Route key={route.path} path={route.path} element={route.element} />))}
      <Route
        path="*"
        element={session ? (
          <AppShell>
            <Routes>
              <Route path="/" element={<Navigate to="/dashboard" replace />} />
              {protectedRoutes.map((route) => (
                <Route key={route.path} path={route.path} element={<GuardedRoute route={route} session={session} />} />
              ))}
              <Route path="*" element={<NotFoundPage />} />
            </Routes>
          </AppShell>
        ) : (<LoginScreen />)}
      />
    </Routes>
  );
}

import { useNavigate } from 'react-router-dom';

export function NotFoundPage() {
  const navigate = useNavigate();

  return (
    <div className="not-found-page">
      <div className="not-found-container">
        <div className="not-found-icon">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" width="32" height="32">
            <circle cx="11" cy="11" r="8" />
            <path d="M21 21l-4.35-4.35" />
            <path d="M11 8v4" />
            <path d="M11 16h.01" />
          </svg>
        </div>
        <div className="not-found-code">404</div>
        <div className="not-found-title">页面不存在</div>
        <div className="not-found-desc">您访问的页面已被移除或从未存在。请检查链接是否正确。</div>
        <div className="not-found-actions">
          <button className="btn btn-primary" onClick={() => navigate('/dashboard', { replace: true })}>
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <path d="M3 9l9-7 9 7v11a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" />
              <polyline points="9 22 9 12 15 12 15 22" />
            </svg>
            返回首页
          </button>
          <button className="btn btn-outline" onClick={() => navigate(-1)}>
            返回上页
          </button>
        </div>
      </div>
    </div>
  );
}

import { useNavigate } from 'react-router-dom';

export function NotFoundPage() {
  const navigate = useNavigate();

  return (
    <div className="not-found-page">
      <div className="not-found-container">
        <div className="not-found-code">404</div>
        <div className="not-found-title">页面不存在</div>
        <div className="not-found-desc">您访问的页面已被移除或从未存在。</div>
        <button className="btn btn-primary" onClick={() => navigate('/dashboard', { replace: true })}>
          返回首页
        </button>
      </div>
    </div>
  );
}

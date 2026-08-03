import { PublicPageShell } from './PublicPageShell.jsx';

export function PublicPlayerReportPage() {
  return (
    <PublicPageShell>
      <div className="public-hero">
        <div className="public-hero-icon">
          <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <path d="M10.29 3.86L1.82 18a2 2 0 001.71 3h16.94a2 2 0 001.71-3L13.71 3.86a2 2 0 00-3.42 0z" />
            <path d="M12 9v4" /><path d="M12 17h.01" />
          </svg>
        </div>
        <h1>玩家举报</h1>
      </div>
      <div className="public-card">
        <div className="public-card-body" style={{ textAlign: 'center', padding: '48px 24px' }}>
          <p style={{ fontSize: 15, lineHeight: 1.8, color: 'var(--text2)', margin: 0 }}>
            该功能迁移至CNGOKZ论坛：<a href="https://chat.cngokz.com" target="_blank" rel="noopener noreferrer" style={{ color: 'var(--accent)', fontWeight: 600 }}>https://chat.cngokz.com</a>
            <br />
            请在论坛内选择好相应板块发帖求助。
          </p>
        </div>
      </div>
    </PublicPageShell>
  );
}

import { PublicPageShell } from './PublicPageShell.jsx';

export function PublicBanAppealPage() {
  return (
    <PublicPageShell>
      <div className="public-hero">
        <div className="public-hero-icon">
          <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z" />
          </svg>
        </div>
        <h1>封禁申诉</h1>
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

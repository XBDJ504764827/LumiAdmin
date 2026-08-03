import { PublicPageShell } from './PublicPageShell.jsx';

export function PublicMapFeedbackPage() {
  return (
    <PublicPageShell>
      <div className="public-hero">
        <div className="public-hero-icon">
          <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <path d="M9 20l-5.45-2.73a1 1 0 0 1-.55-.9V4.38a1 1 0 0 1 1.45-.9L9 6m0 14V6m0 14l6-3m0-11L9 6m6 11l4.55 2.27a1 1 0 0 0 1.45-.9V5.18a1 1 0 0 0-.55-.9L15 2M15 17V2m0 15l-6-3" />
          </svg>
        </div>
        <h1>地图反馈</h1>
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

import { useState } from 'react';
import { useApiQuery } from '../../shared/useApiQuery.js';
import { api } from '../../lib/api.js';
import { PublicPageShell } from './PublicPageShell.jsx';
import { SearchBar } from '../../shared/SearchBar.jsx';
import { Pagination } from '../../shared/Pagination.jsx';

function formatExpiry(item) {
  if (item.status === 'inactive') return 'expired';
  if (item.status !== 'active') return item.status;
  if (item.duration_minutes === 0) return 'permanent';
  if (item.expires_at && new Date(item.expires_at) < new Date()) return 'expired';
  return 'active';
}

function banStatusLabel(item) {
  const state = formatExpiry(item);
  if (state === 'permanent') return '永久封禁';
  if (state === 'expired') return '已过期';
  return '封禁中';
}

function banStatusPill(item) {
  const state = formatExpiry(item);
  if (state === 'permanent') return 'pill-danger';
  if (state === 'expired') return 'pill-offline';
  return 'pill-warning';
}

export function PublicBanPage() {
  const [search, setSearch] = useState('');
  const [page, setPage] = useState(1);

  const { data, isLoading, error, refetch } = useApiQuery(
    ['publicBans', { page, search }],
    () => api.publicBans({ page, page_size: 20, ...(search ? { search } : {}) }),
    { enabled: true },
  );

  const items = data?.items ?? [];
  const total = data?.total ?? 0;

  function renderContent() {
    if (isLoading) return (
      <div className="public-loading">
        <div className="public-loading-spinner" />
        正在加载封禁记录...
      </div>
    );

    if (error) return (
      <div className="public-error">
        加载封禁记录失败
        <div><button className="public-error-retry" onClick={() => refetch()}>重新加载</button></div>
      </div>
    );

    if (items.length === 0) return (
      <div className="public-empty">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
          <circle cx="12" cy="12" r="10" /><line x1="4.93" y1="4.93" x2="19.07" y2="19.07" />
        </svg>
        <div className="public-empty-text">暂无封禁记录</div>
        <div className="public-empty-hint">服务器目前没有封禁的玩家</div>
      </div>
    );

    return (
      <>
        <div className="table-responsive">
          <table className="public-table">
            <thead>
              <tr>
                <th>玩家名称</th>
                <th>Steam 标识符</th>
                <th>所在服务器</th>
                <th>封禁状态</th>
                <th>封禁缘由</th>
              </tr>
            </thead>
            <tbody>
              {items.map((x) => (
                <tr key={x.id}>
                  <td className="fw-600">{x.player}</td>
                  <td className="steam-id">{x.steam_id}</td>
                  <td className="text-muted">{x.server_name ?? '未记录'}</td>
                  <td><span className={`status-pill ${banStatusPill(x)}`}>{banStatusLabel(x)}</span></td>
                  <td style={{ color: formatExpiry(x) === 'expired' ? 'var(--text3)' : 'var(--text2)', fontWeight: 500 }}>{x.reason}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
        <Pagination page={page} pageSize={20} total={total} onChange={setPage} />
      </>
    );
  }

  return (
    <PublicPageShell>
      <div className="public-hero">
        <div className="public-hero-icon" style={{ background: 'linear-gradient(135deg, var(--danger-dot), var(--accent))' }}>
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <circle cx="12" cy="12" r="10" /><line x1="4.93" y1="4.93" x2="19.07" y2="19.07" />
          </svg>
        </div>
        <h1>封禁公示</h1>
        <p>为维护良好的游戏环境，违规玩家将被公示在此处。封禁状态包括封禁中、永久封禁和已过期。</p>
      </div>

      {!isLoading && data && (
        <div className="public-stats">
          <div className="public-stat">
            <span className="public-stat-value" style={{ color: 'var(--warning-text)' }}>{data.stats?.active ?? 0}</span>
            <span className="public-stat-label">封禁中</span>
          </div>
          <div className="public-stat">
            <span className="public-stat-value" style={{ color: 'var(--danger-text)' }}>{data.stats?.permanent ?? 0}</span>
            <span className="public-stat-label">永久封禁</span>
          </div>
          <div className="public-stat">
            <span className="public-stat-value text-muted-light">{data.stats?.expired ?? 0}</span>
            <span className="public-stat-label">已过期</span>
          </div>
          <div className="public-stat">
            <span className="public-stat-value">{total}</span>
            <span className="public-stat-label">总记录</span>
          </div>
        </div>
      )}

      <div className="mb-16">
        <SearchBar
          value={search}
          onChange={(v) => { setSearch(v); setPage(1); }}
          placeholder="搜索玩家名称 / SteamID..."
        />
      </div>

      <div className="public-card">
        <div style={{ overflow: 'hidden' }}>
          {renderContent()}
        </div>
      </div>
    </PublicPageShell>
  );
}

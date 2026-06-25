import { useState } from 'react';
import { useApiQuery } from '../../shared/useApiQuery.js';
import { publicApi } from '../../lib/publicApi.js';
import { PublicPageShell } from './PublicPageShell.jsx';
import { SearchBar } from '../../shared/SearchBar.jsx';
import { Pagination } from '../../shared/Pagination.jsx';
import { formatChinaDateTime } from '../../shared/time.js';

export function PublicWhitelistPage() {
  const [search, setSearch] = useState('');
  const [page, setPage] = useState(1);

  const { data, isLoading, error, refetch } = useApiQuery(
    ['publicWhitelist', { page, search }],
    () => publicApi.publicWhitelist({ page, page_size: 20, ...(search ? { search } : {}) }),
    { auth: false },
  );

  const items = data?.items ?? [];
  const total = data?.total ?? 0;

  function renderContent() {
    if (isLoading) return (
      <div className="public-loading">
        <div className="public-loading-spinner" />
        正在加载白名单...
      </div>
    );

    if (error) return (
      <div className="public-error">
        加载白名单失败
        <div><button className="public-error-retry" onClick={() => refetch()}>重新加载</button></div>
      </div>
    );

    if (items.length === 0) return (
      <div className="public-empty">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
          <path d="M17 21v-2a4 4 0 0 0-4-4H5a4 4 0 0 0-4 4v2" /><circle cx="9" cy="7" r="4" /><path d="M23 21v-2a4 4 0 0 0-3-3.87" /><path d="M16 3.13a4 4 0 0 1 0 7.75" />
        </svg>
        <div className="public-empty-text">暂无白名单记录</div>
        <div className="public-empty-hint">目前还没有已通过的白名单玩家</div>
      </div>
    );

    return (
      <>
        <div className="table-responsive">
          <table className="public-table">
            <thead>
              <tr>
                <th>游戏昵称</th>
                <th>SteamID64</th>
                <th>申请时间</th>
                <th>通过时间</th>
              </tr>
            </thead>
            <tbody>
              {items.map((item) => (
                <tr key={item.id}>
                  <td className="fw-600">{item.nickname}</td>
                  <td className="steam-id">{item.steamid64}</td>
                  <td className="text-muted-light">{formatChinaDateTime(item.applied_at, { seconds: false })}</td>
                  <td>
                    {item.approved_at
                      ? <span style={{ color: 'var(--teal)' }}>{formatChinaDateTime(item.approved_at, { seconds: false })}</span>
                      : <span className="text-muted-light">-</span>}
                  </td>
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
        <div className="public-hero-icon" style={{ background: 'linear-gradient(135deg, var(--teal), #7dd3c0)' }}>
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <path d="M17 21v-2a4 4 0 0 0-4-4H5a4 4 0 0 0-4 4v2" /><circle cx="9" cy="7" r="4" /><path d="M23 21v-2a4 4 0 0 0-3-3.87" /><path d="M16 3.13a4 4 0 0 1 0 7.75" />
          </svg>
        </div>
        <h1>白名单公示</h1>
        <p>查看当前已通过审核的白名单玩家，所有信息实时更新。</p>
      </div>

      {!isLoading && data && (
        <div className="public-stats">
          <div className="public-stat">
            <span className="public-stat-value" style={{ color: 'var(--teal)' }}>{total}</span>
            <span className="public-stat-label">已通过玩家</span>
          </div>
        </div>
      )}

      <div className="mb-16">
        <SearchBar
          value={search}
          onChange={(v) => { setSearch(v); setPage(1); }}
          placeholder="搜索 SteamID64 / 昵称..."
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

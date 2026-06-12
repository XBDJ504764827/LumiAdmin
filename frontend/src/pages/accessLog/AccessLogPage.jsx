import React, { useState } from 'react';
import { api } from '../../lib/api.js';
import { useApiQuery } from '../../shared/useApiQuery.js';
import { useAuth } from '../../state/auth.jsx';
import { Pagination } from '../../shared/Pagination.jsx';
import { formatChinaDateTime } from '../../shared/time.js';
import { StatusPill } from '../../shared/StatusPill.jsx';

const ACCESS_METHOD_MAP = {
  unrestricted: { label: '无限制', kind: 'success' },
  whitelist: { label: '白名单', kind: 'success' },
  restriction: { label: 'Rating 限制', kind: 'info' },
  custom_rule: { label: '自定义规则', kind: 'info' },
  custom_rule_blocked: { label: '规则拒绝', kind: 'danger' },
};

function methodLabel(method) {
  return ACCESS_METHOD_MAP[method]?.label || method || '-';
}

function methodKind(method) {
  return ACCESS_METHOD_MAP[method]?.kind || 'default';
}

export function AccessLogPage() {
  const { session } = useAuth();
  const token = session?.token ?? null;
  const [page, setPage] = useState(1);
  const [steamFilter, setSteamFilter] = useState('');
  const [serverFilter, setServerFilter] = useState('');

  const queryBody = { page, page_size: 30 };
  if (steamFilter.trim()) queryBody.steam_id64 = steamFilter.trim();
  if (serverFilter.trim()) queryBody.server_id = serverFilter.trim();

  const { data, isLoading, error } = useApiQuery(
    ['accessLogs', queryBody],
    (token) => api.accessLogs(token, queryBody),
    typeof session !== 'undefined',
  );

  const items = data?.items || [];
  const total = data?.total || 0;

  function handleSearch(e) {
    e.preventDefault();
    setPage(1);
  }

  return (
    <div className="content-section active">
      <div className="breadcrumb">
        <span>核心管理</span><span className="sep">›</span>
        <span className="current">进服监控</span>
      </div>
      <div className="page-header">
        <div>
          <div className="page-title">进服监控</div>
          <div className="page-sub">记录玩家每次进入服务器的详细信息：进服方式、时间、IP、服务器等。</div>
        </div>
      </div>

      <div className="card" style={{ marginBottom: 16 }}>
        <div className="card-body">
          <form onSubmit={handleSearch} style={{ display: 'flex', gap: 8, flexWrap: 'wrap' }}>
            <input
              className="form-control"
              style={{ maxWidth: 260 }}
              placeholder="SteamID64"
              value={steamFilter}
              onChange={(e) => setSteamFilter(e.target.value)}
            />
            <input
              className="form-control"
              style={{ maxWidth: 320 }}
              placeholder="服务器 ID"
              value={serverFilter}
              onChange={(e) => setServerFilter(e.target.value)}
            />
            <button className="btn btn-primary" type="submit">查询</button>
            {(steamFilter || serverFilter) ? (
              <button className="btn btn-outline" type="button" onClick={() => { setSteamFilter(''); setServerFilter(''); setPage(1); }}>
                清除
              </button>
            ) : null}
          </form>
        </div>
      </div>

      {isLoading && <div style={{ padding: 24, color: 'var(--text3)' }}>加载中...</div>}
      {error && <div style={{ padding: 24, color: 'var(--accent)' }}>加载失败: {error.message}</div>}

      {!isLoading && !error ? (
        <>
          <div className="table-responsive">
            <table className="data-table">
              <thead>
                <tr>
                  <th>时间</th>
                  <th>玩家</th>
                  <th>SteamID64</th>
                  <th>IP</th>
                  <th>服务器</th>
                  <th>进服方式</th>
                  <th>Rating</th>
                  <th>Steam 等级</th>
                </tr>
              </thead>
              <tbody>
                {items.length === 0 ? (
                  <tr><td colSpan={8} style={{ textAlign: 'center', color: 'var(--text3)' }}>暂无进服记录</td></tr>
                ) : (
                  items.map((item) => (
                    <tr key={item.id}>
                      <td style={{ whiteSpace: 'nowrap' }}>{formatChinaDateTime(item.created_at, { seconds: false })}</td>
                      <td className="fw-600">{item.player_name || '-'}</td>
                      <td><code style={{ fontSize: 12 }}>{item.steam_id64}</code></td>
                      <td><code style={{ fontSize: 12 }}>{item.ip_address || '-'}</code></td>
                      <td>{item.server_name} :{item.server_port}</td>
                      <td><StatusPill kind={methodKind(item.access_method)}>{methodLabel(item.access_method)}</StatusPill></td>
                      <td>{item.rating ?? '-'}</td>
                      <td>{item.steam_level ?? '-'}</td>
                    </tr>
                  ))
                )}
              </tbody>
            </table>
          </div>
          {total > 0 ? (
            <Pagination
              page={page}
              pageSize={30}
              total={total}
              onPageChange={setPage}
            />
          ) : null}
        </>
      ) : null}
    </div>
  );
}

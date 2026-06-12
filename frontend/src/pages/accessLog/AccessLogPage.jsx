import React, { useState } from 'react';
import { api } from '../../lib/api.js';
import { useApiQuery } from '../../shared/useApiQuery.js';
import { useAuth } from '../../state/auth.jsx';
import { Pagination } from '../../shared/Pagination.jsx';
import { formatChinaDateTime } from '../../shared/time.js';
import { StatusPill } from '../../shared/StatusPill.jsx';

const ACCESS_METHOD_MAP = {
  // 进服成功
  unrestricted: { label: '无限制', kind: 'success' },
  whitelist: { label: '白名单', kind: 'success' },
  restriction: { label: 'Rating 限制', kind: 'success' },
  custom_rule: { label: '自定义规则', kind: 'success' },
  snapshot_fallback: { label: '快照回退', kind: 'default' },
  // 进服失败
  banned: { label: '被封禁', kind: 'danger' },
  whitelist_rejected: { label: '白名单未通过', kind: 'danger' },
  restriction_rejected: { label: 'Rating 不足', kind: 'danger' },
  custom_rule_rejected: { label: '规则拒绝', kind: 'danger' },
};

function methodLabel(method) {
  return ACCESS_METHOD_MAP[method]?.label || method || '-';
}

function methodKind(method) {
  return ACCESS_METHOD_MAP[method]?.kind || 'default';
}

const ACCESS_METHOD_OPTIONS = [
  { value: '', label: '全部方式' },
  { value: 'unrestricted', label: '无限制' },
  { value: 'whitelist', label: '白名单' },
  { value: 'restriction', label: 'Rating 限制' },
  { value: 'custom_rule', label: '自定义规则' },
  { value: 'banned', label: '被封禁' },
  { value: 'whitelist_rejected', label: '白名单未通过' },
  { value: 'restriction_rejected', label: 'Rating 不足' },
  { value: 'custom_rule_rejected', label: '规则拒绝' },
];

const ALLOWED_OPTIONS = [
  { value: '', label: '全部状态' },
  { value: 'true', label: '进服成功' },
  { value: 'false', label: '进服失败' },
];

export function AccessLogPage() {
  const { session } = useAuth();
  const token = session?.token ?? null;
  const [page, setPage] = useState(1);
  const [filters, setFilters] = useState({
    steam_id64: '',
    server_id: '',
    community_id: '',
    access_method: '',
    allowed: '',
    search: '',
  });

  const queryBody = { page, page_size: 30 };
  if (filters.steam_id64.trim()) queryBody.steam_id64 = filters.steam_id64.trim();
  if (filters.server_id.trim()) queryBody.server_id = filters.server_id.trim();
  if (filters.community_id.trim()) queryBody.community_id = filters.community_id.trim();
  if (filters.access_method) queryBody.access_method = filters.access_method;
  if (filters.allowed !== '') queryBody.allowed = filters.allowed === 'true';
  if (filters.search.trim()) queryBody.search = filters.search.trim();

  const { data, isLoading, error } = useApiQuery(
    ['accessLogs', queryBody],
    (token) => api.accessLogs(token, queryBody),
    typeof session !== 'undefined',
  );

  const items = data?.items || [];
  const total = data?.total || 0;
  const hasFilters = Object.values(filters).some((v) => v);

  function handleSearch(e) {
    e.preventDefault();
    setPage(1);
  }

  function clearFilters() {
    setFilters({ steam_id64: '', server_id: '', community_id: '', access_method: '', allowed: '', search: '' });
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
          <div className="page-sub">记录玩家每次尝试进入服务器的结果：进服成功/失败、进服方式、拒绝原因等。</div>
        </div>
      </div>

      <div className="card" style={{ marginBottom: 16 }}>
        <div className="card-body">
          <form onSubmit={handleSearch} style={{ display: 'flex', gap: 8, flexWrap: 'wrap', alignItems: 'center' }}>
            <select
              className="form-control"
              style={{ maxWidth: 120 }}
              value={filters.allowed}
              onChange={(e) => setFilters((f) => ({ ...f, allowed: e.target.value }))}
            >
              {ALLOWED_OPTIONS.map((o) => (
                <option key={o.value} value={o.value}>{o.label}</option>
              ))}
            </select>
            <input
              className="form-control"
              style={{ maxWidth: 220 }}
              placeholder="SteamID64"
              value={filters.steam_id64}
              onChange={(e) => setFilters((f) => ({ ...f, steam_id64: e.target.value }))}
            />
            <input
              className="form-control"
              style={{ maxWidth: 280 }}
              placeholder="服务器 ID"
              value={filters.server_id}
              onChange={(e) => setFilters((f) => ({ ...f, server_id: e.target.value }))}
            />
            <input
              className="form-control"
              style={{ maxWidth: 280 }}
              placeholder="社区组 ID"
              value={filters.community_id}
              onChange={(e) => setFilters((f) => ({ ...f, community_id: e.target.value }))}
            />
            <select
              className="form-control"
              style={{ maxWidth: 140 }}
              value={filters.access_method}
              onChange={(e) => setFilters((f) => ({ ...f, access_method: e.target.value }))}
            >
              {ACCESS_METHOD_OPTIONS.map((o) => (
                <option key={o.value} value={o.value}>{o.label}</option>
              ))}
            </select>
            <input
              className="form-control"
              style={{ maxWidth: 200 }}
              placeholder="搜索玩家/服务器..."
              value={filters.search}
              onChange={(e) => setFilters((f) => ({ ...f, search: e.target.value }))}
            />
            <button className="btn btn-primary" type="submit">查询</button>
            {hasFilters && (
              <button className="btn btn-outline" type="button" onClick={clearFilters}>
                清除
              </button>
            )}
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
                  <th>结果</th>
                  <th>玩家</th>
                  <th>SteamID64</th>
                  <th>IP</th>
                  <th>服务器</th>
                  <th>社区组</th>
                  <th>进服方式</th>
                  <th>拒绝原因</th>
                  <th>Rating</th>
                  <th>Steam 等级</th>
                </tr>
              </thead>
              <tbody>
                {items.length === 0 ? (
                  <tr><td colSpan={11} style={{ textAlign: 'center', color: 'var(--text3)' }}>暂无进服记录</td></tr>
                ) : (
                  items.map((item) => (
                    <tr key={item.id} style={!item.allowed ? { opacity: 0.7 } : undefined}>
                      <td style={{ whiteSpace: 'nowrap' }}>{formatChinaDateTime(item.created_at, { seconds: false })}</td>
                      <td>
                        <StatusPill kind={item.allowed ? 'success' : 'danger'}>
                          {item.allowed ? '成功' : '失败'}
                        </StatusPill>
                      </td>
                      <td className="fw-600">{item.player_name || '-'}</td>
                      <td><code style={{ fontSize: 12 }}>{item.steam_id64}</code></td>
                      <td><code style={{ fontSize: 12 }}>{item.ip_address || '-'}</code></td>
                      <td>{item.server_name} :{item.server_port}</td>
                      <td>{item.community_name || '-'}</td>
                      <td><StatusPill kind={methodKind(item.access_method)}>{methodLabel(item.access_method)}</StatusPill></td>
                      <td style={{ maxWidth: 200, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }} title={item.reject_reason || ''}>
                        {item.reject_reason || '-'}
                      </td>
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

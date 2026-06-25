import { useState } from 'react';
import { api } from '../../lib/api.js';
import { useApiQuery } from '../../shared/useApiQuery.js';
import { useAuth } from '../../state/store.js';
import { Pagination } from '../../shared/Pagination.jsx';
import { formatChinaDateTime } from '../../shared/time.js';
import { StatusPill } from '../../shared/StatusPill.jsx';
import { TableLoading, TableEmpty } from '../../shared/TableState.jsx';

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

const FAILURE_CODE_MAP = {
  banned: '被封禁',
  global_banned: '全球封禁',
  linked_ip_banned: '同 IP 关联封禁',
  not_whitelisted: '白名单未通过',
  low_rating: 'Rating 不足',
  low_steam_level: 'Steam 等级不足',
  custom_rule_rejected: '自定义规则拒绝',
  profile_fetch_failed: '无法获取玩家资料',
  snapshot_unavailable: '服务降级',
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
  { value: 'banned', label: '被封禁' },
  { value: 'whitelist_rejected', label: '白名单未通过' },
  { value: 'restriction_rejected', label: 'Rating 不足' },
];

const ALLOWED_OPTIONS = [
  { value: '', label: '全部状态' },
  { value: 'true', label: '进服成功' },
  { value: 'false', label: '进服失败' },
];

const FAILURE_CODE_OPTIONS = [
  { value: '', label: '全部原因' },
  ...Object.entries(FAILURE_CODE_MAP).map(([k, v]) => ({ value: k, label: v })),
];

const COL_COUNT = 11;

export function AccessLogPage() {
  const { session } = useAuth();
  const _token = session?.token ?? null;
  const [page, setPage] = useState(1);
  const [filters, setFilters] = useState({
    steam_id64: '',
    server_id: '',
    community_id: '',
    access_method: '',
    allowed: '',
    failure_code: '',
    ip_address: '',
    search: '',
  });

  // 构建 GET 查询参数
  const queryParams = { page, page_size: 30 };
  if (filters.steam_id64.trim()) queryParams.steam_id64 = filters.steam_id64.trim();
  if (filters.server_id.trim()) queryParams.server_id = filters.server_id.trim();
  if (filters.community_id.trim()) queryParams.community_id = filters.community_id.trim();
  if (filters.access_method) queryParams.access_method = filters.access_method;
  if (filters.allowed !== '') queryParams.allowed = filters.allowed === 'true';
  if (filters.failure_code) queryParams.failure_code = filters.failure_code;
  if (filters.ip_address.trim()) queryParams.ip_address = filters.ip_address.trim();
  if (filters.search.trim()) queryParams.search = filters.search.trim();

  const { data, isLoading, error } = useApiQuery(
    ['accessLogs', queryParams],
    (token) => api.accessLogs(token, queryParams),
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
    setFilters({ steam_id64: '', server_id: '', community_id: '', access_method: '', allowed: '', failure_code: '', ip_address: '', search: '' });
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
          <div className="page-sub">记录玩家每次尝试进入服务器的结果：进服成功/失败、进服方式、失败原因等。</div>
        </div>
      </div>

      <div className="card" style={{ marginBottom: 16 }}>
        <div className="filter-bar">
          <select
            className="filter-select"
            value={filters.allowed}
            onChange={(e) => { setFilters((f) => ({ ...f, allowed: e.target.value })); setPage(1); }}
          >
            {ALLOWED_OPTIONS.map((o) => (
              <option key={o.value} value={o.value}>{o.label}</option>
            ))}
          </select>
          <input
            className="search-bar-input"
            style={{ maxWidth: 180 }}
            placeholder="SteamID64"
            value={filters.steam_id64}
            onChange={(e) => setFilters((f) => ({ ...f, steam_id64: e.target.value }))}
          />
          <input
            className="search-bar-input"
            style={{ maxWidth: 180 }}
            placeholder="服务器 ID"
            value={filters.server_id}
            onChange={(e) => setFilters((f) => ({ ...f, server_id: e.target.value }))}
          />
          <input
            className="search-bar-input"
            style={{ maxWidth: 180 }}
            placeholder="社区组 ID"
            value={filters.community_id}
            onChange={(e) => setFilters((f) => ({ ...f, community_id: e.target.value }))}
          />
          <input
            className="search-bar-input"
            style={{ maxWidth: 160 }}
            placeholder="IP 地址"
            value={filters.ip_address}
            onChange={(e) => setFilters((f) => ({ ...f, ip_address: e.target.value }))}
          />
          <select
            className="filter-select"
            value={filters.access_method}
            onChange={(e) => { setFilters((f) => ({ ...f, access_method: e.target.value })); setPage(1); }}
          >
            {ACCESS_METHOD_OPTIONS.map((o) => (
              <option key={o.value} value={o.value}>{o.label}</option>
            ))}
          </select>
          <select
            className="filter-select"
            value={filters.failure_code}
            onChange={(e) => { setFilters((f) => ({ ...f, failure_code: e.target.value })); setPage(1); }}
          >
            {FAILURE_CODE_OPTIONS.map((o) => (
              <option key={o.value} value={o.value}>{o.label}</option>
            ))}
          </select>
          <input
            className="search-bar-input"
            style={{ maxWidth: 200 }}
            placeholder="搜索玩家/服务器..."
            value={filters.search}
            onChange={(e) => setFilters((f) => ({ ...f, search: e.target.value }))}
          />
          <button className="btn btn-primary" type="submit" onClick={handleSearch}>查询</button>
          {hasFilters && (
            <button className="btn btn-outline" type="button" onClick={clearFilters}>清除</button>
          )}
        </div>
      </div>

      <div className="card">
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
                <th>失败原因</th>
                <th>Rating</th>
                <th>Steam 等级</th>
              </tr>
            </thead>
            <tbody>
              {isLoading ? (
                <TableLoading colSpan={COL_COUNT} text="正在加载进服记录..." />
              ) : error ? (
                <tr><td colSpan={COL_COUNT} className="table-state-cell">
                  <div className="table-state-inner table-state-inner--error">加载失败: {error.message}</div>
                </td></tr>
              ) : items.length === 0 ? (
                <TableEmpty colSpan={COL_COUNT} text="暂无进服记录" />
              ) : (
                items.map((item) => (
                  <tr key={item.id} className={!item.allowed ? 'row-access-denied' : undefined}>
                    <td style={{ whiteSpace: 'nowrap' }}>{formatChinaDateTime(item.created_at, { seconds: false })}</td>
                    <td>
                      <StatusPill kind={item.allowed ? 'success' : 'danger'}>
                        {item.allowed ? '成功' : '失败'}
                      </StatusPill>
                    </td>
                    <td className="fw-600">{item.player_name || '-'}</td>
                    <td className="steam-id">{item.steam_id64}</td>
                    <td className="steam-id">{item.ip_address || '-'}</td>
                    <td>{item.server_name} :{item.server_port}</td>
                    <td>{item.community_name || '-'}</td>
                    <td><StatusPill kind={methodKind(item.access_method)}>{methodLabel(item.access_method)}</StatusPill></td>
                    <td style={{ maxWidth: 180, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }} title={item.reject_reason || ''}>
                      {item.failure_code
                        ? (FAILURE_CODE_MAP[item.failure_code] || item.failure_code)
                        : (item.reject_reason || '-')}
                    </td>
                    <td>{item.rating ?? '-'}</td>
                    <td>{item.steam_level ?? '-'}</td>
                  </tr>
                ))
              )}
            </tbody>
          </table>
        </div>
      </div>

      <Pagination
        page={page}
        pageSize={30}
        total={total}
        onChange={setPage}
      />
    </div>
  );
}

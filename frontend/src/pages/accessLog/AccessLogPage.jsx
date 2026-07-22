import { useState } from 'react';
import { api } from '../../lib/api.js';
import { useApiQuery } from '../../shared/useApiQuery.js';
import { useAuth } from '../../state/store.js';
import { Pagination } from '../../shared/Pagination.jsx';
import { formatChinaDateTime } from '../../shared/time.js';
import { StatusPill } from '../../shared/StatusPill.jsx';
import { TableLoading, TableError, TableEmpty } from '../../shared/TableState.jsx';
import { FilterToolbar } from '../../shared/SearchBar.jsx';

const ACCESS_METHOD_MAP = {
  // 进服成功
  unrestricted: { label: '无限制', kind: 'success' },
  whitelist: { label: '白名单', kind: 'success' },
  restriction: { label: 'Rating 限制', kind: 'success' },
  cs_prime: { label: 'CS优先账户', kind: 'success' },
  custom_rule: { label: '自定义规则', kind: 'success' },
  snapshot_fallback: { label: '快照回退', kind: 'default' },
  // 进服失败
  banned: { label: '被封禁', kind: 'danger' },
  whitelist_rejected: { label: '白名单未通过', kind: 'danger' },
  restriction_rejected: { label: 'Rating 不足', kind: 'danger' },
  cs_prime_rejected: { label: '非CS优先账户', kind: 'danger' },
  custom_rule_rejected: { label: '规则拒绝', kind: 'danger' },
};

const FAILURE_CODE_MAP = {
  banned: '被封禁',
  global_banned: '全球封禁',
  linked_ip_banned: '同 IP 关联封禁',
  not_whitelisted: '白名单未通过',
  low_rating: 'Rating 不足',
  low_steam_level: 'Steam 等级不足',
  not_cs_prime: '非 CS 优先账户',
  custom_rule_rejected: '自定义规则拒绝',
  profile_fetch_failed: '无法获取玩家资料',
  prime_verification_failed: '无法验证 CS 优先账户',
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
  { value: 'cs_prime', label: 'CS优先账户' },
  { value: 'banned', label: '被封禁' },
  { value: 'whitelist_rejected', label: '白名单未通过' },
  { value: 'restriction_rejected', label: 'Rating 不足' },
  { value: 'cs_prime_rejected', label: '非CS优先账户' },
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

  function handleSearch(e) {
    e?.preventDefault();
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

      <FilterToolbar
        search={filters.search}
        onSearchChange={(value) => setFilters((f) => ({ ...f, search: value }))}
        searchPlaceholder="搜索玩家 / 服务器..."
        autoSubmit={false}
        onSubmit={() => handleSearch()}
        onReset={clearFilters}
        activeCount={Object.values(filters).filter(Boolean).length}
        filters={[
          {
            key: 'allowed',
            type: 'select',
            value: filters.allowed,
            options: ALLOWED_OPTIONS,
            onChange: (value) => { setFilters((f) => ({ ...f, allowed: value ?? '' })); setPage(1); },
          },
          {
            key: 'steam_id64',
            placeholder: 'SteamID64',
            value: filters.steam_id64,
            onChange: (value) => setFilters((f) => ({ ...f, steam_id64: value })),
          },
          {
            key: 'server_id',
            placeholder: '服务器 ID',
            value: filters.server_id,
            onChange: (value) => setFilters((f) => ({ ...f, server_id: value })),
          },
          {
            key: 'community_id',
            placeholder: '社区组 ID',
            value: filters.community_id,
            onChange: (value) => setFilters((f) => ({ ...f, community_id: value })),
          },
          {
            key: 'ip_address',
            placeholder: 'IP 地址',
            value: filters.ip_address,
            onChange: (value) => setFilters((f) => ({ ...f, ip_address: value })),
          },
          {
            key: 'access_method',
            type: 'select',
            value: filters.access_method,
            options: ACCESS_METHOD_OPTIONS,
            onChange: (value) => { setFilters((f) => ({ ...f, access_method: value ?? '' })); setPage(1); },
          },
          {
            key: 'failure_code',
            type: 'select',
            value: filters.failure_code,
            options: FAILURE_CODE_OPTIONS,
            onChange: (value) => { setFilters((f) => ({ ...f, failure_code: value ?? '' })); setPage(1); },
          },
        ]}
      />

      <div className="card">
        <div className="table-responsive">
          <table className="data-table mobile-card-table">
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
                <TableError colSpan={COL_COUNT} message={`加载失败: ${error.message}`} />
              ) : items.length === 0 ? (
                <TableEmpty colSpan={COL_COUNT} text="暂无进服记录" />
              ) : (
                items.map((item) => (
                  <tr key={item.id} className={!item.allowed ? 'row-access-denied' : undefined}>
                    <td className="text-muted-light" style={{ whiteSpace: 'nowrap' }} data-label="时间">{formatChinaDateTime(item.created_at, { seconds: false })}</td>
                    <td data-label="结果">
                      <StatusPill kind={item.allowed ? 'success' : 'danger'}>
                        {item.allowed ? '成功' : '失败'}
                      </StatusPill>
                    </td>
                    <td className="fw-600 mobile-card-primary" data-label="玩家">{item.player_name || '-'}</td>
                    <td className="steam-id" data-label="SteamID64">{item.steam_id64}</td>
                    <td className="steam-id" data-label="IP">{item.ip_address || '-'}</td>
                    <td data-label="服务器">{item.server_name} :{item.server_port}</td>
                    <td data-label="社区组">{item.community_name || '-'}</td>
                    <td data-label="进服方式"><StatusPill kind={methodKind(item.access_method)}>{methodLabel(item.access_method)}</StatusPill></td>
                    <td data-label="失败原因" style={{ maxWidth: 180, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }} title={item.reject_reason || ''}>
                      {item.failure_code
                        ? (FAILURE_CODE_MAP[item.failure_code] || item.failure_code)
                        : (item.reject_reason || '-')}
                    </td>
                    <td data-label="Rating">{item.rating ?? '-'}</td>
                    <td data-label="Steam 等级">{item.steam_level ?? '-'}</td>
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

import { useState } from 'react';
import { api } from '../../lib/api.js';
import { useApiQuery } from '../../shared/useApiQuery.js';
import { SearchBar } from '../../shared/SearchBar.jsx';
import { Pagination } from '../../shared/Pagination.jsx';
import { TableLoading, TableError, TableEmpty } from '../../shared/TableState.jsx';
import { formatChinaDateTime } from '../../shared/time.js';

const OPERATION_LABELS = {
  ban: '封禁',
  ban_update: '编辑封禁',
  ban_delete: '删除封禁',
  ban_file_upload: '上传封禁附件',
  unban: '解封',
  whitelist_add: '添加白名单',
  whitelist_approve: '通过白名单',
  whitelist_reject: '拒绝白名单',
  whitelist_restore: '恢复白名单',
  whitelist_revoke: '撤销白名单',
  whitelist_remove: '移除白名单',
  whitelist_refresh_names: '刷新白名单名称',
  global_ban_sync: '同步全球封禁',
  global_ban_unban: '全球封禁解封',
  external_ban_sync: '外部封禁同步',
  external_ban_retry: '重试外部同步',
};

const SOURCE_LABELS = {
  web: '网站',
  game_plugin: '游戏插件',
  offline_sync: '离线同步',
  manual: '手动操作',
  manual_sync: '手动同步',
  manual_retry: '手动重试',
};

function detailsPreview(details) {
  if (!details || typeof details !== 'object') return null;
  const entries = Object.entries(details)
    .filter(([, value]) => value !== null && value !== undefined && value !== '')
    .slice(0, 3);
  if (entries.length === 0) return null;
  return entries.map(([key, value]) => `${key}: ${typeof value === 'object' ? JSON.stringify(value) : value}`).join(' · ');
}

export function AuditPage() {
  const [search, setSearch] = useState('');
  const [operationFilter, setOperationFilter] = useState('');
  const [sourceFilter, setSourceFilter] = useState('');
  const [successFilter, setSuccessFilter] = useState('');
  const [page, setPage] = useState(1);

  const buildParams = () => {
    const params = { page, page_size: 20 };
    if (search) params.search = search;
    if (operationFilter) params.operation = operationFilter;
    if (sourceFilter) params.source = sourceFilter;
    if (successFilter) params.success = successFilter === 'true';
    return params;
  };

  const { data, isLoading, error } = useApiQuery(
    ['auditLogs', { page, search, operationFilter, sourceFilter, successFilter }],
    (token) => api.auditLogs(token, buildParams()),
  );

  const items = data?.items ?? [];
  const total = data?.total ?? 0;

  function handleFilterChange(filterType, value) {
    setPage(1);
    switch (filterType) {
      case 'operation': setOperationFilter(value); break;
      case 'source': setSourceFilter(value); break;
      case 'success': setSuccessFilter(value); break;
    }
  }

  return (
    <div id="audit" className="content-section active">
      <div className="breadcrumb"><span>系统管理</span><span className="sep">›</span><span className="current">审计日志</span></div>
      <div className="page-header">
        <div>
          <div className="page-title">审计日志</div>
          <div className="page-sub">记录所有封禁、解封、白名单操作，包括网站和游戏内操作。</div>
        </div>
      </div>

      <div className="card">
        <div className="card-header">
          <div>
            <div className="card-title">操作记录</div>
            <div className="card-sub">全量审计日志</div>
          </div>
        </div>
        <div className="card-body p-0">
          {/* 过滤栏 */}
          <div className="filter-bar">
            <SearchBar
              value={search}
              onChange={(v) => { setSearch(v); setPage(1); }}
              placeholder="搜索目标、玩家、操作人、IP、原因..."
            />
            <select
              className="filter-select"
              value={operationFilter}
              onChange={(e) => handleFilterChange('operation', e.target.value)}
            >
              <option value="">全部操作</option>
              <option value="ban">封禁</option>
              <option value="ban_update">编辑封禁</option>
              <option value="ban_delete">删除封禁</option>
              <option value="ban_file_upload">上传封禁附件</option>
              <option value="unban">解封</option>
              <option value="whitelist_add">添加白名单</option>
              <option value="whitelist_approve">通过白名单</option>
              <option value="whitelist_reject">拒绝白名单</option>
              <option value="whitelist_restore">恢复白名单</option>
              <option value="whitelist_revoke">撤销白名单</option>
              <option value="whitelist_remove">移除白名单</option>
              <option value="whitelist_refresh_names">刷新白名单名称</option>
              <option value="global_ban_sync">同步全球封禁</option>
              <option value="global_ban_unban">全球封禁解封</option>
              <option value="external_ban_sync">外部封禁同步</option>
              <option value="external_ban_retry">重试外部同步</option>
            </select>
            <select
              className="filter-select"
              value={sourceFilter}
              onChange={(e) => handleFilterChange('source', e.target.value)}
            >
              <option value="">全部来源</option>
              <option value="web">网站</option>
              <option value="game_plugin">游戏插件</option>
              <option value="offline_sync">离线同步</option>
              <option value="manual">手动操作</option>
              <option value="manual_sync">手动同步</option>
              <option value="manual_retry">手动重试</option>
            </select>
            <select
              className="filter-select"
              value={successFilter}
              onChange={(e) => handleFilterChange('success', e.target.value)}
            >
              <option value="">全部状态</option>
              <option value="true">成功</option>
              <option value="false">失败</option>
            </select>
          </div>

          {isLoading ? <TableLoading colSpan={9} text="正在加载审计日志..." /> : null}
          {!isLoading && error ? <TableError colSpan={9} message={error.message} /> : null}
          {!isLoading && !error ? (
            <div className="table-responsive">
              <table className="data-table">
                <thead>
                  <tr>
                    <th>时间</th>
                    <th>操作</th>
                    <th>目标</th>
                    <th>操作人</th>
                    <th>来源 / IP</th>
                    <th>服务器</th>
                    <th>状态</th>
                    <th>备注</th>
                    <th>详情</th>
                  </tr>
                </thead>
                <tbody>
                  {items.length === 0 ? (
                    <TableEmpty colSpan={9} text="暂无审计记录" />
                  ) : null}
                  {items.map((item) => (
                    <tr key={item.id}>
                      <td style={{ color: 'var(--text3)', whiteSpace: 'nowrap' }}>{formatChinaDateTime(item.created_at)}</td>
                      <td>
                        <span className={`status-pill ${item.operation === 'ban' ? 'pill-accent' : item.operation === 'unban' ? 'pill-online' : 'pill-default'}`}>
                          {OPERATION_LABELS[item.operation] || item.operation}
                        </span>
                      </td>
                      <td>
                        <div>
                          <code className="fs-12">{item.target}</code>
                          {item.player_name ? <div style={{ fontSize: 12, color: 'var(--text3)' }}>{item.player_name}</div> : null}
                        </div>
                      </td>
                      <td>
                        <div>
                          <span className="fw-500">{item.operator_name}</span>
                          {item.operator_steamid ? <div style={{ fontSize: 11, color: 'var(--text3)' }}>{item.operator_steamid}</div> : null}
                        </div>
                      </td>
                      <td>
                        <span className="status-pill pill-default">{SOURCE_LABELS[item.source] || item.source}</span>
                        {item.client_ip ? <div style={{ fontSize: 11, color: 'var(--text3)', marginTop: 4 }}>{item.client_ip}</div> : null}
                      </td>
                      <td className="fs-13">
                        {item.server_name ? (
                          <div>
                            <span>{item.server_name}</span>
                            {item.server_port ? <span className="text-muted-light">:{item.server_port}</span> : null}
                          </div>
                        ) : '-'}
                      </td>
                      <td>
                        <span className={`status-pill ${item.success ? 'pill-online' : 'pill-accent'}`}>
                          {item.success ? '成功' : '失败'}
                        </span>
                      </td>
                      <td style={{ fontSize: 12, color: 'var(--text3)', maxWidth: 200 }}>
                        {item.reason ? (
                          <div><strong>原因:</strong> {item.reason}</div>
                        ) : null}
                        {item.duration_minutes ? (
                          <div><strong>时长:</strong> {item.duration_minutes === 0 ? '永久' : `${item.duration_minutes}分钟`}</div>
                        ) : null}
                        {item.message ? (
                          <div className="mt-4">{item.message}</div>
                        ) : null}
                      </td>
                      <td style={{ fontSize: 12, color: 'var(--text3)', maxWidth: 260 }}>
                        {item.details ? (
                          <details>
                            <summary style={{ cursor: 'pointer', color: 'var(--accent)' }}>{detailsPreview(item.details) || '查看详情'}</summary>
                            <pre className="text-pre-wrap" style={{ margin: '8px 0 0', whiteSpace: 'pre-wrap', wordBreak: 'break-word' }}>{JSON.stringify(item.details, null, 2)}</pre>
                          </details>
                        ) : '-'}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          ) : null}
        </div>
      </div>

      <Pagination page={page} pageSize={20} total={total} onChange={setPage} />
    </div>
  );
}

import React, { useCallback, useEffect, useState } from 'react';
import { api } from '../../lib/api.js';
import { useAuth } from '../../state/auth.jsx';
import { ToastContainer, useToast } from '../../shared/Toast.jsx';
import { SearchBar } from '../../shared/SearchBar.jsx';
import { Pagination } from '../../shared/Pagination.jsx';

const OPERATION_LABELS = {
  ban: '封禁',
  unban: '解封',
  whitelist_add: '添加白名单',
  whitelist_remove: '移除白名单',
};

const SOURCE_LABELS = {
  web: '网站',
  game_plugin: '游戏插件',
  offline_sync: '离线同步',
  manual: '手动操作',
};

function formatDateTime(isoString) {
  if (!isoString) return '-';
  try {
    const date = new Date(isoString);
    const y = date.getFullYear();
    const m = String(date.getMonth() + 1).padStart(2, '0');
    const d = String(date.getDate()).padStart(2, '0');
    const h = String(date.getHours()).padStart(2, '0');
    const min = String(date.getMinutes()).padStart(2, '0');
    const s = String(date.getSeconds()).padStart(2, '0');
    return `${y}-${m}-${d} ${h}:${min}:${s}`;
  } catch {
    return isoString;
  }
}

export function AuditPage() {
  const { session } = useAuth();
  const { toast, toasts, dismiss: dismissToast } = useToast();
  const token = session?.token ?? null;

  const [data, setData] = useState(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState('');

  // 过滤条件
  const [search, setSearch] = useState('');
  const [operationFilter, setOperationFilter] = useState('');
  const [sourceFilter, setSourceFilter] = useState('');
  const [successFilter, setSuccessFilter] = useState('');
  const [page, setPage] = useState(1);

  const loadItems = useCallback(async () => {
    try {
      setLoading(true);
      setError('');
      const params = { page, page_size: 20 };
      if (search) params.target = search;
      if (operationFilter) params.operation = operationFilter;
      if (sourceFilter) params.source = sourceFilter;
      if (successFilter) params.success = successFilter === 'true';
      const result = await api.auditLogs(token, params);
      setData(result);
    } catch (err) {
      setError(err.message);
      setData(null);
    } finally {
      setLoading(false);
    }
  }, [token, page, search, operationFilter, sourceFilter, successFilter]);

  useEffect(() => { loadItems(); }, [loadItems]);

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
        <div className="card-body" style={{ padding: 0 }}>
          {/* 过滤栏 */}
          <div className="filter-bar">
            <SearchBar
              value={search}
              onChange={(v) => { setSearch(v); setPage(1); }}
              placeholder="搜索目标（SteamID/IP）..."
            />
            <select
              className="filter-select"
              value={operationFilter}
              onChange={(e) => handleFilterChange('operation', e.target.value)}
            >
              <option value="">全部操作</option>
              <option value="ban">封禁</option>
              <option value="unban">解封</option>
              <option value="whitelist_add">添加白名单</option>
              <option value="whitelist_remove">移除白名单</option>
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

          {loading ? <div style={{ padding: 20 }}>正在加载审计日志...</div> : null}
          {!loading && error ? <div style={{ padding: 20, color: 'var(--accent)' }}>{error}</div> : null}
          {!loading && !error ? (
            <div className="table-responsive">
              <table className="data-table">
                <thead>
                  <tr>
                    <th>时间</th>
                    <th>操作</th>
                    <th>目标</th>
                    <th>操作人</th>
                    <th>来源</th>
                    <th>服务器</th>
                    <th>状态</th>
                    <th>备注</th>
                  </tr>
                </thead>
                <tbody>
                  {items.length === 0 ? (
                    <tr><td colSpan={8} style={{ padding: 20, color: 'var(--text3)' }}>暂无审计记录。</td></tr>
                  ) : null}
                  {items.map((item) => (
                    <tr key={item.id}>
                      <td style={{ color: 'var(--text3)', whiteSpace: 'nowrap' }}>{formatDateTime(item.created_at)}</td>
                      <td>
                        <span className={`status-pill ${item.operation === 'ban' ? 'pill-accent' : item.operation === 'unban' ? 'pill-online' : 'pill-default'}`}>
                          {OPERATION_LABELS[item.operation] || item.operation}
                        </span>
                      </td>
                      <td>
                        <div>
                          <code style={{ fontSize: 12 }}>{item.target}</code>
                          {item.player_name ? <div style={{ fontSize: 12, color: 'var(--text3)' }}>{item.player_name}</div> : null}
                        </div>
                      </td>
                      <td>
                        <div>
                          <span style={{ fontWeight: 500 }}>{item.operator_name}</span>
                          {item.operator_steamid ? <div style={{ fontSize: 11, color: 'var(--text3)' }}>{item.operator_steamid}</div> : null}
                        </div>
                      </td>
                      <td>
                        <span className="status-pill pill-default">{SOURCE_LABELS[item.source] || item.source}</span>
                      </td>
                      <td style={{ fontSize: 13 }}>
                        {item.server_name ? (
                          <div>
                            <span>{item.server_name}</span>
                            {item.server_port ? <span style={{ color: 'var(--text3)' }}>:{item.server_port}</span> : null}
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
                          <div style={{ marginTop: 4 }}>{item.message}</div>
                        ) : null}
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

      <ToastContainer toasts={toasts} onDismiss={dismissToast} />
    </div>
  );
}
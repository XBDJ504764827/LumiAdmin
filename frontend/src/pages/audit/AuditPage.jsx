import { useState } from 'react';
import { api } from '../../lib/api.js';
import { useApiQuery } from '../../shared/useApiQuery.js';
import { FilterToolbar } from '../../shared/SearchBar.jsx';
import { Pagination } from '../../shared/Pagination.jsx';
import { TableLoading, TableError, TableEmpty } from '../../shared/TableState.jsx';
import { Modal } from '../../shared/Modal.jsx';
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

const OPERATION_OPTIONS = [
  { value: '', label: '全部操作' },
  ...Object.entries(OPERATION_LABELS).map(([value, label]) => ({ value, label })),
];

const SOURCE_OPTIONS = [
  { value: '', label: '全部来源' },
  ...Object.entries(SOURCE_LABELS).map(([value, label]) => ({ value, label })),
];

const SUCCESS_OPTIONS = [
  { value: '', label: '全部状态' },
  { value: 'true', label: '成功' },
  { value: 'false', label: '失败' },
];

function textPreview(value, maxLength = 48) {
  if (!value) return null;
  const text = String(value).replace(/\s+/g, ' ').trim();
  if (text.length <= maxLength) return text;
  return `${text.slice(0, maxLength)}...`;
}

function AuditDetailModal({ item, onClose }) {
  if (!item) return null;
  return (
    <Modal open title="审计详情" onClose={onClose} wide footer={<button className="btn btn-outline" onClick={onClose}>关闭</button>}>
      <div className="detail-grid">
        <span className="detail-label">时间</span><span className="detail-value">{formatChinaDateTime(item.created_at)}</span>
        <span className="detail-label">操作</span><span className="detail-value">{OPERATION_LABELS[item.operation] || item.operation}</span>
        <span className="detail-label">状态</span><span className="detail-value">{item.success ? '成功' : '失败'}</span>
        <span className="detail-label">目标</span><span className="detail-value pre">{item.target || '-'}</span>
        <span className="detail-label">玩家</span><span className="detail-value">{item.player_name || '-'}</span>
        <span className="detail-label">操作人</span><span className="detail-value">{item.operator_name || '-'}</span>
        <span className="detail-label">来源</span><span className="detail-value">{SOURCE_LABELS[item.source] || item.source}</span>
        <span className="detail-label">IP</span><span className="detail-value mono">{item.client_ip || '-'}</span>
        <span className="detail-label">服务器</span><span className="detail-value">{item.server_name ? `${item.server_name}${item.server_port ? `:${item.server_port}` : ''}` : '-'}</span>
        <span className="detail-label">原因</span><span className="detail-value pre">{item.reason || '-'}</span>
        <span className="detail-label">时长</span><span className="detail-value">{item.duration_minutes === 0 ? '永久' : item.duration_minutes ? `${item.duration_minutes} 分钟` : '-'}</span>
        <span className="detail-label">消息</span><span className="detail-value pre">{item.message || '-'}</span>
        <span className="detail-label">详情</span><span className="detail-value pre">{item.details ? JSON.stringify(item.details, null, 2) : '-'}</span>
      </div>
    </Modal>
  );
}

export function AuditPage() {
  const [search, setSearch] = useState('');
  const [operationFilter, setOperationFilter] = useState('');
  const [sourceFilter, setSourceFilter] = useState('');
  const [successFilter, setSuccessFilter] = useState('');
  const [page, setPage] = useState(1);
  const [detailItem, setDetailItem] = useState(null);

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

      <FilterToolbar
        search={search}
        onSearchChange={(value) => { setSearch(value); setPage(1); }}
        searchPlaceholder="搜索目标、玩家、操作人、IP、原因..."
        activeCount={[search, operationFilter, sourceFilter, successFilter].filter(Boolean).length}
        onReset={() => {
          setSearch('');
          setOperationFilter('');
          setSourceFilter('');
          setSuccessFilter('');
          setPage(1);
        }}
        filters={[
          {
            key: 'operation',
            type: 'select',
            value: operationFilter,
            options: OPERATION_OPTIONS,
            onChange: (value) => handleFilterChange('operation', value ?? ''),
          },
          {
            key: 'source',
            type: 'select',
            value: sourceFilter,
            options: SOURCE_OPTIONS,
            onChange: (value) => handleFilterChange('source', value ?? ''),
          },
          {
            key: 'success',
            type: 'select',
            value: successFilter,
            options: SUCCESS_OPTIONS,
            onChange: (value) => handleFilterChange('success', value ?? ''),
          },
        ]}
      />

      <div className="card">
        <div className="card-header">
          <div>
            <div className="card-title">操作记录</div>
            <div className="card-sub">全量审计日志</div>
          </div>
        </div>
        <div className="card-body p-0">
          <div className="table-responsive">
            <table className="data-table mobile-card-table">
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
                {isLoading ? <TableLoading colSpan={9} text="正在加载审计日志..." /> : null}
                {!isLoading && error ? <TableError colSpan={9} message={error.message} /> : null}
                {!isLoading && !error && items.length === 0 ? <TableEmpty colSpan={9} text="暂无审计记录" /> : null}
                {!isLoading && !error ? items.map((item) => (
                    <tr key={item.id}>
                      <td className="text-muted-light" style={{ whiteSpace: 'nowrap' }} data-label="时间">{formatChinaDateTime(item.created_at)}</td>
                      <td className="mobile-card-primary" data-label="操作">
                        <span className={`status-pill ${item.operation === 'ban' ? 'pill-accent' : item.operation === 'unban' ? 'pill-online' : 'pill-default'}`}>
                          {OPERATION_LABELS[item.operation] || item.operation}
                        </span>
                      </td>
                      <td data-label="目标">
                        <div>
                          <code className="fs-12">{item.target}</code>
                          {item.player_name ? <div style={{ fontSize: 12, color: 'var(--text3)' }}>{item.player_name}</div> : null}
                        </div>
                      </td>
                      <td data-label="操作人">
                        <div>
                          <span className="fw-500">{item.operator_name}</span>
                          {item.operator_steamid ? <div style={{ fontSize: 11, color: 'var(--text3)' }}>{item.operator_steamid}</div> : null}
                        </div>
                      </td>
                      <td data-label="来源 / IP">
                        <span className="status-pill pill-default">{SOURCE_LABELS[item.source] || item.source}</span>
                        {item.client_ip ? <div style={{ fontSize: 11, color: 'var(--text3)', marginTop: 4 }}>{item.client_ip}</div> : null}
                      </td>
                      <td className="fs-13" data-label="服务器">
                        {item.server_name ? (
                          <div>
                            <span>{item.server_name}</span>
                            {item.server_port ? <span className="text-muted-light">:{item.server_port}</span> : null}
                          </div>
                        ) : '-'}
                      </td>
                      <td data-label="状态">
                        <span className={`status-pill ${item.success ? 'pill-online' : 'pill-accent'}`}>
                          {item.success ? '成功' : '失败'}
                        </span>
                      </td>
                      <td style={{ fontSize: 12, color: 'var(--text3)', maxWidth: 200 }} data-label="备注">
                        {textPreview(item.reason || item.message) || '-'}
                      </td>
                      <td style={{ fontSize: 12, color: 'var(--text3)', maxWidth: 260 }} data-label="详情">
                        <button className="action-btn action-btn-accent" onClick={() => setDetailItem(item)}>查看详情</button>
                      </td>
                    </tr>
                )) : null}
              </tbody>
            </table>
          </div>
        </div>
      </div>

      <Pagination page={page} pageSize={20} total={total} onChange={setPage} />
      <AuditDetailModal item={detailItem} onClose={() => setDetailItem(null)} />
    </div>
  );
}

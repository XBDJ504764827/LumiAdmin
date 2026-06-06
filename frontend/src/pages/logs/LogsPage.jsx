import React, { useCallback, useEffect, useState } from 'react';
import { useAuth } from '../../state/auth.jsx';
import { api } from '../../lib/api.js';
import { SearchBar } from '../../shared/SearchBar.jsx';
import { Pagination } from '../../shared/Pagination.jsx';
import { formatChinaDateTime } from '../../shared/time.js';

const MODULE_TONE = {
  '白名单管理': 'pill-online',
  '封禁管理': 'pill-danger',
  '社区配置': 'pill-info',
  '用户管理': 'pill-warning',
  '玩家进服': 'pill-info',
  '玩家API': 'pill-info',
};

function modulePillClass(module) {
  return MODULE_TONE[module] ?? 'pill-info';
}

export function LogsPage() {
  const { session } = useAuth();
  const token = session?.token ?? null;
  const [search, setSearch] = useState('');
  const [page, setPage] = useState(1);
  const [data, setData] = useState(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState('');

  const loadItems = useCallback(async () => {
    try {
      setLoading(true);
      setError('');
      const params = { page, page_size: 20 };
      if (search) params.search = search;
      const result = await api.logs(token, params);
      setData(result);
    } catch (loadError) {
      setError(loadError.message);
      setData(null);
    } finally {
      setLoading(false);
    }
  }, [token, page, search]);

  useEffect(() => { loadItems(); }, [loadItems]);

  const items = data?.items ?? [];
  const total = data?.total ?? 0;

  return (
    <div id="logs" className="content-section active">
      <div className="breadcrumb"><span>系统功能</span><span className="sep">›</span><span className="current">操作日志</span></div>
      <div className="page-header">
        <div><div className="page-title">系统操作日志</div><div className="page-sub">记录网站管理员的关键操作追踪（不包含开发管理员记录）。</div></div>
      </div>

      <SearchBar
        value={search}
        onChange={(v) => { setSearch(v); setPage(1); }}
        placeholder="搜索操作人 / 模块 / 动作..."
      />

      <div className="card"><div className="card-body p-0">
        <div className="table-responsive"><table className="data-table"><thead><tr><th>操作人</th><th>模块</th><th>操作动作</th><th>目标详情</th><th>操作IP</th><th>操作时间</th></tr></thead><tbody>
          {loading ? <tr><td colSpan={6} style={{ textAlign: 'center', color: 'var(--text2)' }}>正在加载日志...</td></tr> : null}
          {error ? <tr><td colSpan={6} style={{ textAlign: 'center', color: 'var(--accent)' }}>{error}</td></tr> : null}
          {!loading && !error && items.map((x) => (
            <tr key={`${x.operator_name}-${x.created_at}`}>
              <td className="fw-500">{x.operator_name}</td>
              <td><span className={`status-pill ${modulePillClass(x.module)}`}>{x.module}</span></td>
              <td style={{ fontWeight: 600, color: 'var(--text)' }}>{x.action}</td>
              <td className="text-muted">{x.target_detail}</td>
              <td className="steam-id">{x.ip_address}</td>
              <td className="text-muted-light">{formatChinaDateTime(x.created_at, { seconds: false })}</td>
            </tr>
          ))}
          {!loading && !error && items.length === 0 ? <tr><td colSpan={6} style={{ textAlign: 'center', color: 'var(--text2)' }}>暂无日志记录</td></tr> : null}
        </tbody></table></div>
      </div></div>

      <Pagination page={page} pageSize={20} total={total} onChange={setPage} />
    </div>
  );
}

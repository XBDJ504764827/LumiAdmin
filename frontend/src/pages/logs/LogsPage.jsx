import { useState } from 'react';
import { useApiQuery } from '../../shared/useApiQuery.js';
import { api } from '../../lib/api.js';
import { SearchBar } from '../../shared/SearchBar.jsx';
import { Pagination } from '../../shared/Pagination.jsx';
import { TableLoading, TableError, TableEmpty } from '../../shared/TableState.jsx';
import { formatChinaDateTime } from '../../shared/time.js';

const MODULE_TONE = {
  '白名单管理': 'pill-online',
  '封禁管理': 'pill-danger',
  '社区组管理': 'pill-warning',
  'RCON命令': 'pill-danger',
  '外部服务器': 'pill-info',
  '玩家信息API': 'pill-info',
  'API密钥管理': 'pill-info',
  '封禁API': 'pill-danger',
  '外部封禁API': 'pill-info',
  '地图同步': 'pill-info',
  '网站用户管理': 'pill-warning',
  '玩家进服设置': 'pill-info',
  '封禁申诉': 'pill-warning',
  '玩家举报': 'pill-warning',
  '公共展示页': 'pill-online',
  '认证': 'pill-info',
  '游戏封禁': 'pill-danger',
  '游戏解封': 'pill-online',
};

function modulePillClass(module) {
  return MODULE_TONE[module] ?? 'pill-default';
}

export function LogsPage() {
  const [search, setSearch] = useState('');
  const [page, setPage] = useState(1);

  const { data, isLoading, error } = useApiQuery(
    ['logs', { page, search }],
    (token) => api.logs(token, { page, page_size: 20, ...(search ? { search } : {}) }),
  );

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
          {isLoading ? <TableLoading colSpan={6} text="正在加载日志..." /> : null}
          {!isLoading && error ? <TableError colSpan={6} message={error.message} /> : null}
          {!isLoading && !error && items.map((x) => (
            <tr key={`${x.operator_name}-${x.created_at}`}>
              <td className="fw-500">{x.operator_name}</td>
              <td><span className={`status-pill ${modulePillClass(x.module)}`}>{x.module}</span></td>
              <td style={{ fontWeight: 600, color: 'var(--text)' }}>{x.action}</td>
              <td className="text-muted">{x.target_detail}</td>
              <td className="steam-id">{x.ip_address}</td>
              <td className="text-muted-light">{formatChinaDateTime(x.created_at, { seconds: false })}</td>
            </tr>
          ))}
          {!isLoading && !error && items.length === 0 ? <TableEmpty colSpan={6} text="暂无日志记录" /> : null}
        </tbody></table></div>
      </div></div>

      <Pagination page={page} pageSize={20} total={total} onChange={setPage} />
    </div>
  );
}

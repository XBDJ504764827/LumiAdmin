import { api } from '../../lib/api.js';
import { useApiQuery } from '../../shared/useApiQuery.js';
import { TableEmpty, TableError } from '../../shared/TableState.jsx';
import { formatChinaDateTime } from '../../shared/time.js';

function formatDuration(seconds = 0) {
  const days = Math.floor(seconds / 86400);
  const hours = Math.floor((seconds % 86400) / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  if (days > 0) return `${days}天 ${hours}小时`;
  if (hours > 0) return `${hours}小时 ${minutes}分钟`;
  return `${minutes}分钟`;
}

function formatBytes(bytes = 0) {
  if (bytes >= 1024 * 1024 * 1024) return `${(bytes / 1024 / 1024 / 1024).toFixed(1)} GB`;
  if (bytes >= 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
  if (bytes >= 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${bytes} B`;
}

function taskStatus(task) {
  if (!task.enabled) return { label: '未启用', className: 'pill-default' };
  if (task.consecutive_failures > 0) return { label: '异常', className: 'pill-danger' };
  if (!task.last_success_at && !task.last_failure_at) return { label: '等待首轮', className: 'pill-warning' };
  return { label: '正常', className: 'pill-online' };
}

function dependencyStatus(enabled) {
  return enabled
    ? <span className="status-pill pill-online">已配置</span>
    : <span className="status-pill pill-default">未配置</span>;
}

function OpsMetric({ label, value, hint, tone = 'info' }) {
  return (
    <div className="ops-metric">
      <div className={`ops-metric-dot ops-metric-dot-${tone}`} />
      <div>
        <div className="ops-metric-value">{value}</div>
        <div className="ops-metric-label">{label}</div>
        {hint ? <div className="ops-metric-hint">{hint}</div> : null}
      </div>
    </div>
  );
}

export function OpsOverviewPage() {
  const { data, isLoading, error, refetch, isFetching } = useApiQuery(
    ['opsOverview'],
    (token) => api.opsOverview(token),
    { refetchInterval: 15_000 },
  );

  const overview = data?.data;
  const tasks = overview?.background_tasks ?? [];
  const failingTasks = tasks.filter((task) => task.consecutive_failures > 0).length;
  const pendingTasks = tasks.filter((task) => !task.last_success_at && !task.last_failure_at).length;
  const db = overview?.database ?? {};
  const http = overview?.http ?? {};
  const config = overview?.config ?? {};
  const dependencies = overview?.dependencies ?? {};
  const dbUsage = db.max_connections ? Math.round(((db.size ?? 0) / db.max_connections) * 100) : 0;

  return (
    <div id="ops-overview" className="content-section active">
      <div className="breadcrumb"><span>系统功能</span><span className="sep">›</span><span className="current">系统观测</span></div>
      <div className="page-header">
        <div>
          <div className="page-title">系统观测</div>
          <div className="page-sub">后台任务、请求统计、数据库连接池和外部依赖状态。</div>
        </div>
        <button className="btn btn-outline" onClick={() => refetch()} disabled={isFetching}>
          {isFetching ? '刷新中...' : '刷新'}
        </button>
      </div>

      {isLoading ? (
        <div className="card"><div className="card-body"><div className="loading-state"><div className="loading-spinner" />正在加载系统观测数据...</div></div></div>
      ) : null}

      {!isLoading && error ? (
        <div className="card"><div className="card-body p-0"><table className="data-table"><tbody><TableError colSpan={1} message={error.message} /></tbody></table></div></div>
      ) : null}

      {!isLoading && !error && overview ? (
        <>
          <div className="ops-metric-grid">
            <OpsMetric label="进程运行时间" value={formatDuration(overview.process?.uptime_seconds ?? 0)} hint={`启动于 ${formatChinaDateTime(overview.process?.started_at)}`} tone="online" />
            <OpsMetric label="后台任务" value={`${tasks.length - failingTasks} / ${tasks.length}`} hint={failingTasks ? `${failingTasks} 个任务异常` : pendingTasks ? `${pendingTasks} 个任务等待首轮` : '全部任务正常'} tone={failingTasks ? 'danger' : pendingTasks ? 'warning' : 'online'} />
            <OpsMetric label="HTTP 请求" value={http.total_requests ?? 0} hint={`错误率 ${(http.error_rate ?? 0).toFixed(2)}%，慢请求 ${http.slow_requests ?? 0}`} tone={(http.error_rate ?? 0) > 0 ? 'warning' : 'info'} />
            <OpsMetric label="数据库连接池" value={`${db.size ?? 0} / ${db.max_connections ?? 0}`} hint={`空闲 ${db.idle ?? 0}，使用率 ${dbUsage}%`} tone={dbUsage >= 80 ? 'warning' : 'info'} />
          </div>

          <div className="lower-grid ops-lower-grid">
            <div className="card">
              <div className="card-header">
                <div>
                  <div className="card-title">后台任务</div>
                  <div className="card-sub">最近一次执行结果和连续失败次数</div>
                </div>
              </div>
              <div className="card-body p-0">
                <div className="table-responsive">
                  <table className="data-table">
                    <thead>
                      <tr>
                        <th>任务</th>
                        <th>分类</th>
                        <th>状态</th>
                        <th>周期</th>
                        <th>最近完成</th>
                        <th>耗时</th>
                        <th>运行 / 失败</th>
                        <th>说明</th>
                      </tr>
                    </thead>
                    <tbody>
                      {tasks.length === 0 ? <TableEmpty colSpan={8} text="暂无后台任务数据" /> : null}
                      {tasks.map((task) => {
                        const status = taskStatus(task);
                        return (
                          <tr key={task.key}>
                            <td className="fw-500">{task.name}</td>
                            <td><span className="status-pill pill-default">{task.category}</span></td>
                            <td><span className={`status-pill ${status.className}`}>{status.label}</span></td>
                            <td className="text-muted-light">{task.interval_secs ? `${task.interval_secs}s` : '动态'}</td>
                            <td className="text-muted-light">{task.last_finished_at ? formatChinaDateTime(task.last_finished_at) : '-'}</td>
                            <td className="text-muted-light">{task.last_duration_ms != null ? `${task.last_duration_ms} ms` : '-'}</td>
                            <td className="text-muted-light">{task.runs} / {task.failures}</td>
                            <td className="ops-task-message">{task.last_message ?? '-'}</td>
                          </tr>
                        );
                      })}
                    </tbody>
                  </table>
                </div>
              </div>
            </div>

            <div className="ops-side-stack">
              <div className="card">
                <div className="card-header">
                  <div>
                    <div className="card-title">请求概况</div>
                    <div className="card-sub">当前进程内聚合统计</div>
                  </div>
                </div>
                <div className="card-body">
                  <div className="ops-kv"><span>总请求</span><strong>{http.total_requests ?? 0}</strong></div>
                  <div className="ops-kv"><span>5xx 错误</span><strong>{http.error_requests ?? 0}</strong></div>
                  <div className="ops-kv"><span>平均耗时</span><strong>{http.average_duration_ms ?? 0} ms</strong></div>
                  <div className="ops-kv"><span>最大耗时</span><strong>{http.max_duration_ms ?? 0} ms</strong></div>
                  <div className="ops-kv"><span>慢请求</span><strong>{http.slow_requests ?? 0}</strong></div>
                </div>
              </div>

              <div className="card">
                <div className="card-header">
                  <div>
                    <div className="card-title">外部依赖</div>
                    <div className="card-sub">按当前环境变量判断</div>
                  </div>
                </div>
                <div className="card-body">
                  <div className="ops-kv"><span>Steam Web API</span>{dependencyStatus(dependencies.steam_web_api)}</div>
                  <div className="ops-kv"><span>SteamChina Profile</span>{dependencyStatus(dependencies.steamchina_profile)}</div>
                  <div className="ops-kv"><span>SteamChina Level</span>{dependencyStatus(dependencies.steamchina_level)}</div>
                  <div className="ops-kv"><span>地图等级 MySQL</span>{dependencyStatus(dependencies.mysql_map_tiers)}</div>
                  <div className="ops-kv"><span>R2 文件存储</span>{dependencyStatus(dependencies.r2_storage)}</div>
                </div>
              </div>

              <div className="card">
                <div className="card-header">
                  <div>
                    <div className="card-title">运行配置</div>
                    <div className="card-sub">非敏感摘要</div>
                  </div>
                </div>
                <div className="card-body">
                  <div className="ops-kv"><span>请求超时</span><strong>{config.request_timeout_secs}s</strong></div>
                  <div className="ops-kv"><span>请求体上限</span><strong>{formatBytes(config.max_request_body_bytes)}</strong></div>
                  <div className="ops-kv"><span>性能历史保留</span><strong>{formatDuration(config.status_history_retention_secs)}</strong></div>
                  <div className="ops-kv"><span>进服记录保留</span><strong>{config.access_log_retention_days} 天</strong></div>
                  <div className="ops-kv"><span>CORS Origin</span>{dependencyStatus(config.cors_origin_configured)}</div>
                </div>
              </div>
            </div>
          </div>
        </>
      ) : null}
    </div>
  );
}

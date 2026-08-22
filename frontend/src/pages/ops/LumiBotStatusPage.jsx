import { api } from '../../lib/api.js';
import { useApiQuery } from '../../shared/useApiQuery.js';
import { PageState } from '../../shared/PageState.jsx';
import { Pagination } from '../../shared/Pagination.jsx';
import { StatusPill } from '../../shared/StatusPill.jsx';
import { formatChinaDateTime } from '../../shared/time.js';
import { IconActivity, IconRefresh } from '../../shared/Icons.jsx';
import { useState } from 'react';

function eventKind(item) {
  if (item?.status === 'failed') return 'danger';
  if (item?.status === 'pending') return 'warning';
  return item?.status === 'sent' ? 'success' : 'default';
}

function eventStatusText(item) {
  if (item?.status === 'pending') return '待上报';
  if (item?.status === 'sent') return '已上报';
  if (item?.status === 'failed') return '失败死信';
  return item?.status ?? '-';
}

function eventLevelTag(level) {
  const kind = level === 'error' || level === 'critical' ? 'danger' : level === 'warning' ? 'warning' : 'info';
  return <StatusPill kind={kind}>{level || '-'}</StatusPill>;
}

function eventTitle(item) {
  return item?.title || item?.event_type || `事件 ${String(item?.id ?? '').slice(0, 8)}`;
}

function eventMessage(item) {
  const message = item?.message || item?.last_error || '-';
  return (
    <span className={"lumi-bot-event-msg" + (item?.last_error ? ' lumi-bot-event-msg-error' : '')}>
      {message}
    </span>
  );
}

function auditKind(item) {
  if (item?.success === false) return 'danger';
  if (item?.operation?.startsWith('qq_review_denied')) return 'danger';
  return 'success';
}

function auditStatusText(item) {
  if (item?.success === false) return '失败';
  return '成功';
}

function auditOperationLabel(item) {
  const op = item?.operation ?? '';
  if (op.startsWith('qq_review_denied')) return '审批拒绝';
  if (op === 'whitelist_approve') return '通过';
  if (op === 'whitelist_reject') return '拒绝';
  return op;
}

function AuditLogTable({ logs }) {
  if (!logs || logs.length === 0) {
    return <div className="lumi-bot-audit-empty">暂无 QQ 审批记录。管理员点击「通过/拒绝」按钮后，这里会展示后端判定结果。</div>;
  }
  return (
    <div className="table-responsive">
      <table className="data-table">
        <thead>
          <tr><th>时间</th><th>结果</th><th>操作</th><th>操作人（openid）</th><th>说明</th></tr>
        </thead>
        <tbody>
          {logs.map((item) => (
            <tr key={item.id}>
              <td className="text-muted-light">{formatChinaDateTime(item.created_at)}</td>
              <td><StatusPill kind={auditKind(item)}>{auditStatusText(item)}</StatusPill></td>
              <td>{auditOperationLabel(item)}</td>
              <td className="steam-id">{item.operator_name || '-'}</td>
              <td className="lumi-bot-audit-msg">{item.message || item.reason || '-'}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function formatDuration(seconds = 0) {
  if (seconds < 60) return `${seconds} 秒`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes} 分钟`;
  const hours = Math.floor(minutes / 60);
  return `${hours} 小时 ${minutes % 60} 分钟`;
}

function connectionStatus(status) {
  if (!status?.configured) return { label: '未配置', kind: 'default' };
  if (status.reachable) return { label: '在线', kind: 'success' };
  return { label: '不可达', kind: 'danger' };
}

function taskStatus(task) {
  if (!task) return { label: '未启动', kind: 'default' };
  if (!task.enabled) return { label: '未启用', kind: 'default' };
  if (task.running) return { label: '运行中', kind: 'info' };
  if (task.consecutive_failures > 0) return { label: '异常', kind: 'danger' };
  if (!task.last_success_at && !task.last_failure_at) return { label: '等待首轮', kind: 'warning' };
  return { label: '正常', kind: 'success' };
}

function Metric({ label, value, hint, tone = 'info' }) {
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

const LUMI_BOT_EVENT_PAGE_SIZE = 10;

const EVENT_STATUS_FILTERS = [
  { value: '', label: '全部状态' },
  { value: 'pending', label: '待上报' },
  { value: 'sent', label: '已上报' },
  { value: 'failed', label: '失败死信' },
];

function EventLogTable({ logs }) {
  if (!logs || logs.length === 0) {
    return <div className="lumi-bot-audit-empty">暂无 LumiBot 事件记录。</div>;
  }
  return (
    <div className="table-responsive">
      <table className="data-table">
        <thead>
          <tr><th>时间</th><th>状态</th><th>级别</th><th>事件</th><th>说明</th><th>重试</th></tr>
        </thead>
        <tbody>
          {logs.map((item) => (
            <tr key={item.id}>
              <td className="text-muted-light">{formatChinaDateTime(item.queued_at)}</td>
              <td><StatusPill kind={eventKind(item)}>{eventStatusText(item)}</StatusPill></td>
              <td>{eventLevelTag(item.level)}</td>
              <td>{eventTitle(item)}</td>
              <td>{eventMessage(item)}</td>
              <td className="text-muted-light">{item.attempts ?? 0}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

// LumiBot 事件日志：lumi_bot_event_queue 中每条事件的逐条记录
// （提交时间、当前状态、重试次数、最近失败原因），按状态筛选 + 分页查看。
function LumiBotEventLogCard() {
  const [status, setStatus] = useState('');
  const [page, setPage] = useState(1);

  const { data, isLoading, error, refetch, isFetching } = useApiQuery(
    ['lumiBotEvents', status, page],
    (token) =>
      api.lumiBotEvents(token, {
        status: status || undefined,
        page,
        page_size: LUMI_BOT_EVENT_PAGE_SIZE,
      }),
    { refetchInterval: 15_000, refetchOnWindowFocus: false },
  );

  const items = data?.items ?? [];
  const total = data?.total ?? 0;

  const selectStatus = (value) => {
    setStatus(value);
    setPage(1);
  };

  return (
    <div className="card lumi-bot-audit-card">
      <div className="card-header">
        <div>
          <div className="card-title">事件日志</div>
          <div className="card-sub">lumi_bot_event_queue 中每条事件的提交、上报与失败原因（每 15 秒自动刷新）</div>
        </div>
        <div className="lumi-bot-event-toolbar">
          <select
            className="filter-select"
            value={status}
            onChange={(event) => selectStatus(event.target.value)}
            aria-label="按状态筛选事件"
          >
            {EVENT_STATUS_FILTERS.map((filter) => (
              <option key={filter.value} value={filter.value}>
                {filter.label}
              </option>
            ))}
          </select>
          <button className="btn btn-outline" type="button" onClick={() => refetch()} disabled={isFetching}>
            <IconRefresh size={14} />
            {isFetching ? '刷新中...' : '刷新'}
          </button>
        </div>
      </div>
      <div className="card-body">
        {isLoading ? <PageState title="正在读取事件日志" /> : null}
        {!isLoading && error ? <PageState tone="danger" title="无法读取事件日志" message={error.message} action={refetch} /> : null}
        {!isLoading && !error ? (
          <>
            <EventLogTable logs={items} />
            <Pagination page={page} pageSize={LUMI_BOT_EVENT_PAGE_SIZE} total={total} onChange={setPage} />
          </>
        ) : null}
      </div>
    </div>
  );
}

export function LumiBotStatusPage() {
  const { data, isLoading, error, refetch, isFetching } = useApiQuery(
    ['lumiBotStatus'],
    (token) => api.lumiBotStatus(token),
    { refetchInterval: 15_000, refetchOnWindowFocus: false },
  );

  // QQ 按钮审批日志：管理员点击「通过/拒绝」后，后端每次判定（绑定/启用/角色）
  // 都会写入 audit_logs（source='qq_bot'）。这里每 10 秒自动刷新，方便在线排查。
  const { data: auditData, refetch: refetchAudit, isFetching: auditFetching } = useApiQuery(
    ['lumiBotAuditLogs'],
    (token) => api.lumiBotAuditLogs(token, { page: 1, page_size: 50 }),
    { refetchInterval: 10_000, refetchOnWindowFocus: false },
  );
  const auditLogs = auditData?.items ?? [];
  const status = data?.data;
  const connection = connectionStatus(status);
  const task = taskStatus(status?.sync_task);
  const queue = status?.queue ?? {};
  const taskData = status?.sync_task;
  const hasOperationalIssue = status && (
    !status.configured || !status.reachable || task.kind === 'danger' || queue.failed > 0
  );

  return (
    <div id="lumi-bot-status" className="content-section active">
      <div className="breadcrumb"><span>日志与审计</span><span className="sep">›</span><span className="current">LumiBot 状态</span></div>
      <div className="page-header">
        <div>
          <h1>LumiBot 状态</h1>
          <p>查看 QQ 机器人连通性、事件队列和 LumiAdmin 上报任务。</p>
        </div>
        <button className="btn btn-outline" type="button" onClick={() => refetch()} disabled={isFetching}>
          <IconRefresh size={14} />
          {isFetching ? '刷新中...' : '刷新'}
        </button>
      </div>

      {isLoading ? <PageState title="正在读取 LumiBot 状态" message="正在检查连接和事件队列。" /> : null}
      {!isLoading && error ? <PageState tone="danger" title="无法读取 LumiBot 状态" message={error.message} action={refetch} /> : null}

      {!isLoading && !error && status ? (
        <>
          <div className="ops-metric-grid lumi-bot-metric-grid">
            <Metric
              label="连接状态"
              value={<StatusPill kind={connection.kind}>{connection.label}</StatusPill>}
              hint={status.api_url || '未配置 API 地址'}
              tone={connection.kind === 'success' ? 'online' : connection.kind === 'danger' ? 'danger' : 'warning'}
            />
            <Metric
              label="健康检查延迟"
              value={status.latency_ms != null ? `${status.latency_ms} ms` : '-'}
              hint={`检查于 ${formatChinaDateTime(status.checked_at)}`}
              tone={status.reachable ? 'online' : 'danger'}
            />
            <Metric
              label="待上报事件"
              value={queue.pending ?? 0}
              hint={`累计成功 ${queue.sent ?? 0} 条`}
              tone={(queue.pending ?? 0) > 0 ? 'warning' : 'online'}
            />
            <Metric
              label="失败事件"
              value={queue.failed ?? 0}
              hint={queue.last_failure_at ? `最近失败 ${formatChinaDateTime(queue.last_failure_at)}` : '暂无失败事件'}
              tone={(queue.failed ?? 0) > 0 ? 'danger' : 'online'}
            />
          </div>

          {hasOperationalIssue ? (
            <div className="lumi-bot-alert" role="status">
              <IconActivity size={18} />
              <div>
                <strong>{!status.configured ? 'LumiBot 集成未配置' : !status.reachable ? 'LumiBot 当前不可达' : 'LumiBot 上报链路需要关注'}</strong>
                <span>{status.health_error || status.last_error || (queue.failed > 0 ? `${queue.failed} 条事件已进入失败状态。` : '请检查后台同步任务。')}</span>
              </div>
            </div>
          ) : null}

          <div className="lumi-bot-status-grid">
            <div className="card">
              <div className="card-header">
                <div>
                  <div className="card-title">连接信息</div>
                  <div className="card-sub">后端每次刷新都会探测 LumiBot `/health`</div>
                </div>
                <StatusPill kind={connection.kind}>{connection.label}</StatusPill>
              </div>
              <div className="card-body">
                <div className="ops-kv"><span>API 地址</span><strong className="lumi-bot-url">{status.api_url || '-'}</strong></div>
                <div className="ops-kv"><span>健康检查</span><strong>{status.reachable ? 'HTTP 200' : (status.health_error || '未执行')}</strong></div>
                <div className="ops-kv"><span>最近检查</span><strong>{formatChinaDateTime(status.checked_at)}</strong></div>
                <div className="ops-kv"><span>最近成功上报</span><strong>{formatChinaDateTime(queue.last_sent_at)}</strong></div>
              </div>
            </div>

            <div className="card">
              <div className="card-header">
                <div>
                  <div className="card-title">事件队列</div>
                  <div className="card-sub">立即上报失败的事件会进入这里重试</div>
                </div>
              </div>
              <div className="card-body">
                <div className="ops-kv"><span>等待上报</span><strong>{queue.pending ?? 0}</strong></div>
                <div className="ops-kv"><span>已成功上报</span><strong>{queue.sent ?? 0}</strong></div>
                <div className="ops-kv"><span>失败 / 死信</span><strong className={queue.failed > 0 ? 'text-danger' : ''}>{queue.failed ?? 0}</strong></div>
                <div className="ops-kv"><span>最近失败</span><strong>{formatChinaDateTime(queue.last_failure_at)}</strong></div>
              </div>
            </div>
          </div>

          <div className="card lumi-bot-task-card">
            <div className="card-header">
              <div>
                <div className="card-title">后台同步任务</div>
                <div className="card-sub">负责重试 LumiBot 事件队列</div>
              </div>
              <StatusPill kind={task.kind}>{task.label}</StatusPill>
            </div>
            <div className="card-body lumi-bot-task-grid">
              <div className="ops-kv"><span>任务名称</span><strong>{taskData?.name || 'LumiBot 事件上报'}</strong></div>
              <div className="ops-kv"><span>执行周期</span><strong>{taskData?.interval_secs ? `${taskData.interval_secs} 秒` : '-'}</strong></div>
              <div className="ops-kv"><span>执行次数</span><strong>{taskData?.runs ?? 0}</strong></div>
              <div className="ops-kv"><span>失败次数</span><strong>{taskData?.failures ?? 0}</strong></div>
              <div className="ops-kv"><span>连续失败</span><strong className={taskData?.consecutive_failures > 0 ? 'text-danger' : ''}>{taskData?.consecutive_failures ?? 0}</strong></div>
              <div className="ops-kv"><span>任务运行时间</span><strong>{taskData?.running ? formatChinaDateTime(taskData.current_started_at) : '-'}</strong></div>
              <div className="ops-kv"><span>最近完成</span><strong>{formatChinaDateTime(taskData?.last_finished_at)}</strong></div>
              <div className="ops-kv"><span>下次执行</span><strong>{formatChinaDateTime(taskData?.next_run_at)}</strong></div>
            </div>
            {taskData?.last_error ? <div className="lumi-bot-task-error">最近错误：{taskData.last_error}</div> : null}
          </div>

          <div className="card lumi-bot-audit-card">
            <div className="card-header">
              <div>
                <div className="card-title">QQ 审批记录</div>
                <div className="card-sub">管理员点击「通过/拒绝」按钮后，后端每次判定都会记录在这里（每 10 秒自动刷新）</div>
              </div>
              <button className="btn btn-outline" type="button" onClick={() => refetchAudit()} disabled={auditFetching}>
                <IconRefresh size={14} />
                {auditFetching ? '刷新中...' : '刷新'}
              </button>
            </div>
            <div className="card-body">
              <AuditLogTable logs={auditLogs} />
            </div>
          </div>

          <LumiBotEventLogCard />
        </>
      ) : null}
    </div>
  );
}

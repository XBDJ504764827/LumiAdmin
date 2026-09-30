import { useState } from 'react';
import { api } from '../../lib/api.js';
import { useAsync } from '../../shared/useAsync.js';
import { useAuth } from '../../state/store.js';
import { useToast } from '../../shared/Toast.jsx';
import { MetricCard } from '../../shared/MetricCard.jsx';
import { TableLoading, TableError } from '../../shared/TableState.jsx';
import { downloadHostAgentFile, normalizeSetupResponse, MOCK_POWER_LOGS, POWER_LOG_RESULT_META } from './hostAgent.js';
import { HeartbeatTrendChart, PowerTaskTrendChart } from './HostAgentCharts.jsx';

// 演示数据：监控接口未上线前用于预览页面设计，接入真实上报后删除。
const MOCK_HOSTS = [
  {
    hostname: 'game-01',
    ip: '192.168.0.101',
    instances: ['csgoserver', 'csgo2server', 'csgo3server'],
    online: true,
    lastSeen: '10 秒前',
    version: '0.1.0',
  },
  {
    hostname: 'game-02',
    ip: '192.168.0.102',
    instances: ['csgoserver'],
    online: false,
    lastSeen: '3 分钟前',
    version: '0.1.0',
  },
];
const MOCK_PENDING_JOBS = 1;

function mockStats() {
  const instances = MOCK_HOSTS.reduce((sum, host) => sum + host.instances.length, 0);
  const online = MOCK_HOSTS.filter((host) => host.online).length;
  return { hosts: MOCK_HOSTS.length, instances, online, pendingJobs: MOCK_PENDING_JOBS };
}

export function HostAgentPage() {
  const { session } = useAuth();
  const { toast } = useToast();
  const token = session?.token ?? null;

  const [refreshKey, setRefreshKey] = useState(0);
  const [downloading, setDownloading] = useState(false);

  const setupState = useAsync(() => api.hostAgentSetup(token), [token, refreshKey]);
  const setup = setupState.data ? normalizeSetupResponse(setupState.data) : null;
  const installFile = setup?.files.find((file) => file.name === 'install.sh');

  const stats = mockStats();
  const offline = stats.hosts - stats.online;

  async function handleDownloadInstall() {
    if (!installFile?.available) {
      toast({ title: '暂不可下载', message: 'install.sh 待发布流程落地后提供', tone: 'warning' });
      return;
    }
    try {
      setDownloading(true);
      await downloadHostAgentFile(token, 'install.sh');
      toast({ title: '下载成功', message: 'install.sh', tone: 'success' });
    } catch (error) {
      toast({ title: '下载失败', message: error.message, tone: 'danger' });
    } finally {
      setDownloading(false);
    }
  }

  return (
    <div id="host-agent" className="content-section active">
      <div className="breadcrumb"><span>核心管理</span><span className="sep">›</span><span className="current">Agent控制</span></div>
      <div className="page-header">
        <div><div className="page-title">Agent控制</div><div className="page-sub">宿主机 Agent 运行监控：一台宿主机跑一个 Agent，管理本机全部 LGSM 实例。</div></div>
        <div style={{ display: 'flex', gap: 8, alignItems: 'center' }}>
          <span className="status-pill pill-warning">演示数据</span>
          <button className="btn btn-outline" onClick={() => setRefreshKey((v) => v + 1)} disabled={setupState.loading}>
            {setupState.loading ? '刷新中...' : '刷新'}
          </button>
        </div>
      </div>

      <div className="metric-grid">
        <MetricCard label="宿主机" value={stats.hosts} badge={`${stats.online} 在线`} />
        <MetricCard label="受管实例" value={stats.instances} badge={`${stats.hosts} 台宿主机`} />
        <MetricCard label="在线 Agent" value={`${stats.online}/${stats.hosts}`} badge={offline ? `${offline} 台离线` : '全部在线'} accent={offline > 0} />
        <MetricCard label="待执行任务" value={stats.pendingJobs} badge={stats.pendingJobs ? '有任务排队' : '队列为空'} accent={stats.pendingJobs > 0} />
      </div>

      <div className="dash-charts-grid">
        <HeartbeatTrendChart />
        <PowerTaskTrendChart />
      </div>

      <div className="card">
        <div className="card-header">
          <div>
            <div className="card-title">电源操作记录</div>
            <div className="card-sub">与操作日志页同源（演示数据，接入真实日志后替换）</div>
          </div>
        </div>
        <div className="card-body">
          <div className="table-responsive">
            <table className="data-table mobile-card-table">
              <thead>
                <tr>
                  <th>操作人</th>
                  <th>模块</th>
                  <th>操作动作</th>
                  <th>目标详情</th>
                  <th>操作IP</th>
                  <th>操作时间</th>
                </tr>
              </thead>
              <tbody>
                {MOCK_POWER_LOGS.map((log) => {
                  const result = POWER_LOG_RESULT_META[log.result];
                  return (
                    <tr key={log.id}>
                      <td className="fw-600 mobile-card-primary" data-label="操作人">{log.operator}</td>
                      <td data-label="模块"><span className="status-pill pill-danger">{log.module}</span></td>
                      <td data-label="操作动作">
                        {log.action}
                        <span className={`status-pill ${result.className}`} style={{ marginLeft: 6 }}>{result.label}</span>
                      </td>
                      <td data-label="目标详情">{log.target}</td>
                      <td data-label="操作IP">{log.ip}</td>
                      <td data-label="操作时间">{log.time}</td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        </div>
      </div>

      <div className="lower-grid ops-lower-grid">
        <div className="card">
          <div className="card-header">
            <div>
              <div className="card-title">宿主机</div>
              <div className="card-sub">共 {stats.hosts} 台，{stats.online} 台在线</div>
            </div>
          </div>
          <div className="card-body">
            <div className="table-responsive">
              <table className="data-table mobile-card-table">
                <thead>
                  <tr>
                    <th>宿主机</th>
                    <th>受管实例</th>
                    <th>状态</th>
                    <th>最近心跳</th>
                  </tr>
                </thead>
                <tbody>
                  {setupState.loading ? <TableLoading colSpan={4} text="正在加载宿主机数据..." /> : null}
                  {!setupState.loading && setupState.error ? (
                    <TableError colSpan={4} message={setupState.error.message} />
                  ) : null}
                  {!setupState.loading && !setupState.error && MOCK_HOSTS.map((host) => (
                    <tr key={host.hostname}>
                      <td className="fw-600 mobile-card-primary" data-label="宿主机">
                        {host.hostname}
                        <div className="text-muted-light" style={{ fontWeight: 400, fontSize: 12 }}>{host.ip} · v{host.version}</div>
                      </td>
                      <td data-label="受管实例">
                        {host.instances.length} 个
                        <div className="text-muted-light" style={{ fontSize: 12 }}>{host.instances.join('、')}</div>
                      </td>
                      <td data-label="状态">
                        {host.online
                          ? <span className="status-pill pill-online">在线</span>
                          : <span className="status-pill pill-danger">离线</span>}
                      </td>
                      <td data-label="最近心跳">{host.lastSeen}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </div>
        </div>

        <div className="card">
          <div className="card-header">
            <div>
              <div className="card-title">快速上手</div>
              <div className="card-sub">约 1 分钟完成接入</div>
            </div>
          </div>
          <div className="card-body">
            <ol style={{ margin: '0 0 12px', paddingLeft: 20, fontSize: 13, color: 'var(--text2)', lineHeight: 1.8 }}>
              <li>下载安装脚本传到宿主机。</li>
              <li>按脚本头部注释填好 4 个参数，sudo 执行。</li>
              <li>用 <code style={{ fontSize: 12, background: 'var(--surface2)', padding: '2px 6px', borderRadius: 4 }}>journalctl -u lumi-host-agent -f</code> 确认启动。</li>
              <li>回到本页查看宿主机上线（监控功能待接入）。</li>
            </ol>
            <button
              className="btn btn-primary"
              style={{ width: '100%' }}
              disabled={downloading || !installFile?.available}
              onClick={handleDownloadInstall}
            >
              {downloading ? '下载中...' : '下载 install.sh'}
            </button>
            <div className="info-box warning" style={{ marginTop: 12 }}>
              Agent 口令签发待开发，执行前先将命令中的口令占位替换为后续签发的口令。
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

import { useState } from 'react';
import { api } from '../../lib/api.js';
import { useAsync } from '../../shared/useAsync.js';
import { useAuth } from '../../state/store.js';
import { useToast } from '../../shared/Toast.jsx';
import { MetricCard } from '../../shared/MetricCard.jsx';
import { TableLoading, TableError, TableEmpty } from '../../shared/TableState.jsx';
import { downloadHostAgentFile, normalizeSetupResponse } from './hostAgent.js';

export function HostAgentPage() {
  const { session } = useAuth();
  const { toast } = useToast();
  const token = session?.token ?? null;

  const [refreshKey, setRefreshKey] = useState(0);
  const [downloading, setDownloading] = useState(false);

  const setupState = useAsync(() => api.hostAgentSetup(token), [token, refreshKey]);
  const setup = setupState.data ? normalizeSetupResponse(setupState.data) : null;
  const installFile = setup?.files.find((file) => file.name === 'install.sh');

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
        <button className="btn btn-outline" onClick={() => setRefreshKey((v) => v + 1)} disabled={setupState.loading}>
          {setupState.loading ? '刷新中...' : '刷新'}
        </button>
      </div>

      <div className="metric-grid">
        <MetricCard label="宿主机" value="—" badge="待接入" />
        <MetricCard label="受管实例" value="—" badge="待接入" />
        <MetricCard label="在线 Agent" value="—" badge="待接入" />
        <MetricCard label="待执行任务" value="—" badge="待接入" />
      </div>

      <div className="lower-grid ops-lower-grid">
        <div className="card">
          <div className="card-header">
            <div>
              <div className="card-title">宿主机</div>
              <div className="card-sub">Agent 上报接入后自动出现</div>
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
                  {!setupState.loading && !setupState.error ? (
                    <TableEmpty colSpan={4} text="暂无宿主机接入，完成右侧快速上手后自动出现。" />
                  ) : null}
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

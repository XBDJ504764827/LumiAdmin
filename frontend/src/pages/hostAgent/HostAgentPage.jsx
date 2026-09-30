import { useState } from 'react';
import { api } from '../../lib/api.js';
import { useAsync } from '../../shared/useAsync.js';
import { useAuth } from '../../state/store.js';
import { useToast } from '../../shared/Toast.jsx';
import { Card } from '../../shared/Card.jsx';
import { TableLoading, TableError, TableEmpty } from '../../shared/TableState.jsx';
import {
  buildInstallCommand,
  downloadHostAgentFile,
  formatBytes,
  normalizeSetupResponse,
} from './hostAgent.js';

export function HostAgentPage() {
  const { session } = useAuth();
  const { toast } = useToast();
  const token = session?.token ?? null;

  const [refreshKey, setRefreshKey] = useState(0);
  const [downloading, setDownloading] = useState(null);
  const [backendUrl, setBackendUrl] = useState(() => window.location.origin);
  const [lgsmDir, setLgsmDir] = useState('/home/steam/lgsm');
  const [instances, setInstances] = useState('csgoserver,csgo2server');

  const setupState = useAsync(() => api.hostAgentSetup(token), [token, refreshKey]);
  const setup = setupState.data ? normalizeSetupResponse(setupState.data) : null;
  const installCommand = buildInstallCommand({ backendUrl, lgsmDir, instances });

  async function handleDownload(file) {
    if (!file.available) {
      toast({ title: '暂不可下载', message: `${file.name} 待发布流程落地后提供`, tone: 'warning' });
      return;
    }
    try {
      setDownloading(file.name);
      await downloadHostAgentFile(token, file.name);
      toast({ title: '下载成功', message: file.name, tone: 'success' });
    } catch (error) {
      toast({ title: '下载失败', message: error.message, tone: 'danger' });
    } finally {
      setDownloading(null);
    }
  }

  async function handleCopy(text, label) {
    try {
      await navigator.clipboard.writeText(text);
      toast({ title: '已复制', message: label, tone: 'success' });
    } catch (error) {
      toast({ title: '复制失败', message: error.message, tone: 'danger' });
    }
  }

  return (
    <div id="host-agent" className="content-section active">
      <div className="breadcrumb"><span>核心管理</span><span className="sep">›</span><span className="current">Agent控制</span></div>
      <div className="page-header">
        <div><div className="page-title">Agent控制</div><div className="page-sub">宿主机 Agent 下载与设置（一台宿主机跑一个 Agent，管理本机全部 LGSM 实例）。</div></div>
      </div>

      <Card title="Agent 下载" subtitle="安装包由本站直接托管下发，不走 GitHub">
        {setupState.loading ? <TableLoading /> : null}
        {setupState.error ? (
          <TableError message={setupState.error.message} onRetry={() => setRefreshKey((v) => v + 1)} />
        ) : null}
        {setup && setup.files.length === 0 ? <TableEmpty message="暂无可下载文件。" /> : null}
        {setup && setup.files.length > 0 ? (
          <div className="table-responsive">
            <table className="data-table mobile-card-table">
              <thead>
                <tr>
                  <th>文件</th>
                  <th>说明</th>
                  <th>大小</th>
                  <th className="text-right">操作</th>
                </tr>
              </thead>
              <tbody>
                {setup.files.map((file) => (
                  <tr key={file.name}>
                    <td className="fw-600 mobile-card-primary" data-label="文件">{file.name}</td>
                    <td data-label="说明">{file.description}</td>
                    <td data-label="大小">{file.available ? formatBytes(file.sizeBytes) : <span className="text-muted-light">—</span>}</td>
                    <td className="text-right mobile-card-actions" data-label="操作">
                      {file.available ? (
                        <button
                          className="btn btn-sm"
                          disabled={downloading === file.name}
                          onClick={() => handleDownload(file)}
                        >
                          {downloading === file.name ? '下载中...' : '下载'}
                        </button>
                      ) : (
                        <span className="status-pill pill-warning">待发布</span>
                      )}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        ) : null}
        {setup?.version ? <div className="text-muted-light" style={{ marginTop: 8 }}>后端版本：{setup.version}</div> : null}
      </Card>

      <Card title="安装与设置" subtitle="在游戏服宿主机上执行，约 1 分钟完成">
        <ol style={{ margin: '0 0 12px', paddingLeft: 20, fontSize: 13, color: 'var(--text2)', lineHeight: 1.7 }}>
          <li>从上方下载 <code style={{ fontSize: 12, background: 'var(--surface2)', padding: '2px 6px', borderRadius: 4 }}>install.sh</code> 传到宿主机（或直接在宿主机上执行第 2 步的一键命令）。</li>
          <li>按实际填写下面三个参数，点击复制后在宿主机上 <span className="fw-600">sudo 执行</span>。</li>
          <li>执行后用 <code style={{ fontSize: 12, background: 'var(--surface2)', padding: '2px 6px', borderRadius: 4 }}>journalctl -u lumi-host-agent -f</code> 确认 Agent 已启动。</li>
        </ol>
        <div className="form-group">
          <label>后端地址</label>
          <input className="form-control" value={backendUrl} onChange={(e) => setBackendUrl(e.target.value)} placeholder="https://admin.example.com" />
        </div>
        <div className="form-group">
          <label>LGSM 目录</label>
          <input className="form-control" value={lgsmDir} onChange={(e) => setLgsmDir(e.target.value)} placeholder="/home/steam/lgsm" />
        </div>
        <div className="form-group">
          <label>实例清单（逗号分隔）</label>
          <input className="form-control" value={instances} onChange={(e) => setInstances(e.target.value)} placeholder="csgoserver,csgo2server" />
        </div>
        <pre style={{ fontSize: 12.5, background: 'var(--surface2)', border: '1px solid var(--border)', borderRadius: 8, padding: '12px 14px', overflowX: 'auto', whiteSpace: 'pre-wrap', wordBreak: 'break-all', marginBottom: 8 }}>{installCommand}</pre>
        <div style={{ display: 'flex', gap: 8 }}>
          <button className="btn btn-sm" onClick={() => handleCopy(installCommand, '安装命令')}>复制安装命令</button>
        </div>
        <div className="info-box warning" style={{ marginTop: 12 }}>
          Agent 口令签发功能待开发：执行时先将命令中的 `&lt;Agent口令&gt;` 替换为后续签发的口令；
          配置文件写入 <code style={{ fontSize: 12, background: 'var(--surface2)', padding: '2px 6px', borderRadius: 4 }}>/etc/lumi-agent.env</code>（权限 600），请勿泄露。
        </div>
      </Card>

      <Card title="待开发" subtitle="电源控制闭环后续上线">
        <ul style={{ margin: 0, paddingLeft: 20, fontSize: 13, color: 'var(--text2)', lineHeight: 1.7 }}>
          <li>宿主机在线状态与实例清单展示</li>
          <li>服务器电源操作：重启 / 开机 / 关机（含任务状态与审计）</li>
          <li>Agent 口令签发与轮换</li>
        </ul>
      </Card>
    </div>
  );
}

import { useState } from 'react';
import { api } from '../../lib/api.js';
import { useAsync } from '../../shared/useAsync.js';
import { useAuth } from '../../state/store.js';
import { useToast } from '../../shared/Toast.jsx';
import { MetricCard } from '../../shared/MetricCard.jsx';
import { Modal } from '../../shared/Modal.jsx';
import { useConfirmDialog } from '../../shared/ConfirmModal.jsx';
import { TableLoading, TableError, TableEmpty } from '../../shared/TableState.jsx';
import { formatChinaDateTime } from '../../shared/time.js';
import { downloadHostAgentFile, normalizeSetupResponse } from './hostAgent.js';
import { HeartbeatTrendChart, PowerTaskTrendChart } from './HostAgentCharts.jsx';

function normalizeAgents(payload) {
  const agents = Array.isArray(payload?.agents) ? payload.agents : [];
  return agents.map((item) => ({
    id: item.id ?? '',
    hostname: item.hostname || '(未上报主机名)',
    displayName: item.display_name || '',
    lgsmDir: item.lgsm_dir || '',
    instances: Array.isArray(item.instances) ? item.instances : [],
    disabled: item.disabled === true,
    online: item.online === true,
    lastSeenAt: item.last_seen_at ?? null,
  }));
}

export function displayHostName(host) {
  return host.displayName || host.hostname;
}

export function HostAgentPage() {
  const { session } = useAuth();
  const { toast } = useToast();
  const { confirm, dialog } = useConfirmDialog();
  const token = session?.token ?? null;
  const isDeveloper = session?.role === 'developer';

  const [refreshKey, setRefreshKey] = useState(0);
  const [downloading, setDownloading] = useState(false);
  const [installToken, setInstallToken] = useState(null);
  const [issuing, setIssuing] = useState(false);
  const [renameModal, setRenameModal] = useState({ open: false, agentId: null, name: '' });
  const [acting, setActing] = useState(null);

  const setupState = useAsync(() => api.hostAgentSetup(token), [token, refreshKey]);
  const setup = setupState.data ? normalizeSetupResponse(setupState.data) : null;
  const installFile = setup?.files.find((file) => file.name === 'install.sh');

  const dataState = useAsync(async () => {
    const [overviewRes, agentsRes] = await Promise.all([
      api.hostAgentOverview(token),
      api.hostAgents(token),
    ]);
    return {
      overview: overviewRes?.overview ?? { hosts: 0, instances: 0, online: 0, pending_jobs: 0 },
      agents: normalizeAgents(agentsRes),
    };
  }, [token, refreshKey]);
  const overview = dataState.data?.overview ?? { hosts: 0, instances: 0, online: 0, pending_jobs: 0 };
  const agents = dataState.data?.agents ?? [];
  const offline = overview.hosts - overview.online;
  const loading = setupState.loading || dataState.loading;
  const loadError = setupState.error ?? dataState.error;

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

  async function handleIssueInstallToken() {
    try {
      setIssuing(true);
      const response = await api.createHostInstallToken(token);
      setInstallToken({ token: response?.token ?? '', expiresAt: response?.expires_at ?? '' });
      toast({ title: '签发成功', message: '安装口令 15 分钟有效，仅显示一次，请立即复制。' });
    } catch (error) {
      toast({ title: '签发失败', message: error.message, tone: 'danger' });
    } finally {
      setIssuing(false);
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

  function refresh() {
    setRefreshKey((v) => v + 1);
  }

  async function handleToggleDisabled(host) {
    const next = !host.disabled;
    const confirmed = await confirm({
      title: next ? '停用宿主机' : '启用宿主机',
      message: next
        ? `停用后「${displayHostName(host)}」不再领取电源任务，已下发任务不受影响，确定继续？`
        : `确定重新启用「${displayHostName(host)}」？`,
      confirmText: next ? '停用' : '启用',
    });
    if (!confirmed) return;
    try {
      setActing(host.id);
      await api.updateHostAgent(token, host.id, { disabled: next });
      toast({ title: next ? '已停用' : '已启用', message: displayHostName(host) });
      refresh();
    } catch (error) {
      toast({ title: '操作失败', message: error.message, tone: 'danger' });
    } finally {
      setActing(null);
    }
  }

  async function handleDeleteHost(host) {
    const confirmed = await confirm({
      title: '删除宿主机',
      message: `确定删除「${displayHostName(host)}」？已绑定的服务器将自动解绑，历史任务保留，确定继续？`,
      confirmText: '删除',
    });
    if (!confirmed) return;
    try {
      setActing(host.id);
      await api.deleteHostAgent(token, host.id);
      toast({ title: '已删除', message: displayHostName(host) });
      refresh();
    } catch (error) {
      toast({ title: '删除失败', message: error.message, tone: 'danger' });
    } finally {
      setActing(null);
    }
  }

  async function handleSaveRename() {
    if (!renameModal.agentId) return;
    if (renameModal.name.trim().length > 64) {
      toast({ title: '保存失败', message: '备注名最多 64 个字符。', tone: 'danger' });
      return;
    }
    try {
      setActing(renameModal.agentId);
      await api.updateHostAgent(token, renameModal.agentId, { display_name: renameModal.name.trim() });
      toast({ title: '保存成功', message: '备注名已更新。' });
      setRenameModal({ open: false, agentId: null, name: '' });
      refresh();
    } catch (error) {
      toast({ title: '保存失败', message: error.message, tone: 'danger' });
    } finally {
      setActing(null);
    }
  }

  return (
    <div id="host-agent" className="content-section active">
      <div className="breadcrumb"><span>核心管理</span><span className="sep">›</span><span className="current">Agent控制</span></div>
      <div className="page-header">
        <div><div className="page-title">Agent控制</div><div className="page-sub">宿主机 Agent 运行监控：一台宿主机跑一个 Agent，管理本机全部 LGSM 实例。</div></div>
        <button className="btn btn-outline" onClick={() => setRefreshKey((v) => v + 1)} disabled={loading}>
          {loading ? '刷新中...' : '刷新'}
        </button>
      </div>

      <div className="metric-grid">
        <MetricCard label="宿主机" value={overview.hosts} badge={`${overview.online} 在线`} />
        <MetricCard label="受管实例" value={overview.instances} badge={`${overview.hosts} 台宿主机`} />
        <MetricCard label="在线 Agent" value={`${overview.online}/${overview.hosts}`} badge={offline ? `${offline} 台离线` : '全部在线'} accent={offline > 0} />
        <MetricCard label="待执行任务" value={overview.pending_jobs} badge={overview.pending_jobs ? '有任务排队' : '队列为空'} accent={overview.pending_jobs > 0} />
      </div>

      <div className="dash-charts-grid">
        <HeartbeatTrendChart />
        <PowerTaskTrendChart />
      </div>

      <div className="lower-grid ops-lower-grid">
        <div className="card">
          <div className="card-header">
            <div>
              <div className="card-title">宿主机</div>
              <div className="card-sub">共 {overview.hosts} 台，{overview.online} 台在线（停用后不再下发任务）</div>
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
                    {isDeveloper ? <th className="text-right">操作</th> : null}
                  </tr>
                </thead>
                <tbody>
                  {loading ? <TableLoading colSpan={isDeveloper ? 5 : 4} text="正在加载宿主机数据..." /> : null}
                  {!loading && loadError ? (
                    <TableError colSpan={isDeveloper ? 5 : 4} message={loadError.message} />
                  ) : null}
                  {!loading && !loadError && agents.length === 0 ? (
                    <TableEmpty colSpan={isDeveloper ? 5 : 4} text="暂无宿主机接入，完成右侧快速上手后自动出现。" />
                  ) : null}
                  {!loading && !loadError && agents.map((host) => (
                    <tr key={host.id}>
                      <td className="fw-600 mobile-card-primary" data-label="宿主机">
                        {displayHostName(host)}
                        <div className="text-muted-light" style={{ fontWeight: 400, fontSize: 12 }}>{host.hostname}{host.lgsmDir ? ` · ${host.lgsmDir}` : ''}</div>
                      </td>
                      <td data-label="受管实例">
                        {host.instances.length} 个
                        {host.instances.length ? <div className="text-muted-light" style={{ fontSize: 12 }}>{host.instances.join('、')}</div> : null}
                      </td>
                      <td data-label="状态">
                        {host.disabled
                          ? <span className="status-pill pill-default">已停用</span>
                          : host.online
                            ? <span className="status-pill pill-online">在线</span>
                            : <span className="status-pill pill-danger">离线</span>}
                      </td>
                      <td data-label="最近心跳">{host.lastSeenAt ? formatChinaDateTime(host.lastSeenAt) : <span className="text-muted-light">—</span>}</td>
                      {isDeveloper ? (
                        <td className="text-right mobile-card-actions" data-label="操作">
                          <div className="action-btn-group" style={{ justifyContent: 'flex-end' }}>
                            <button
                              className="action-btn"
                              disabled={acting === host.id}
                              onClick={() => setRenameModal({ open: true, agentId: host.id, name: host.displayName })}
                            >
                              重命名
                            </button>
                            <button
                              className="action-btn"
                              disabled={acting === host.id}
                              onClick={() => handleToggleDisabled(host)}
                            >
                              {host.disabled ? '启用' : '停用'}
                            </button>
                            <button
                              className="action-btn action-btn-danger"
                              disabled={acting === host.id}
                              onClick={() => handleDeleteHost(host)}
                            >
                              删除
                            </button>
                          </div>
                        </td>
                      ) : null}
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
              <li>在下方签发安装口令（15 分钟有效，一次性），下载安装脚本传到宿主机。</li>
              <li>切换到 LGSM 属主用户（如 <code style={{ fontSize: 12, background: 'var(--surface2)', padding: '2px 6px', borderRadius: 4 }}>su - steam</code>，不要用 root），运行 <code style={{ fontSize: 12, background: 'var(--surface2)', padding: '2px 6px', borderRadius: 4 }}>bash install.sh</code> 按提示填写（实例支持逗号分隔多个）。</li>
              <li>用 <code style={{ fontSize: 12, background: 'var(--surface2)', padding: '2px 6px', borderRadius: 4 }}>tail -f ~/lumi-agent/agent.log</code> 确认启动。</li>
              <li>回到本页查看宿主机上线。</li>
            </ol>
            <button
              className="btn btn-primary"
              style={{ width: '100%' }}
              disabled={downloading || !installFile?.available}
              onClick={handleDownloadInstall}
            >
              {downloading ? '下载中...' : '下载 install.sh'}
            </button>
            {isDeveloper ? (
              <div style={{ marginTop: 12 }}>
                <button
                  className="btn btn-outline"
                  style={{ width: '100%' }}
                  disabled={issuing}
                  onClick={handleIssueInstallToken}
                >
                  {issuing ? '签发中...' : '签发安装口令（15 分钟有效）'}
                </button>
                {installToken ? (
                  <div className="info-box warning" style={{ marginTop: 8 }}>
                    <div style={{ marginBottom: 6 }}>口令仅显示一次，请立即复制{installToken.expiresAt ? `（过期 ${formatChinaDateTime(installToken.expiresAt)}）` : ''}：</div>
                    <code style={{ fontSize: 12, background: 'var(--surface2)', padding: '2px 6px', borderRadius: 4, wordBreak: 'break-all' }}>{installToken.token}</code>
                    <div style={{ marginTop: 8 }}>
                      <button className="btn btn-sm" onClick={() => handleCopy(installToken.token, '安装口令')}>复制口令</button>
                    </div>
                  </div>
                ) : null}
              </div>
            ) : (
              <div className="info-box warning" style={{ marginTop: 12 }}>
                安装口令需 developer 签发，请联系开发管理员获取。
              </div>
            )}
          </div>
        </div>
      </div>

      <Modal
        open={renameModal.open}
        title="重命名宿主机"
        onClose={() => setRenameModal({ open: false, agentId: null, name: '' })}
        footer={(
          <>
            <button className="btn btn-outline" onClick={() => setRenameModal({ open: false, agentId: null, name: '' })}>取消</button>
            <button className="btn btn-primary" disabled={acting === renameModal.agentId} onClick={handleSaveRename}>保存</button>
          </>
        )}
      >
        <div className="form-group">
          <label>备注名（最多 64 个字符，留空则显示上报主机名）</label>
          <input
            type="text"
            className="form-control"
            placeholder="例如：主服-电信"
            value={renameModal.name}
            onChange={(e) => setRenameModal((prev) => ({ ...prev, name: e.target.value }))}
          />
        </div>
      </Modal>
      {dialog}
    </div>
  );
}

import React, { useEffect, useMemo, useState } from 'react';
import { api } from '../../lib/api.js';
import { useAsync } from '../../shared/useAsync.js';
import { useAuth } from '../../state/auth.jsx';
import { useToast, ToastContainer } from '../../shared/Toast.jsx';
import { useConfirmDialog } from '../../shared/ConfirmModal.jsx';
import { formatChinaMonthDayTime } from '../../shared/time.js';

const defaultSources = [
  'https://files.femboykz.com/fastdl/csgo/maps/',
  'https://download.axekz.com/csgo/maps/',
];

const defaultForm = {
  enabled: false,
  autoUpdate: true,
  sourceUrls: defaultSources.join('\n'),
  mapPoolUrl: 'https://kztimerglobal.com/api/v1.0/maps?is_validated=true&limit=999',
  checkIntervalSecs: 3600,
};

const defaultAgentForm = { name: '', targetType: 'game' };

function linesToArray(value) {
  return value
    .split('\n')
    .map((item) => item.trim())
    .filter(Boolean);
}

function targetTypeLabel(value) {
  return value === 'download' ? '下载站' : '游戏服';
}

function statusLabel(value) {
  const labels = {
    pending: '等待执行',
    running: '执行中',
    succeeded: '已完成',
    failed: '失败',
  };
  return labels[value] ?? value ?? '-';
}

function taskTone(value) {
  if (value === 'succeeded') return 'pill-online';
  if (value === 'failed') return 'pill-offline';
  return 'pill-warning';
}

export function MapSyncPage() {
  const { session } = useAuth();
  const token = session?.token ?? null;
  const { toast, toasts, dismiss } = useToast();
  const { confirm, dialog } = useConfirmDialog();

  const [refreshKey, setRefreshKey] = useState(0);
  const [form, setForm] = useState(defaultForm);
  const [agentForm, setAgentForm] = useState(defaultAgentForm);
  const [singleMap, setSingleMap] = useState('');
  const [saving, setSaving] = useState(false);
  const [creatingAgent, setCreatingAgent] = useState(false);
  const [checking, setChecking] = useState(false);
  const [singleSyncing, setSingleSyncing] = useState(false);
  const [resettingToken, setResettingToken] = useState(null);

  const overviewState = useAsync(() => api.mapSyncOverview(token), [token, refreshKey]);
  const overview = overviewState.data?.overview;
  const config = overview?.config;
  const agents = overview?.agents ?? [];
  const recentTasks = useMemo(() => overview?.recent_tasks ?? [], [overview]);
  const mapPoolNames = overview?.map_pool_names ?? [];

  useEffect(() => {
    if (!config) return;
    React.startTransition(() => {
      setForm({
        enabled: config.enabled,
        autoUpdate: config.auto_update,
        sourceUrls: (config.source_urls?.length ? config.source_urls : defaultSources).join('\n'),
        mapPoolUrl: config.map_pool_url || defaultForm.mapPoolUrl,
        checkIntervalSecs: config.check_interval_secs ?? 3600,
      });
    });
  }, [config]);

  const pendingCount = useMemo(
    () => recentTasks.filter((task) => task.status === 'pending' || task.status === 'running').length,
    [recentTasks],
  );

  async function saveConfig() {
    try {
      setSaving(true);
      await api.updateMapSyncConfig(token, {
        enabled: form.enabled,
        auto_update: form.autoUpdate,
        source_urls: linesToArray(form.sourceUrls),
        map_pool_url: form.mapPoolUrl,
        check_interval_secs: Number(form.checkIntervalSecs) || 3600,
      });
      toast({ title: '配置已保存' });
      setRefreshKey((key) => key + 1);
    } catch (error) {
      toast({ title: '保存失败', message: error.message, tone: 'danger' });
    } finally {
      setSaving(false);
    }
  }

  async function createAgent() {
    if (!agentForm.name.trim()) {
      toast({ title: '创建失败', message: '请填写代理名称', tone: 'danger' });
      return;
    }
    try {
      setCreatingAgent(true);
      await api.createMapSyncAgent(token, {
        name: agentForm.name,
        target_type: agentForm.targetType,
        enabled: true,
      });
      setAgentForm(defaultAgentForm);
      toast({ title: '代理已创建' });
      setRefreshKey((key) => key + 1);
    } catch (error) {
      toast({ title: '创建失败', message: error.message, tone: 'danger' });
    } finally {
      setCreatingAgent(false);
    }
  }

  async function deleteAgent(agent) {
    const ok = await confirm({
      title: '删除地图同步代理',
      message: `确定要删除「${agent.name}」吗？相关库存和任务也会删除。`,
      confirmText: '确认删除',
    });
    if (!ok) return;
    try {
      await api.deleteMapSyncAgent(token, agent.id);
      toast({ title: '代理已删除' });
      setRefreshKey((key) => key + 1);
    } catch (error) {
      toast({ title: '删除失败', message: error.message, tone: 'danger' });
    }
  }

  async function resetToken(agent) {
    const ok = await confirm({
      title: '重置代理 Token',
      message: `重置后「${agent.name}」当前部署的远端脚本会失效。`,
      confirmText: '确认重置',
    });
    if (!ok) return;
    try {
      setResettingToken(agent.id);
      await api.resetMapSyncAgentToken(token, agent.id);
      toast({ title: 'Token 已重置' });
      setRefreshKey((key) => key + 1);
    } catch (error) {
      toast({ title: '重置失败', message: error.message, tone: 'danger' });
    } finally {
      setResettingToken(null);
    }
  }

  async function checkMaps() {
    try {
      setChecking(true);
      const payload = await api.checkMaps(token);
      const result = payload.result;
      toast({
        title: '检测完成',
        message: `发现 ${result.maps_found} 张地图，创建 ${result.tasks_created} 个更新任务`,
      });
      setRefreshKey((key) => key + 1);
    } catch (error) {
      toast({ title: '检测失败', message: error.message, tone: 'danger' });
    } finally {
      setChecking(false);
    }
  }

  async function syncSingleMap() {
    if (!singleMap.trim()) {
      toast({ title: '更新失败', message: '请填写地图名', tone: 'danger' });
      return;
    }
    try {
      setSingleSyncing(true);
      const payload = await api.syncSingleMap(token, singleMap.trim());
      const result = payload.result;
      toast({
        title: '单图任务已创建',
        message: `创建 ${result.tasks_created} 个任务，跳过 ${result.skipped_files} 个目标`,
      });
      setSingleMap('');
      setRefreshKey((key) => key + 1);
    } catch (error) {
      toast({ title: '更新失败', message: error.message, tone: 'danger' });
    } finally {
      setSingleSyncing(false);
    }
  }

  return (
    <div id="map-sync" className="content-section active">
      <div className="breadcrumb">
        <span>系统功能</span><span className="sep">›</span><span className="current">地图同步</span>
      </div>

      <div className="page-header">
        <div>
          <div className="page-title">地图同步</div>
          <div className="page-sub">检测镜像源地图变化，向游戏服务器和下载站代理下发更新任务。</div>
        </div>
        <div className="action-btn-group">
          <button className="btn btn-outline" onClick={() => setRefreshKey((key) => key + 1)}>刷新</button>
          <button className="btn btn-primary" onClick={checkMaps} disabled={checking}>
            {checking ? '检测中...' : '检测并创建任务'}
          </button>
        </div>
      </div>

      <div style={{ display: 'grid', gridTemplateColumns: 'minmax(0, 1.1fr) minmax(320px, 0.9fr)', gap: 16, alignItems: 'start' }}>
        <div className="card">
          <div className="card-header">
            <div>
              <h3>同步配置</h3>
              <p>定时检测只创建远端任务，实际下载由代理在服务器本机完成。</p>
            </div>
          </div>
          <div className="card-body" style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
            <div className="form-group">
              <label>地图源 URL</label>
              <textarea
                className="form-control"
                rows={4}
                value={form.sourceUrls}
                onChange={(event) => setForm((prev) => ({ ...prev, sourceUrls: event.target.value }))}
              />
            </div>
            <div className="form-group">
              <label>GOKZ 地图池 API</label>
              <input
                className="form-control"
                value={form.mapPoolUrl}
                onChange={(event) => setForm((prev) => ({ ...prev, mapPoolUrl: event.target.value }))}
              />
            </div>
            <div className="form-group">
              <label>检测间隔（秒）</label>
              <input
                className="form-control"
                type="number"
                min={60}
                max={86400}
                value={form.checkIntervalSecs}
                onChange={(event) => setForm((prev) => ({ ...prev, checkIntervalSecs: event.target.value }))}
              />
            </div>
            <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(220px, 1fr))', gap: 12 }}>
              <div className="toggle-row" style={{ borderTop: 'none', padding: '8px 12px', border: '1px solid var(--border)', borderRadius: 10 }}>
                <div>
                  <div className="toggle-label">启用定时检测</div>
                </div>
                <label className="toggle-switch" aria-label="启用定时检测">
                  <input type="checkbox" checked={form.enabled} onChange={(event) => setForm((prev) => ({ ...prev, enabled: event.target.checked }))} />
                  <span className="toggle-slider" />
                </label>
              </div>
              <div className="toggle-row" style={{ borderTop: 'none', padding: '8px 12px', border: '1px solid var(--border)', borderRadius: 10 }}>
                <div>
                  <div className="toggle-label">自动创建更新任务</div>
                </div>
                <label className="toggle-switch" aria-label="自动创建更新任务">
                  <input type="checkbox" checked={form.autoUpdate} onChange={(event) => setForm((prev) => ({ ...prev, autoUpdate: event.target.checked }))} />
                  <span className="toggle-slider" />
                </label>
              </div>
            </div>
            <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', gap: 12 }}>
              <div style={{ fontSize: 12, color: 'var(--text3)' }}>
                上次检测：{formatChinaMonthDayTime(config?.last_checked_at)} · 状态：{config?.last_status ?? '-'}
                {config?.last_error ? ` · ${config.last_error}` : ''}
              </div>
              <button className="btn btn-primary" onClick={saveConfig} disabled={saving}>{saving ? '保存中...' : '保存配置'}</button>
            </div>
          </div>
        </div>

        <div className="card">
          <div className="card-header">
            <div>
              <h3>单图更新</h3>
              <p>按地图名给所有匹配代理创建更新任务。</p>
            </div>
          </div>
          <div className="card-body" style={{ display: 'flex', gap: 10 }}>
            <input
              className="form-control"
              value={singleMap}
              onChange={(event) => setSingleMap(event.target.value)}
              placeholder="kz_example 或 kz_example.bsp"
            />
            <button className="btn btn-primary" onClick={syncSingleMap} disabled={singleSyncing}>
              {singleSyncing ? '创建中...' : '更新'}
            </button>
          </div>
        </div>
      </div>

      <div className="card mt-16">
        <div className="card-header">
          <div>
            <h3>GOKZ 地图池</h3>
            <p>当前缓存地图 {mapPoolNames.length} 张，maplist.txt 和 mapcycle.txt 会按此列表同步。</p>
          </div>
        </div>
        <div className="card-body">
          <textarea
            className="form-control"
            rows={8}
            readOnly
            value={mapPoolNames.join('\n')}
            placeholder="执行一次检测后显示 GOKZ 地图池"
          />
        </div>
      </div>

      <div className="card mt-16">
        <div className="card-header">
          <div>
            <h3>远端代理</h3>
            <p>游戏服代理领取 .bsp，下载站代理领取 .bsp 和 .bsp.bz2。</p>
          </div>
        </div>
        <div className="card-body" style={{ display: 'flex', flexDirection: 'column', gap: 14 }}>
          <div style={{ display: 'grid', gridTemplateColumns: 'minmax(180px, 1fr) 160px 120px', gap: 10 }}>
            <input
              className="form-control"
              value={agentForm.name}
              onChange={(event) => setAgentForm((prev) => ({ ...prev, name: event.target.value }))}
              placeholder="代理名称，例如 KZ-1 游戏服"
            />
            <select
              className="form-control"
              value={agentForm.targetType}
              onChange={(event) => setAgentForm((prev) => ({ ...prev, targetType: event.target.value }))}
            >
              <option value="game">游戏服</option>
              <option value="download">下载站</option>
            </select>
            <button className="btn btn-primary" onClick={createAgent} disabled={creatingAgent}>
              {creatingAgent ? '创建中...' : '创建代理'}
            </button>
          </div>

          <div className="table-responsive">
            <table className="data-table">
              <thead>
                <tr>
                  <th>名称</th>
                  <th>类型</th>
                  <th>Token</th>
                  <th>上次在线</th>
                  <th>库存上报</th>
                  <th>操作</th>
                </tr>
              </thead>
              <tbody>
                {overviewState.loading && <tr><td colSpan={6} style={{ textAlign: 'center', color: 'var(--text3)' }}>加载中...</td></tr>}
                {!overviewState.loading && agents.length === 0 && <tr><td colSpan={6} style={{ textAlign: 'center', color: 'var(--text3)' }}>暂无代理。</td></tr>}
                {agents.map((agent) => (
                  <tr key={agent.id}>
                    <td className="fw-600">{agent.name}</td>
                    <td>{targetTypeLabel(agent.target_type)}</td>
                    <td className="steam-id text-ellipsis-260">{agent.token}</td>
                    <td>{formatChinaMonthDayTime(agent.last_seen_at)}</td>
                    <td>{formatChinaMonthDayTime(agent.last_inventory_at)}</td>
                    <td>
                      <div className="action-btn-group">
                        <button className="action-btn" onClick={() => resetToken(agent)} disabled={resettingToken === agent.id}>重置 Token</button>
                        <button className="action-btn action-btn-danger" onClick={() => deleteAgent(agent)}>删除</button>
                      </div>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      </div>

      <div className="card mt-16">
        <div className="card-header">
          <div>
            <h3>最近任务</h3>
            <p>当前待执行任务 {pendingCount} 个。</p>
          </div>
        </div>
        <div className="card-body p-0">
          <div className="table-responsive">
            <table className="data-table">
              <thead>
                <tr>
                  <th>代理</th>
                  <th>地图</th>
                  <th>文件</th>
                  <th>状态</th>
                  <th>原因</th>
                  <th>更新时间</th>
                </tr>
              </thead>
              <tbody>
                {recentTasks.length === 0 && <tr><td colSpan={6} style={{ textAlign: 'center', color: 'var(--text3)' }}>暂无任务。</td></tr>}
                {recentTasks.map((task) => (
                  <tr key={task.id}>
                    <td>{task.agent_name ?? '-'}</td>
                    <td>{task.map_name}</td>
                    <td className="steam-id">{task.file_name}</td>
                    <td><span className={`status-pill ${taskTone(task.status)}`}>{statusLabel(task.status)}</span></td>
                    <td>{task.error || task.reason}</td>
                    <td>{formatChinaMonthDayTime(task.updated_at)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      </div>

      {dialog}
      <ToastContainer toasts={toasts} onDismiss={dismiss} />
    </div>
  );
}

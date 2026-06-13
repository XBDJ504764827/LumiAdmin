import React, { useState } from 'react';
import { api } from '../../lib/api.js';
import { useAsync } from '../../shared/useAsync.js';
import { useAuth } from '../../state/auth.jsx';
import { useToast, ToastContainer } from '../../shared/Toast.jsx';
import { TableLoading, TableEmpty } from '../../shared/TableState.jsx';
import { useConfirmDialog } from '../../shared/ConfirmModal.jsx';
import { formatChinaMonthDayTime } from '../../shared/time.js';

function normalizeServer(item) {
  return {
    id: item.id,
    name: item.name,
    ip: item.ip,
    port: item.port,
    rconPassword: item.rcon_password,
    enabled: item.enabled,
    pollInterval: item.poll_interval ?? 30,
    lastQueriedAt: item.last_queried_at,
    createdAt: item.created_at,
    serverName: item.server_name ?? '',
    currentMap: item.current_map ?? '',
    playerCount: item.player_count ?? 0,
    maxPlayers: item.max_players ?? 0,
    players: item.players ?? [],
    statusQueriedAt: item.status_queried_at ?? null,
  };
}

const defaultForm = { name: '', ip: '', port: 27015, rconPassword: '', enabled: true, pollInterval: 30 };

export function ExternalServerPage() {
  const { session } = useAuth();
  const { confirm, dialog } = useConfirmDialog();
  const { toast, toasts, dismiss } = useToast();
  const token = session?.token ?? null;

  const [modalOpen, setModalOpen] = useState(false);
  const [editingId, setEditingId] = useState(null);
  const [form, setForm] = useState(defaultForm);
  const [saving, setSaving] = useState(false);
  const [testing, setTesting] = useState(null);
  const [refreshKey, setRefreshKey] = useState(0);
  const [expandedPlayers, setExpandedPlayers] = useState(null);

  const serversState = useAsync(() => api.externalServers(token), [token, refreshKey]);
  const servers = (serversState.data?.items ?? []).map(normalizeServer);

  function openCreate() {
    setForm({ ...defaultForm });
    setEditingId(null);
    setModalOpen(true);
  }

  function openEdit(server) {
    setForm({
      name: server.name,
      ip: server.ip,
      port: server.port,
      rconPassword: '',
      enabled: server.enabled,
      pollInterval: server.pollInterval,
    });
    setEditingId(server.id);
    setModalOpen(true);
  }

  function closeModal() {
    setModalOpen(false);
    setForm(defaultForm);
    setEditingId(null);
  }

  async function handleSave() {
    if (!form.name.trim() || !form.ip.trim() || !form.port) {
      toast({ title: '保存失败', message: '请填写必填字段', tone: 'danger' });
      return;
    }
    const poll = Math.max(5, Math.min(3600, Number(form.pollInterval) || 30));
    try {
      setSaving(true);
      const body = {
        name: form.name,
        ip: form.ip,
        port: Number(form.port),
        enabled: form.enabled,
        poll_interval: poll,
      };
      if (form.rconPassword.trim()) body.rcon_password = form.rconPassword.trim();
      if (editingId) {
        await api.updateExternalServer(token, editingId, body);
        toast({ title: '更新成功' });
      } else {
        await api.createExternalServer(token, body);
        toast({ title: '添加成功' });
      }
      closeModal();
      setRefreshKey((k) => k + 1);
    } catch (e) {
      toast({ title: '保存失败', message: e.message, tone: 'danger' });
    } finally {
      setSaving(false);
    }
  }

  async function handleDelete(server) {
    const confirmed = await confirm({
      title: '删除外部服务器',
      message: `确定要删除「${server.name}」吗?`,
      confirmText: '确认删除',
    });
    if (!confirmed) return;
    try {
      await api.deleteExternalServer(token, server.id);
      toast({ title: '删除成功' });
      setRefreshKey((k) => k + 1);
    } catch (e) {
      toast({ title: '删除失败', message: e.message, tone: 'danger' });
    }
  }

  async function handleTest(server) {
    try {
      setTesting(server.id);
      const result = await api.testExternalServer(token, server.id);
      if (result.result.ok) {
        const s = result.result.status;
        toast({ title: '连接成功', message: `地图: ${s?.current_map ?? '-'}, 玩家: ${s?.player_count ?? 0}/${s?.max_players ?? 0}` });
      } else {
        toast({ title: '连接失败', message: result.result.message, tone: 'danger' });
      }
    } catch (e) {
      toast({ title: '测试失败', message: e.message, tone: 'danger' });
    } finally {
      setTesting(null);
    }
  }

  return (
    <div id="external-servers" className="content-section active">
      <div className="breadcrumb">
        <span>系统功能</span><span className="sep">›</span><span className="current">外部服务器</span>
      </div>

      <div className="page-header">
        <div>
          <div className="page-title">外部服务器管理</div>
          <div className="page-sub">管理外部服务器,通过 A2S 协议自动查询信息,可选配置 RCON 密码获取更多详情。</div>
        </div>
        <button className="btn btn-primary" onClick={openCreate}>添加服务器</button>
      </div>

      <div className="card">
        <div className="card-body p-0">
          <div className="table-responsive">
            <table className="data-table">
              <thead>
                <tr>
                  <th>名称</th>
                  <th>地址</th>
                  <th>状态</th>
                  <th>地图</th>
                  <th>玩家</th>
                  <th>轮询间隔</th>
                  <th>上次查询</th>
                  <th>操作</th>
                </tr>
              </thead>
              <tbody>
                {serversState.loading && <TableLoading colSpan={8} text="正在加载外部服务器..." />}
                {serversState.error && <tr><td colSpan={8} className="table-state-cell"><div className="table-state-inner table-state-inner--error">{serversState.error.message}</div></td></tr>}
                {!serversState.loading && !serversState.error && servers.length === 0 && (
                  <TableEmpty colSpan={8} text="暂无外部服务器，点击上方“添加服务器”按钮创建。" />
                )}
                {servers.map((server) => (
                  <React.Fragment key={server.id}>
                    <tr>
                      <td className="fw-600">{server.name}{!server.enabled && <span className="tag-badge muted" style={{ marginLeft: 8 }}>已禁用</span>}</td>
                      <td className="steam-id">{server.ip}:{server.port}</td>
                      <td>
                        {server.statusQueriedAt
                          ? <span className="status-pill pill-online">已查询</span>
                          : <span className="text-muted-light">未查询</span>}
                      </td>
                      <td className="text-muted">{server.currentMap || '-'}</td>
                      <td>
                        <span
                          style={{ cursor: server.playerCount > 0 ? 'pointer' : 'default' }}
                          onClick={() => server.playerCount > 0 && setExpandedPlayers(expandedPlayers === server.id ? null : server.id)}
                        >
                          {server.playerCount}/{server.maxPlayers}
                          {server.playerCount > 0 && (
                            <span style={{ marginLeft: 6, fontSize: 11, color: 'var(--text3)' }}>
                              {expandedPlayers === server.id ? '▲' : '▼'}
                            </span>
                          )}
                        </span>
                      </td>
                      <td style={{ fontSize: 13, color: 'var(--text2)' }}>{server.pollInterval}s</td>
                      <td style={{ fontSize: 12, color: 'var(--text3)' }}>{formatChinaMonthDayTime(server.lastQueriedAt)}</td>
                      <td>
                        <div className="action-btn-group">
                          <button className="action-btn action-btn-accent" onClick={() => handleTest(server)} disabled={testing === server.id}>
                            {testing === server.id ? '...' : '测试'}
                          </button>
                          <button className="action-btn" onClick={() => openEdit(server)}>编辑</button>
                          <button className="action-btn action-btn-danger" onClick={() => handleDelete(server)}>删除</button>
                        </div>
                      </td>
                    </tr>
                    {expandedPlayers === server.id && server.players.length > 0 && (
                      <tr>
                        <td colSpan={8} className="p-0">
                          <div style={{
                            display: 'flex',
                            flexWrap: 'wrap',
                            gap: '6px 12px',
                            padding: '10px 16px 12px',
                            background: 'var(--bg2)',
                            borderTop: '1px solid var(--border)',
                          }}>
                            {server.players.map((name, idx) => (
                              <span key={idx} className="tag-badge muted">
                                border: '1px solid var(--border)',
                              }}>
                                {name}
                              </span>
                            ))}
                          </div>
                        </td>
                      </tr>
                    )}
                  </React.Fragment>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      </div>

      {modalOpen && (
        <div className="modal-overlay active" onClick={closeModal}>
          <div className="modal" style={{ maxWidth: 520 }} onClick={(e) => e.stopPropagation()}>
            <div className="modal-header">
              <h2 style={{ fontSize: 16, fontWeight: 600, margin: 0 }}>{editingId ? '编辑外部服务器' : '添加外部服务器'}</h2>
              <span style={{ cursor: 'pointer', color: 'var(--text3)', fontSize: 18 }} onClick={closeModal}>&#10005;</span>
            </div>
            <div className="modal-body" style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
              <div className="form-group">
                <label>名称 <span className="text-accent">*</span></label>
                <input type="text" className="form-control" value={form.name} onChange={(e) => setForm((p) => ({ ...p, name: e.target.value }))} placeholder="服务器显示名称" />
              </div>

              <div style={{ display: 'grid', gridTemplateColumns: '2fr 1fr', gap: 12 }}>
                <div className="form-group">
                  <label>IP 地址 <span className="text-accent">*</span></label>
                  <input type="text" className="form-control" value={form.ip} onChange={(e) => setForm((p) => ({ ...p, ip: e.target.value }))} placeholder="192.168.1.100" />
                </div>
                <div className="form-group">
                  <label>端口 <span className="text-accent">*</span></label>
                  <input type="number" className="form-control" value={form.port} onChange={(e) => setForm((p) => ({ ...p, port: e.target.value }))} />
                </div>
              </div>

              <div className="form-group">
                <label>RCON 密码</label>
                <input type="password" className="form-control" value={form.rconPassword} onChange={(e) => setForm((p) => ({ ...p, rconPassword: e.target.value }))} placeholder={editingId ? '留空保持不变' : '可选,留空仅使用 A2S 查询'} />
                <div style={{ fontSize: 12, color: 'var(--text3)', marginTop: 4 }}>配置后可通过 RCON 获取更多服务器详情</div>
              </div>

              <div className="form-group">
                <label>轮询间隔(秒)</label>
                <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
                  <input
                    type="number"
                    className="form-control"
                    style={{ width: 100 }}
                    min={5}
                    max={3600}
                    value={form.pollInterval}
                    onChange={(e) => setForm((p) => ({ ...p, pollInterval: e.target.value }))}
                  />
                  <span style={{ fontSize: 13, color: 'var(--text3)' }}>范围 5 - 3600 秒,默认 30 秒</span>
                </div>
              </div>

              <div className="form-group" style={{ flexDirection: 'row', alignItems: 'center', gap: 10, marginBottom: 0 }}>
                <label className="toggle-switch">
                  <input type="checkbox" checked={form.enabled} onChange={(e) => setForm((p) => ({ ...p, enabled: e.target.checked }))} />
                  <span className="toggle-slider" />
                </label>
                <span style={{ fontSize: 14, color: 'var(--text2)' }}>启用轮询</span>
              </div>
            </div>
            <div className="modal-footer">
              <button className="btn btn-outline" onClick={closeModal}>取消</button>
              <button className="btn btn-primary" onClick={handleSave} disabled={saving}>
                {saving ? '保存中...' : editingId ? '确认更新' : '确认添加'}
              </button>
            </div>
          </div>
        </div>
      )}

      {dialog}
      <ToastContainer toasts={toasts} onDismiss={dismiss} />
    </div>
  );
}

import React, { useEffect, useState } from 'react';
import { api } from '../../lib/api.js';
import { useAsync } from '../../shared/useAsync.js';
import { useAuth } from '../../state/auth.jsx';
import { useToast, ToastContainer } from '../../shared/Toast.jsx';
import { useConfirmDialog } from '../../shared/ConfirmModal.jsx';
import {
  buildPlayerApiConfigPayload,
  defaultPlayerApiConfig,
  defaultWebhookConfig,
  flattenServerOptions,
  flattenExternalServerOptions,
  normalizePlayerApiConfig,
  normalizePlayerApiRows,
} from './apiPages.js';

export function PlayerApiPage() {
  const { session } = useAuth();
  const { confirm, dialog } = useConfirmDialog();
  const { toast, toasts, dismiss: dismissToast } = useToast();
  const token = session?.token ?? null;
  const isDeveloper = session?.role === 'developer';
  const canConfigure = isDeveloper || session?.role === 'admin';

  const [form, setForm] = useState(defaultPlayerApiConfig);
  const [message, setMessage] = useState('');
  const [refreshKey, setRefreshKey] = useState(0);

  // Webhook 弹窗
  const [webhookModalOpen, setWebhookModalOpen] = useState(false);
  const [editingIndex, setEditingIndex] = useState(null);
  const [draftWebhook, setDraftWebhook] = useState(defaultWebhookConfig);
  const [saving, setSaving] = useState(false);

  const playersState = useAsync(() => api.playerApiPlayers(token), [token, refreshKey]);
  const configState = useAsync(() => api.playerApiConfig(token), [token, refreshKey]);
  const externalServersState = useAsync(() => api.externalServers(token), [token, refreshKey]);
  const serversState = useAsync(() => api.servers(token), [token]);

  useEffect(() => {
    if (configState.data?.config) {
      setForm(normalizePlayerApiConfig(configState.data.config));
    }
  }, [configState.data]);

  const rows = normalizePlayerApiRows(playersState.data?.items ?? []);
  const serverOptions = flattenServerOptions(serversState.data?.groups ?? []);
  const externalServerOptions = flattenExternalServerOptions(externalServersState.data?.items ?? []);

  // 按 groupId 分组
  const groupedServers = serverOptions.reduce((acc, s) => {
    const key = s.groupId;
    if (!acc[key]) acc[key] = { name: s.groupName, servers: [] };
    acc[key].servers.push(s);
    return acc;
  }, {});

  function openCreateWebhook() {
    if (form.items.length >= Number(form.maxApiCount)) {
      toast({ title: '无法添加', message: `已达到最大 Webhook 数量限制（${form.maxApiCount}）`, tone: 'danger' });
      return;
    }
    setDraftWebhook({ ...defaultWebhookConfig });
    setEditingIndex(null);
    setMessage('');
    setWebhookModalOpen(true);
  }

  function openEditWebhook(index) {
    setDraftWebhook({ ...form.items[index] });
    setEditingIndex(index);
    setMessage('');
    setWebhookModalOpen(true);
  }

  function closeWebhookModal() {
    setWebhookModalOpen(false);
    setDraftWebhook(defaultWebhookConfig);
    setEditingIndex(null);
    setMessage('');
  }

  function toggleServer(serverId) {
    setDraftWebhook((prev) => ({
      ...prev,
      serverIds: prev.serverIds.includes(serverId)
        ? prev.serverIds.filter((id) => id !== serverId)
        : [...prev.serverIds, serverId],
    }));
  }

  function toggleAllServers(groupId) {
    const groupServerIds = (groupedServers[groupId]?.servers ?? []).map((s) => s.id);
    const allSelected = groupServerIds.every((id) => draftWebhook.serverIds.includes(id));
    setDraftWebhook((prev) => ({
      ...prev,
      serverIds: allSelected
        ? prev.serverIds.filter((id) => !groupServerIds.includes(id))
        : [...new Set([...prev.serverIds, ...groupServerIds])],
    }));
  }

  function toggleExternalServer(serverId) {
    setDraftWebhook((prev) => ({
      ...prev,
      externalServerIds: prev.externalServerIds.includes(serverId)
        ? prev.externalServerIds.filter((id) => id !== serverId)
        : [...prev.externalServerIds, serverId],
    }));
  }

  function toggleAllExternalServers() {
    const allIds = externalServerOptions.map((s) => s.id);
    const allSelected = allIds.length > 0 && allIds.every((id) => draftWebhook.externalServerIds.includes(id));
    setDraftWebhook((prev) => ({
      ...prev,
      externalServerIds: allSelected ? [] : allIds,
    }));
  }

  function saveWebhookToForm() {
    if (!draftWebhook.webhookUrl.trim()) {
      setMessage('请输入 Webhook 接收地址。');
      return;
    }
    if (!draftWebhook.publicPath.trim()) {
      setMessage('请输入自定义访问后缀。');
      return;
    }

    if (editingIndex !== null) {
      setForm((prev) => ({
        ...prev,
        items: prev.items.map((item, i) => (i === editingIndex ? { ...draftWebhook } : item)),
      }));
      toast({ title: '已更新', message: 'Webhook 已更新，请点击"保存全部配置"生效。' });
    } else {
      setForm((prev) => ({ ...prev, items: [...prev.items, { ...draftWebhook }] }));
      toast({ title: '已添加', message: 'Webhook 已添加，请点击"保存全部配置"生效。' });
    }
    closeWebhookModal();
  }

  async function removeWebhook(index) {
    const item = form.items[index];
    const confirmed = await confirm({
      title: '移除 Webhook',
      message: `确定要移除 Webhook「${item.publicPath}」吗？`,
      confirmText: '确认移除',
    });
    if (!confirmed) return;
    setForm((prev) => ({ ...prev, items: prev.items.filter((_, i) => i !== index) }));
    toast({ title: '已移除', message: '请点击"保存全部配置"生效。' });
  }

  async function saveConfig() {
    try {
      setSaving(true);
      const payload = buildPlayerApiConfigPayload(form);
      const response = await api.updatePlayerApiConfig(token, payload);
      setForm(normalizePlayerApiConfig(response.config));
      setRefreshKey((v) => v + 1);
      toast({ title: '保存成功', message: 'API 分发配置已更新。' });
    } catch (e) {
      toast({ title: '保存失败', message: e.message, tone: 'danger' });
    } finally {
      setSaving(false);
    }
  }

  function statusPill(status) {
    if (!status) return <span style={{ color: 'var(--text3)' }}>未分发</span>;
    if (status === 'success' || status === 'ok') return <span className="status-pill pill-online">成功</span>;
    if (status === 'failed' || status === 'error') return <span className="status-pill pill-danger">失败</span>;
    return <span className="status-pill pill-warning">{status}</span>;
  }

  const hasUnsavedChanges = JSON.stringify(form) !== JSON.stringify(
    normalizePlayerApiConfig(configState.data?.config)
  );

  return (
    <div id="player-api" className="content-section active">
      <div className="breadcrumb">
        <span>系统功能</span>
        <span className="sep">›</span>
        <span className="current">玩家信息 API</span>
      </div>

      <div className="page-header">
        <div>
          <div className="page-title">实时玩家数据分发 API</div>
          <div className="page-sub">展示插件上报的在线玩家信息，并按配置周期将数据分发到外部 Webhook。</div>
        </div>
        {canConfigure && (
          <button className="btn btn-primary" onClick={openCreateWebhook}>
            添加 Webhook
          </button>
        )}
      </div>

      {/* ── 在线玩家数据 ── */}
      <div className="card">
        <div className="card-header" style={{ borderBottom: 'none' }}>
          <div>
            <div className="card-title">在线玩家数据</div>
            <div className="card-sub">当前通过插件上报的实时在线玩家信息</div>
          </div>
        </div>
        <div className="card-body" style={{ padding: 0 }}>
          {playersState.error ? (
            <div style={{ padding: 20, textAlign: 'center', color: 'var(--accent)' }}>{playersState.error.message}</div>
          ) : (
            <div className="table-responsive">
              <table className="data-table">
                <thead>
                  <tr>
                    <th>玩家名称</th>
                    <th>SteamID</th>
                    <th>IP 地址</th>
                    <th>所在服务器</th>
                    <th>同步时间</th>
                  </tr>
                </thead>
                <tbody>
                  {playersState.loading ? (
                    <tr><td colSpan={5} style={{ textAlign: 'center', color: 'var(--text3)' }}>加载中...</td></tr>
                  ) : rows.length === 0 ? (
                    <tr><td colSpan={5} style={{ textAlign: 'center', color: 'var(--text3)' }}>暂无在线玩家数据</td></tr>
                  ) : (
                    rows.map((row, idx) => (
                      <tr key={`${row.serverName}-${row.steamId}-${idx}`}>
                        <td style={{ fontWeight: 600 }}>{row.player}</td>
                        <td className="steam-id">{row.steamId}</td>
                        <td className="steam-id">{row.ipAddress}</td>
                        <td style={{ color: 'var(--text2)' }}>{row.serverName}</td>
                        <td><span className="status-pill pill-online">{row.syncedText}</span></td>
                      </tr>
                    ))
                  )}
                </tbody>
              </table>
            </div>
          )}
        </div>
      </div>

      {/* ── 全局配置 ── */}
      {canConfigure && (
        <div className="card" style={{ marginTop: 16 }}>
          <div className="card-header">
            <div>
              <div className="card-title">分发配置</div>
              <div className="card-sub">控制 Webhook 分发的全局参数和 Webhook 列表</div>
            </div>
            {hasUnsavedChanges && (
              <button className="btn btn-primary" onClick={saveConfig} disabled={saving}>
                {saving ? '保存中...' : '保存全部配置'}
              </button>
            )}
          </div>
          <div className="card-body">
            {/* 全局参数 */}
            <div style={{ display: 'flex', gap: 16, marginBottom: 20, flexWrap: 'wrap' }}>
              <div style={{ flex: '1 1 200px', maxWidth: 260 }}>
                <label style={{ display: 'block', fontSize: 12, fontWeight: 500, marginBottom: 6, color: 'var(--text2)' }}>最大 Webhook 数量</label>
                <input
                  type="number" min="0" className="form-control"
                  value={form.maxApiCount}
                  onChange={(e) => setForm((prev) => ({ ...prev, maxApiCount: e.target.value }))}
                />
              </div>
              <div style={{ flex: '1 1 200px', maxWidth: 260 }}>
                <label style={{ display: 'block', fontSize: 12, fontWeight: 500, marginBottom: 6, color: 'var(--text2)' }}>分发周期（秒）</label>
                <input
                  type="number" min="1" className="form-control"
                  value={form.intervalSeconds}
                  onChange={(e) => setForm((prev) => ({ ...prev, intervalSeconds: e.target.value }))}
                />
              </div>
              <div style={{ flex: '1 1 200px', maxWidth: 260 }}>
                <label style={{ display: 'block', fontSize: 12, fontWeight: 500, marginBottom: 6, color: 'var(--text2)' }}>当前 Webhook</label>
                <div style={{ padding: '9px 0', fontSize: 14, fontWeight: 600 }}>
                  {form.items.length} / {form.maxApiCount}
                </div>
              </div>
            </div>

            {/* Webhook 列表 */}
            <div style={{ borderTop: '1px solid var(--border)', paddingTop: 16 }}>
              <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 12 }}>
                <span style={{ fontSize: 13, fontWeight: 600 }}>Webhook 列表</span>
                <button className="action-btn" onClick={openCreateWebhook}>
                  + 添加
                </button>
              </div>

              {form.items.length === 0 ? (
                <div style={{
                  padding: 30, textAlign: 'center', color: 'var(--text3)', fontSize: 13,
                  background: 'var(--surface2)', borderRadius: 'var(--r-md)',
                }}>
                  暂无 Webhook 配置，点击上方"添加"按钮创建。
                </div>
              ) : (
                <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
                  {form.items.map((item, index) => {
                    const selectedServerNames = item.serverIds.length === 0
                      ? '全部服务器'
                      : item.serverIds.map((id) => serverOptions.find((s) => s.id === id)?.label ?? id.slice(0, 8)).join('、');
                    return (
                      <div key={`${item.publicPath}-${index}`} style={{
                        display: 'flex', alignItems: 'center', justifyContent: 'space-between',
                        padding: '12px 16px', border: '1px solid var(--border)', borderRadius: 'var(--r-md)',
                        background: 'var(--surface)', gap: 12,
                      }}>
                        <div style={{ flex: 1, minWidth: 0 }}>
                          <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 4 }}>
                            <span style={{ fontWeight: 600, fontSize: 13 }}>{item.publicPath}</span>
                            {statusPill(item.lastStatus)}
                          </div>
                          <div style={{ fontSize: 11.5, color: 'var(--text3)', display: 'flex', gap: 16, flexWrap: 'wrap' }}>
                            <span>地址：{window.location.origin}/webhook/{item.publicPath}</span>
                            <span>服务器：{selectedServerNames}</span>
                            {item.lastDispatchedAt && <span>上次分发：{item.lastDispatchedAt}</span>}
                          </div>
                          {item.lastError && (
                            <div style={{ fontSize: 11.5, color: 'var(--accent)', marginTop: 2 }}>错误：{item.lastError}</div>
                          )}
                        </div>
                        <div className="action-btn-group" style={{ flexShrink: 0 }}>
                          <button className="action-btn action-btn-accent" onClick={() => openEditWebhook(index)}>编辑</button>
                          <button className="action-btn action-btn-danger" onClick={() => removeWebhook(index)}>移除</button>
                        </div>
                      </div>
                    );
                  })}
                </div>
              )}
            </div>

            {hasUnsavedChanges && (
              <div style={{ marginTop: 16, display: 'flex', justifyContent: 'flex-end' }}>
                <button className="btn btn-primary" onClick={saveConfig} disabled={saving}>
                  {saving ? '保存中...' : '保存全部配置'}
                </button>
              </div>
            )}
          </div>
        </div>
      )}

      {/* ── 非管理员查看 Webhook 状态 ── */}
      {!canConfigure && form.items.length > 0 && (
        <div className="card" style={{ marginTop: 16 }}>
          <div className="card-header" style={{ borderBottom: 'none' }}>
            <div>
              <div className="card-title">Webhook 分发状态</div>
              <div className="card-sub">周期：{form.intervalSeconds} 秒</div>
            </div>
          </div>
          <div className="card-body" style={{ padding: 0 }}>
            <div className="table-responsive">
              <table className="data-table">
                <thead>
                  <tr><th>后缀</th><th>公开地址</th><th>服务器范围</th><th>状态</th><th>最近错误</th><th>最近分发</th></tr>
                </thead>
                <tbody>
                  {form.items.map((item, index) => (
                    <tr key={`${item.publicPath}-${index}`}>
                      <td className="steam-id">{item.publicPath}</td>
                      <td className="steam-id">{window.location.origin}/webhook/{item.publicPath}</td>
                      <td>{item.serverIds.length ? `${item.serverIds.length} 台服务器` : '全部'}</td>
                      <td>{statusPill(item.lastStatus)}</td>
                      <td style={{ color: item.lastError ? 'var(--accent)' : 'var(--text2)', maxWidth: 200, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{item.lastError ?? '无'}</td>
                      <td>{item.lastDispatchedAt ?? '未分发'}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </div>
        </div>
      )}

      {/* ── Webhook 编辑弹窗 ── */}
      {webhookModalOpen && (
        <div className="modal-overlay active" onClick={closeWebhookModal}>
          <div className="modal" style={{ maxWidth: 540 }} onClick={(e) => e.stopPropagation()}>
            <div className="modal-header">
              <h2 style={{ fontSize: 16, fontWeight: 600 }}>
                {editingIndex !== null ? '编辑 Webhook' : '添加 Webhook'}
              </h2>
              <span style={{ cursor: 'pointer', color: 'var(--text3)', fontSize: 18 }} onClick={closeWebhookModal}>✕</span>
            </div>
            <div className="modal-body">
              <div style={{
                marginBottom: 16, padding: '10px 14px', borderRadius: 'var(--r-sm)',
                background: 'var(--info-bg)', fontSize: 12.5, color: 'var(--info-text)', lineHeight: 1.6,
              }}>
                系统会按分发周期将在线玩家数据 POST 到指定的接收地址。配置完成后记得点击页面底部的"保存全部配置"生效。
              </div>

              <div className="form-group">
                <label>Webhook 接收地址</label>
                <input
                  type="url" className="form-control"
                  value={draftWebhook.webhookUrl}
                  onChange={(e) => setDraftWebhook((prev) => ({ ...prev, webhookUrl: e.target.value }))}
                  placeholder="https://example.com/api/players"
                />
                <div style={{ fontSize: 11, color: 'var(--text3)', marginTop: 4 }}>玩家数据将 POST 到此地址</div>
              </div>

              <div className="form-group">
                <label>自定义访问后缀</label>
                <input
                  type="text" className="form-control"
                  value={draftWebhook.publicPath}
                  onChange={(e) => setDraftWebhook((prev) => ({ ...prev, publicPath: e.target.value }))}
                  placeholder="例如: my-player-data"
                />
                {draftWebhook.publicPath.trim() && (
                  <div style={{ fontSize: 12, color: 'var(--text2)', marginTop: 4 }}>
                    公开访问地址：<strong>{window.location.origin}/webhook/{draftWebhook.publicPath.trim()}</strong>
                  </div>
                )}
              </div>

              <div className="form-group">
                <label>安全校验 Secret（可选）</label>
                <input
                  type="text" className="form-control"
                  value={draftWebhook.secret}
                  onChange={(e) => setDraftWebhook((prev) => ({ ...prev, secret: e.target.value }))}
                  placeholder="留空则不校验，请求头 X-Manger-Secret"
                />
              </div>

              {/* 服务器选择 - 按社区分组 */}
              <div className="form-group">
                <label>指定分发数据的服务器</label>
                <div style={{ fontSize: 11.5, color: 'var(--text3)', marginBottom: 8 }}>不选择则默认分发所有服务器的数据</div>
                <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
                  {Object.entries(groupedServers).map(([groupId, group]) => {
                    const groupServerIds = group.servers.map((s) => s.id);
                    const allSelected = groupServerIds.every((id) => draftWebhook.serverIds.includes(id));
                    const someSelected = groupServerIds.some((id) => draftWebhook.serverIds.includes(id));

                    return (
                      <div key={groupId} style={{
                        border: '1px solid var(--border)', borderRadius: 'var(--r-md)',
                        overflow: 'hidden',
                      }}>
                        <div style={{
                          display: 'flex', alignItems: 'center', justifyContent: 'space-between',
                          padding: '8px 12px', background: 'var(--surface2)',
                        }}>
                          <label style={{ display: 'flex', alignItems: 'center', gap: 8, cursor: 'pointer', marginBottom: 0, fontWeight: 500, fontSize: 13 }}>
                            <input
                              type="checkbox"
                              checked={allSelected}
                              ref={(el) => { if (el) el.indeterminate = someSelected && !allSelected; }}
                              onChange={() => toggleAllServers(groupId)}
                            />
                            {group.name}
                          </label>
                          <span style={{ fontSize: 11, color: 'var(--text3)' }}>
                            {group.servers.filter((s) => draftWebhook.serverIds.includes(s.id)).length}/{group.servers.length}
                          </span>
                        </div>
                        <div style={{ padding: '6px 12px 8px', display: 'flex', flexWrap: 'wrap', gap: '6px 16px' }}>
                          {group.servers.map((server) => (
                            <label key={server.id} style={{ display: 'flex', alignItems: 'center', gap: 6, cursor: 'pointer', fontSize: 12.5, fontWeight: 400 }}>
                              <input
                                type="checkbox"
                                checked={draftWebhook.serverIds.includes(server.id)}
                                onChange={() => toggleServer(server.id)}
                              />
                              {server.label}
                            </label>
                          ))}
                        </div>
                      </div>
                    );
                  })}
                </div>
              </div>

              {/* 外部服务器选择 */}
              {externalServerOptions.length > 0 && (
                <div className="form-group">
                  <label>外部服务器（RCON 查询）</label>
                  <div style={{ fontSize: 11.5, color: 'var(--text3)', marginBottom: 8 }}>不选择则不包含外部服务器数据</div>
                  <div style={{ border: '1px solid var(--border)', borderRadius: 'var(--r-md)', overflow: 'hidden' }}>
                    <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', padding: '8px 12px', background: 'var(--surface2)' }}>
                      <label style={{ display: 'flex', alignItems: 'center', gap: 8, cursor: 'pointer', marginBottom: 0, fontWeight: 500, fontSize: 13 }}>
                        <input type="checkbox" checked={externalServerOptions.length > 0 && externalServerOptions.every((s) => draftWebhook.externalServerIds.includes(s.id))} onChange={toggleAllExternalServers} />
                        全部外部服务器
                      </label>
                      <span style={{ fontSize: 11, color: 'var(--text3)' }}>{draftWebhook.externalServerIds.length}/{externalServerOptions.length}</span>
                    </div>
                    <div style={{ padding: '6px 12px 8px', display: 'flex', flexWrap: 'wrap', gap: '6px 16px' }}>
                      {externalServerOptions.map((server) => (
                        <label key={server.id} style={{ display: 'flex', alignItems: 'center', gap: 6, cursor: 'pointer', fontSize: 12.5, fontWeight: 400 }}>
                          <input type="checkbox" checked={draftWebhook.externalServerIds.includes(server.id)} onChange={() => toggleExternalServer(server.id)} />
                          {server.label} ({server.ip}:{server.port})
                        </label>
                      ))}
                    </div>
                  </div>
                </div>
              )}

              {message ? <div style={{ color: 'var(--accent)', marginTop: 8, fontSize: 13 }}>{message}</div> : null}
            </div>
            <div className="modal-footer">
              <button className="btn btn-outline" onClick={closeWebhookModal}>取消</button>
              <button className="btn btn-primary" onClick={saveWebhookToForm}>
                {editingIndex !== null ? '确认更新' : '确认添加'}
              </button>
            </div>
          </div>
        </div>
      )}

      {dialog}
      <ToastContainer toasts={toasts} onDismiss={dismissToast} />
    </div>
  );
}

import React, { useEffect, useRef, useState } from 'react';
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
  const canConfigure = session?.role === 'developer' || session?.role === 'admin';

  const [form, setForm] = useState(defaultPlayerApiConfig);
  const [message, setMessage] = useState('');
  const [refreshKey, setRefreshKey] = useState(0);

  const [modalOpen, setModalOpen] = useState(false);
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

  const groupedServers = serverOptions.reduce((acc, s) => {
    const key = s.groupId;
    if (!acc[key]) acc[key] = { name: s.groupName, servers: [] };
    acc[key].servers.push(s);
    return acc;
  }, {});

  // 拖拽排序
  const dragItem = useRef(null);
  const dragOverItem = useRef(null);

  // 合并已选服务器列表（保持 serverIds + externalServerIds 顺序）
  function getOrderedServers() {
    const list = [];
    for (const id of draftWebhook.serverIds) {
      const s = serverOptions.find((o) => o.id === id);
      if (s) list.push({ id: s.id, name: s.label, group: s.groupName, type: 'plugin' });
    }
    for (const id of draftWebhook.externalServerIds) {
      const s = externalServerOptions.find((o) => o.id === id);
      if (s) list.push({ id: s.id, name: s.label, group: '外部服务器', type: 'external' });
    }
    return list;
  }

  function handleDragStart(index) {
    dragItem.current = index;
  }

  function handleDragEnter(index) {
    dragOverItem.current = index;
  }

  function handleDragEnd() {
    const from = dragItem.current;
    const to = dragOverItem.current;
    if (from == null || to == null || from === to) return;

    const ordered = getOrderedServers();
    const [moved] = ordered.splice(from, 1);
    ordered.splice(to, 0, moved);

    const newServerIds = ordered.filter((s) => s.type === 'plugin').map((s) => s.id);
    const newExternalIds = ordered.filter((s) => s.type === 'external').map((s) => s.id);

    setDraftWebhook((prev) => ({ ...prev, serverIds: newServerIds, externalServerIds: newExternalIds }));
    dragItem.current = null;
    dragOverItem.current = null;
  }

  function openCreateWebhook() {
    if (form.items.length >= Number(form.maxApiCount)) {
      toast({ title: '无法添加', message: `已达到最大端点数量限制（${form.maxApiCount}）`, tone: 'danger' });
      return;
    }
    setDraftWebhook({ ...defaultWebhookConfig });
    setEditingIndex(null);
    setMessage('');
    setModalOpen(true);
  }

  function openEditWebhook(index) {
    setDraftWebhook({ ...form.items[index] });
    setEditingIndex(index);
    setMessage('');
    setModalOpen(true);
  }

  function closeWebhookModal() {
    setModalOpen(false);
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
    if (!draftWebhook.publicPath.trim()) {
      setMessage('请输入自定义访问后缀。');
      return;
    }
    if (/[^a-zA-Z0-9_-]/.test(draftWebhook.publicPath.trim())) {
      setMessage('后缀只能包含字母、数字、下划线和连字符。');
      return;
    }

    if (editingIndex !== null) {
      setForm((prev) => ({
        ...prev,
        items: prev.items.map((item, i) => (i === editingIndex ? { ...draftWebhook } : item)),
      }));
      toast({ title: '已更新', message: '端点已更新，请点击"保存全部配置"生效。' });
    } else {
      setForm((prev) => ({ ...prev, items: [...prev.items, { ...draftWebhook }] }));
      toast({ title: '已添加', message: '端点已添加，请点击"保存全部配置"生效。' });
    }
    closeWebhookModal();
  }

  async function removeWebhook(index) {
    const item = form.items[index];
    const confirmed = await confirm({
      title: '移除 API 端点',
      message: `确定要移除端点「${item.publicPath}」吗？`,
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
      toast({ title: '保存成功', message: 'API 端点配置已更新。' });
    } catch (e) {
      toast({ title: '保存失败', message: e.message, tone: 'danger' });
    } finally {
      setSaving(false);
    }
  }

  const hasUnsavedChanges = JSON.stringify(form) !== JSON.stringify(
    normalizePlayerApiConfig(configState.data?.config)
  );

  return (
    <div id="player-api" className="content-section active">
      <div className="breadcrumb">
        <span>系统功能</span><span className="sep">›</span><span className="current">玩家信息 API</span>
      </div>

      <div className="page-header">
        <div>
          <div className="page-title">实时玩家数据 API</div>
          <div className="page-sub">创建 API 端点，通过当前域名 + 自定义后缀即可访问指定服务器的在线玩家数据。</div>
        </div>
        {canConfigure && (
          <button className="btn btn-primary" onClick={openCreateWebhook}>
            添加端点
          </button>
        )}
      </div>

      {/* 在线玩家数据 */}
      <div className="card">
        <div className="card-header" style={{ borderBottom: 'none' }}>
          <div>
            <div className="card-title">在线玩家数据</div>
            <div className="card-sub">当前通过插件上报的实时在线玩家信息</div>
          </div>
        </div>
        <div className="card-body" className="p-0">
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
                        <td className="fw-600">{row.player}</td>
                        <td className="steam-id">{row.steamId}</td>
                        <td className="steam-id">{row.ipAddress}</td>
                        <td className="text-muted">{row.serverName}</td>
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

      {/* 分发配置 */}
      {canConfigure && (
        <div className="card" className="mt-16">
          <div className="card-header">
            <div>
              <div className="card-title">API 端点配置</div>
              <div className="card-sub">创建端点后，通过 {window.location.origin}/webhook/后缀 即可获取玩家数据</div>
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
                <label style={{ display: 'block', fontSize: 12, fontWeight: 500, marginBottom: 6, color: 'var(--text2)' }}>最大端点数量</label>
                <input
                  type="number" min="0" className="form-control"
                  value={form.maxApiCount}
                  onChange={(e) => setForm((prev) => ({ ...prev, maxApiCount: e.target.value }))}
                />
              </div>
              <div style={{ flex: '1 1 200px', maxWidth: 260 }}>
                <label style={{ display: 'block', fontSize: 12, fontWeight: 500, marginBottom: 6, color: 'var(--text2)' }}>分发周期（秒，仅推送到外部地址时生效）</label>
                <input
                  type="number" min="1" className="form-control"
                  value={form.intervalSeconds}
                  onChange={(e) => setForm((prev) => ({ ...prev, intervalSeconds: e.target.value }))}
                />
              </div>
              <div style={{ flex: '1 1 200px', maxWidth: 260 }}>
                <label style={{ display: 'block', fontSize: 12, fontWeight: 500, marginBottom: 6, color: 'var(--text2)' }}>当前端点</label>
                <div style={{ padding: '9px 0', fontSize: 14, fontWeight: 600 }}>
                  {form.items.length} / {form.maxApiCount}
                </div>
              </div>
            </div>

            {/* 端点列表 */}
            <div style={{ borderTop: '1px solid var(--border)', paddingTop: 16 }}>
              <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 12 }}>
                <span style={{ fontSize: 13, fontWeight: 600 }}>端点列表</span>
                <button className="action-btn" onClick={openCreateWebhook}>+ 添加</button>
              </div>

              {form.items.length === 0 ? (
                <div style={{
                  padding: 30, textAlign: 'center', color: 'var(--text3)', fontSize: 13,
                  background: 'var(--surface2)', borderRadius: 'var(--r-md)',
                }}>
                  暂无 API 端点，点击上方"添加"按钮创建。
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
                        opacity: item.enabled ? 1 : 0.6,
                      }}>
                        <div style={{ flex: 1, minWidth: 0 }}>
                          <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 4 }}>
                            <span style={{ fontWeight: 600, fontSize: 13 }}>{item.publicPath}</span>
                            {!item.enabled && <span style={{ fontSize: 11, color: 'var(--text3)', background: 'var(--surface2)', padding: '1px 6px', borderRadius: 3 }}>已禁用</span>}
                            {item.enabled && item.publicAccess && <span style={{ fontSize: 11, color: '#22c55e', background: 'rgba(34,197,94,0.1)', padding: '1px 6px', borderRadius: 3 }}>公开</span>}
                            {item.enabled && !item.publicAccess && item.secret && <span style={{ fontSize: 11, color: '#f59e0b', background: 'rgba(245,158,11,0.1)', padding: '1px 6px', borderRadius: 3 }}>密钥验证</span>}
                            {item.enabled && !item.publicAccess && !item.secret && <span style={{ fontSize: 11, color: 'var(--accent)', background: 'rgba(239,68,68,0.1)', padding: '1px 6px', borderRadius: 3 }}>未设置密钥</span>}
                          </div>
                          <div style={{ fontSize: 11.5, color: 'var(--text3)', display: 'flex', gap: 16, flexWrap: 'wrap' }}>
                            <span>{window.location.origin}/webhook/{item.publicPath}</span>
                            <span>服务器：{selectedServerNames}</span>
                          </div>
                        </div>
                        <div className="action-btn-group" className="flex-shrink-0">
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

      {/* 编辑弹窗 */}
      {modalOpen && (
        <div className="modal-overlay active" onClick={closeWebhookModal}>
          <div className="modal" style={{ maxWidth: 540 }} onClick={(e) => e.stopPropagation()}>
            <div className="modal-header">
              <h2 style={{ fontSize: 16, fontWeight: 600, margin: 0 }}>
                {editingIndex !== null ? '编辑 API 端点' : '添加 API 端点'}
              </h2>
              <span style={{ cursor: 'pointer', color: 'var(--text3)', fontSize: 18 }} onClick={closeWebhookModal}>&#10005;</span>
            </div>
            <div className="modal-body" style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
              <div style={{
                padding: '10px 14px', borderRadius: 'var(--r-sm)',
                background: 'var(--info-bg)', fontSize: 12.5, color: 'var(--info-text)', lineHeight: 1.6,
              }}>
                配置完成后，通过 <strong>{window.location.origin}/webhook/后缀</strong> 即可获取在线玩家数据。
                {draftWebhook.publicPath.trim() && (
                  <div className="mt-4">当前地址：<strong>{window.location.origin}/webhook/{draftWebhook.publicPath.trim()}</strong></div>
                )}
              </div>

              <div className="form-group">
                <label>自定义访问后缀 <span className="text-accent">*</span></label>
                <input
                  type="text" className="form-control"
                  value={draftWebhook.publicPath}
                  onChange={(e) => setDraftWebhook((prev) => ({ ...prev, publicPath: e.target.value.replace(/[^a-zA-Z0-9_-]/g, '') }))}
                  placeholder="例如: my-server-data"
                />
                <div style={{ fontSize: 12, color: 'var(--text3)', marginTop: 4 }}>仅允许字母、数字、下划线、连字符</div>
              </div>

              <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 12 }}>
                <div className="form-group" style={{ display: 'flex', alignItems: 'center', gap: 10, marginBottom: 0 }}>
                  <label className="toggle-switch">
                    <input type="checkbox" checked={draftWebhook.enabled} onChange={(e) => setDraftWebhook((prev) => ({ ...prev, enabled: e.target.checked }))} />
                    <span className="toggle-slider" />
                  </label>
                  <div>
                    <div style={{ fontSize: 13, fontWeight: 500 }}>启用端点</div>
                    <div style={{ fontSize: 11, color: 'var(--text3)' }}>关闭后无法访问</div>
                  </div>
                </div>
                <div className="form-group" style={{ display: 'flex', alignItems: 'center', gap: 10, marginBottom: 0 }}>
                  <label className="toggle-switch">
                    <input type="checkbox" checked={draftWebhook.publicAccess} onChange={(e) => setDraftWebhook((prev) => ({ ...prev, publicAccess: e.target.checked }))} />
                    <span className="toggle-slider" />
                  </label>
                  <div>
                    <div style={{ fontSize: 13, fontWeight: 500 }}>公开访问</div>
                    <div style={{ fontSize: 11, color: 'var(--text3)' }}>关闭后需密钥验证</div>
                  </div>
                </div>
              </div>

              {!draftWebhook.publicAccess && (
                <div className="form-group">
                  <label>访问密钥</label>
                  <input
                    type="text" className="form-control"
                    value={draftWebhook.secret}
                    onChange={(e) => setDraftWebhook((prev) => ({ ...prev, secret: e.target.value }))}
                    placeholder="设置访问密钥"
                  />
                  <div style={{ fontSize: 12, color: 'var(--text3)', marginTop: 4 }}>访问时需在请求头 <code>X-Manger-Secret</code> 中携带此密钥</div>
                </div>
              )}

              {/* 服务器选择 */}
              <div className="form-group">
                <label>指定分发数据的服务器</label>
                <div style={{ fontSize: 11.5, color: 'var(--text3)', marginBottom: 8 }}>不选择则默认包含所有服务器</div>
                <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
                  {Object.entries(groupedServers).map(([groupId, group]) => {
                    const groupServerIds = group.servers.map((s) => s.id);
                    const allSelected = groupServerIds.every((id) => draftWebhook.serverIds.includes(id));
                    const someSelected = groupServerIds.some((id) => draftWebhook.serverIds.includes(id));

                    return (
                      <div key={groupId} style={{ border: '1px solid var(--border)', borderRadius: 'var(--r-md)', overflow: 'hidden' }}>
                        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', padding: '8px 12px', background: 'var(--surface2)' }}>
                          <label style={{ display: 'flex', alignItems: 'center', gap: 8, cursor: 'pointer', marginBottom: 0, fontWeight: 500, fontSize: 13 }}>
                            <input type="checkbox" checked={allSelected} ref={(el) => { if (el) el.indeterminate = someSelected && !allSelected; }} onChange={() => toggleAllServers(groupId)} />
                            {group.name}
                          </label>
                          <span style={{ fontSize: 11, color: 'var(--text3)' }}>{group.servers.filter((s) => draftWebhook.serverIds.includes(s.id)).length}/{group.servers.length}</span>
                        </div>
                        <div style={{ padding: '6px 12px 8px', display: 'flex', flexWrap: 'wrap', gap: '6px 16px' }}>
                          {group.servers.map((server) => (
                            <label key={server.id} style={{ display: 'flex', alignItems: 'center', gap: 6, cursor: 'pointer', fontSize: 12.5, fontWeight: 400 }}>
                              <input type="checkbox" checked={draftWebhook.serverIds.includes(server.id)} onChange={() => toggleServer(server.id)} />
                              {server.label}
                            </label>
                          ))}
                        </div>
                      </div>
                    );
                  })}
                </div>
              </div>

              {/* 外部服务器 */}
              {externalServerOptions.length > 0 && (
                <div className="form-group">
                  <label>外部服务器</label>
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
                          {server.label}
                        </label>
                      ))}
                    </div>
                  </div>
                </div>
              )}

              {/* 拖拽排序 */}
              {getOrderedServers().length > 1 && (
                <div className="form-group">
                  <label>服务器排序</label>
                  <div style={{ fontSize: 11.5, color: 'var(--text3)', marginBottom: 8 }}>拖拽调整顺序，上方为 API 返回数据中的排列顺序</div>
                  <div style={{ display: 'flex', flexDirection: 'column', gap: 2 }}>
                    {getOrderedServers().map((server, index) => (
                      <div
                        key={`${server.type}-${server.id}`}
                        draggable
                        onDragStart={() => handleDragStart(index)}
                        onDragEnter={() => handleDragEnter(index)}
                        onDragEnd={handleDragEnd}
                        onDragOver={(e) => e.preventDefault()}
                        style={{
                          display: 'flex', alignItems: 'center', gap: 10,
                          padding: '8px 12px', background: 'var(--surface)',
                          border: '1px solid var(--border)', borderRadius: 4,
                          cursor: 'grab', fontSize: 13,
                          transition: 'background 0.15s',
                        }}
                      >
                        <span style={{ color: 'var(--text3)', cursor: 'grab', fontSize: 14, userSelect: 'none' }}>&#9776;</span>
                        <span className="fw-500">{server.name}</span>
                        <span style={{
                          fontSize: 11, color: 'var(--text3)', background: 'var(--surface2)',
                          padding: '1px 6px', borderRadius: 3,
                        }}>
                          {server.group}
                        </span>
                      </div>
                    ))}
                  </div>
                </div>
              )}

              {message ? <div style={{ color: 'var(--accent)', fontSize: 13 }}>{message}</div> : null}
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

import React, { useCallback, useEffect, useState } from 'react';
import { api } from '../../lib/api.js';
import { useAuth } from '../../state/auth.jsx';
import { useToast, ToastContainer } from '../../shared/Toast.jsx';
import { useConfirmDialog } from '../../shared/ConfirmModal.jsx';
import { Pagination } from '../../shared/Pagination.jsx';
import { SearchBar } from '../../shared/SearchBar.jsx';

const banTypeOptions = [
  { value: 'ban_evasion', label: 'Ban Evasion' },
  { value: 'bhop_hack', label: 'Bhop Hack' },
  { value: 'bhop_macro', label: 'Bhop Macro' },
  { value: 'exploiting', label: 'Exploiting' },
  { value: 'strafe_hack', label: 'Strafe Hack' },
  { value: 'strafe_macro', label: 'Strafe Macro' },
  { value: 'other', label: 'Other' },
];

const syncStatusMap = {
  synced: { label: '已同步', className: 'pill-online' },
  failed: { label: '失败', className: 'pill-offline' },
  unsynced: { label: '已撤销', className: 'pill-away' },
  pending: { label: '待同步', className: 'pill-idle' },
};

const defaultForm = {
  name: '',
  enabled: true,
  baseUrl: 'https://api.kzcharm.com',
  bearerToken: '',
  defaultBanType: 'other',
  autoSync: false,
  notesTemplate: '来源: LumiAdmin\n玩家: {player}\nSteamID64: {steam_id}\n原因: {reason}\n操作人: {operator}',
  statsTemplate: '',
};

function formFromTarget(target) {
  if (!target) return { ...defaultForm };
  return {
    name: target.name || '',
    enabled: Boolean(target.enabled),
    baseUrl: target.base_url || defaultForm.baseUrl,
    bearerToken: '',
    defaultBanType: target.default_ban_type || 'other',
    autoSync: Boolean(target.auto_sync),
    notesTemplate: target.notes_template || defaultForm.notesTemplate,
    statsTemplate: target.stats_template || '',
  };
}

function targetPayload(form) {
  return {
    name: form.name.trim(),
    enabled: form.enabled,
    base_url: form.baseUrl.trim(),
    bearer_token: form.bearerToken.trim() || null,
    default_ban_type: form.defaultBanType,
    auto_sync: form.autoSync,
    notes_template: form.notesTemplate,
    stats_template: form.statsTemplate.trim() || null,
  };
}

export function ExternalBanApiPage() {
  const { session } = useAuth();
  const { toast, toasts, dismiss } = useToast();
  const { confirm, dialog } = useConfirmDialog();
  const token = session?.token ?? null;

  // ---- Target management state ----
  const [targets, setTargets] = useState([]);
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState('');
  const [modalOpen, setModalOpen] = useState(false);
  const [editingTarget, setEditingTarget] = useState(null);
  const [form, setForm] = useState(defaultForm);
  const [saving, setSaving] = useState(false);
  const [testingId, setTestingId] = useState(null);

  // ---- Sync history state ----
  const [syncData, setSyncData] = useState(null);
  const [syncLoading, setSyncLoading] = useState(false);
  const [syncSearch, setSyncSearch] = useState('');
  const [syncStatus, setSyncStatus] = useState('');
  const [syncPage, setSyncPage] = useState(1);
  const [retrying, setRetrying] = useState(false);

  const loadTargets = useCallback(async () => {
    try {
      setLoading(true);
      setLoadError('');
      const result = await api.externalBanApiTargets(token);
      setTargets(result.items ?? []);
    } catch (error) {
      setLoadError(error.message || '加载外部封禁 API 失败');
    } finally {
      setLoading(false);
    }
  }, [token]);

  const loadSyncHistory = useCallback(async (page = syncPage, search = syncSearch, status = syncStatus) => {
    try {
      setSyncLoading(true);
      const params = { page, page_size: 20 };
      if (search) params.search = search;
      if (status) params.status = status;
      const result = await api.externalBanApiSyncs(token, params);
      setSyncData(result);
    } catch {
      setSyncData(null);
    } finally {
      setSyncLoading(false);
    }
  }, [token, syncPage, syncSearch, syncStatus]);

  useEffect(() => { React.startTransition(() => { loadTargets(); }); }, [loadTargets]);
  useEffect(() => { if (token) React.startTransition(() => { loadSyncHistory(); }); }, [token, loadSyncHistory, syncPage, syncSearch, syncStatus]);

  function openCreate() {
    setEditingTarget(null);
    setForm({ ...defaultForm, name: targets.length === 0 ? 'GOKZ.TOP' : '' });
    setModalOpen(true);
  }

  function openEdit(target) {
    setEditingTarget(target);
    setForm(formFromTarget(target));
    setModalOpen(true);
  }

  function closeModal() {
    setModalOpen(false);
    setEditingTarget(null);
    setForm(defaultForm);
  }

  async function handleSave() {
    if (!form.name.trim() || !form.baseUrl.trim() || !form.notesTemplate.trim()) {
      toast({ title: '保存失败', message: '请填写名称、API 地址和 Notes 模板', tone: 'danger' });
      return;
    }

    try {
      setSaving(true);
      const body = targetPayload(form);
      if (editingTarget) {
        await api.updateExternalBanApiTarget(token, editingTarget.id, body);
        toast({ title: '更新成功' });
      } else {
        await api.createExternalBanApiTarget(token, body);
        toast({ title: '添加成功' });
      }
      closeModal();
      loadTargets();
    } catch (error) {
      toast({ title: '保存失败', message: error.message, tone: 'danger' });
    } finally {
      setSaving(false);
    }
  }

  async function handleTest(target) {
    try {
      setTestingId(target.id);
      const result = await api.testExternalBanApiTarget(token, target.id);
      toast({
        title: result.result.ok ? '连接成功' : '连接失败',
        message: result.result.message,
        tone: result.result.ok ? 'success' : 'danger',
      });
    } catch (error) {
      toast({ title: '连接失败', message: error.message, tone: 'danger' });
    } finally {
      setTestingId(null);
    }
  }

  async function handleDelete(target) {
    const ok = await confirm({
      title: '删除外部 API',
      message: `确定删除「${target.name}」吗？同步记录也会一并删除。`,
      confirmText: '确认删除',
    });
    if (!ok) return;

    try {
      await api.deleteExternalBanApiTarget(token, target.id);
      toast({ title: '删除成功' });
      loadTargets();
    } catch (error) {
      toast({ title: '删除失败', message: error.message, tone: 'danger' });
    }
  }

  async function handleRetryFailed() {
    try {
      setRetrying(true);
      const result = await api.retryFailedExternalBanSyncs(token);
      toast({
        title: result.result.ok ? '重试完成' : '部分重试失败',
        message: result.result.message,
        tone: result.result.ok ? 'success' : 'warning',
      });
      loadSyncHistory();
    } catch (error) {
      toast({ title: '重试失败', message: error.message, tone: 'danger' });
    } finally {
      setRetrying(false);
    }
  }

  function handleSyncSearch(term) {
    setSyncSearch(term);
    setSyncPage(1);
  }

  function handleSyncStatusChange(e) {
    setSyncStatus(e.target.value);
    setSyncPage(1);
  }

  const syncItems = syncData?.items ?? [];
  const syncTotal = syncData?.total ?? 0;
  const syncPageSize = syncData?.page_size ?? 20;
  const _failedCount = syncItems.filter((x) => x.status === 'failed').length;

  return (
    <div id="external-ban-api" className="content-section active">
      <div className="breadcrumb">
        <span>系统功能</span><span className="sep">›</span><span>API 管理</span><span className="sep">›</span><span className="current">外部封禁 API</span>
      </div>

      <div className="page-header">
        <div>
          <div className="page-title">外部封禁 API</div>
          <div className="page-sub">配置多个 GOKZ.TOP Bans 兼容目标，将本地封禁同步到外部 API。</div>
        </div>
        <button className="btn btn-primary" onClick={openCreate}>添加外部 API</button>
      </div>

      {/* ---- Target management table ---- */}
      <div className="card">
        <div className="card-body p-0">
          <div className="table-responsive">
            <table className="data-table">
              <thead>
                <tr>
                  <th>名称</th>
                  <th>API 地址</th>
                  <th>封禁类型</th>
                  <th>自动同步</th>
                  <th>Token</th>
                  <th className="text-right">操作</th>
                </tr>
              </thead>
              <tbody>
                {loading ? <tr><td colSpan={6} style={{ textAlign: 'center', color: 'var(--text2)' }}>正在加载外部 API...</td></tr> : null}
                {!loading && loadError ? <tr><td colSpan={6} style={{ textAlign: 'center', color: 'var(--danger)' }}>{loadError}</td></tr> : null}
                {!loading && !loadError && targets.length === 0 ? (
                  <tr><td colSpan={6} style={{ textAlign: 'center', color: 'var(--text2)' }}>暂无外部 API，点击右上角添加。</td></tr>
                ) : null}
                {targets.map((target) => (
                  <tr key={target.id}>
                    <td>
                      <div className="fw-600">{target.name}</div>
                      {!target.enabled ? <div style={{ color: 'var(--text3)', fontSize: 12 }}>已禁用</div> : null}
                    </td>
                    <td className="steam-id">{target.base_url}</td>
                    <td>{banTypeOptions.find((item) => item.value === target.default_ban_type)?.label ?? target.default_ban_type}</td>
                    <td>{target.auto_sync ? <span className="status-pill pill-online">开启</span> : <span className="text-muted-light">关闭</span>}</td>
                    <td>{target.has_token ? <span className="status-pill pill-online">已保存</span> : <span style={{ color: 'var(--danger)' }}>未配置</span>}</td>
                    <td className="text-right">
                      <div className="action-btn-group">
                        <button className="action-btn" onClick={() => handleTest(target)} disabled={testingId === target.id}>
                          {testingId === target.id ? '测试中' : '测试'}
                        </button>
                        <button className="action-btn action-btn-accent" onClick={() => openEdit(target)}>编辑</button>
                        <button className="action-btn action-btn-danger" onClick={() => handleDelete(target)}>删除</button>
                      </div>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      </div>

      {/* ---- Sync history section ---- */}
      <div style={{ marginTop: 32 }}>
        <div className="page-header" style={{ marginBottom: 16 }}>
          <div>
            <div className="page-title" style={{ fontSize: 18 }}>同步历史</div>
            <div className="page-sub">查看封禁记录与外部 API 的同步状态。</div>
          </div>
          {syncTotal > 0 && (
            <button className="btn btn-outline" onClick={handleRetryFailed} disabled={retrying}>
              {retrying ? '重试中...' : '重试失败同步'}
            </button>
          )}
        </div>

        <div className="filter-bar">
          <SearchBar value={syncSearch} onChange={handleSyncSearch} placeholder="搜索 SteamID / 玩家名" />
          <select className="form-control" style={{ width: 140 }} value={syncStatus} onChange={handleSyncStatusChange}>
            <option value="">全部状态</option>
            <option value="synced">已同步</option>
            <option value="failed">失败</option>
            <option value="unsynced">已撤销</option>
            <option value="pending">待同步</option>
          </select>
        </div>

        <div className="card">
          <div className="card-body p-0">
            <div className="table-responsive">
              <table className="data-table">
                <thead>
                  <tr>
                    <th>玩家</th>
                    <th>SteamID</th>
                    <th>目标</th>
                    <th>状态</th>
                    <th>外部 UUID</th>
                    <th>错误信息</th>
                    <th>同步时间</th>
                  </tr>
                </thead>
                <tbody>
                  {syncLoading ? <tr><td colSpan={7} style={{ textAlign: 'center', color: 'var(--text2)' }}>正在加载...</td></tr> : null}
                  {!syncLoading && !syncData ? <tr><td colSpan={7} style={{ textAlign: 'center', color: 'var(--text2)' }}>暂无同步记录。</td></tr> : null}
                  {!syncLoading && syncData && syncItems.length === 0 ? (
                    <tr><td colSpan={7} style={{ textAlign: 'center', color: 'var(--text2)' }}>暂无匹配的同步记录。</td></tr>
                  ) : null}
                  {syncItems.map((item) => {
                    const statusInfo = syncStatusMap[item.status] || { label: item.status, className: 'pill-idle' };
                    return (
                      <tr key={`${item.local_ban_id}-${item.target_id}`}>
                        <td>{item.ban_player || '-'}</td>
                        <td className="steam-id">{item.ban_steam_id || '-'}</td>
                        <td>{item.target_name}</td>
                        <td><span className={`status-pill ${statusInfo.className}`}>{statusInfo.label}</span></td>
                        <td className="steam-id">{item.external_uuid || '-'}</td>
                        <td style={{ maxWidth: 200, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }} title={item.last_error || ''}>
                          {item.last_error ? <span style={{ color: 'var(--danger)' }}>{item.last_error}</span> : '-'}
                        </td>
                        <td style={{ whiteSpace: 'nowrap' }}>{item.synced_at || item.updated_at}</td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            </div>
          </div>
          {syncTotal > syncPageSize ? (
            <Pagination page={syncPage} total={syncTotal} pageSize={syncPageSize} onChange={setSyncPage} />
          ) : null}
        </div>
      </div>

      {modalOpen ? (
        <div className="modal-overlay active" onClick={closeModal}>
          <div className="modal" style={{ maxWidth: 720 }} onClick={(e) => e.stopPropagation()}>
            <div className="modal-header">
              <h2 style={{ fontSize: 16, fontWeight: 600, margin: 0 }}>{editingTarget ? '编辑外部 API' : '添加外部 API'}</h2>
              <span style={{ cursor: 'pointer', color: 'var(--text3)', fontSize: 18 }} onClick={closeModal}>&#10005;</span>
            </div>
            <div className="modal-body" style={{ display: 'grid', gap: 16 }}>
              <div className="form-group">
                <label>名称</label>
                <input className="form-control" value={form.name} onChange={(e) => setForm((prev) => ({ ...prev, name: e.target.value }))} placeholder="GOKZ.TOP" />
              </div>

              <div style={{ display: 'grid', gridTemplateColumns: 'minmax(0, 1fr) 220px', gap: 12 }}>
                <div className="form-group">
                  <label>API 地址</label>
                  <input className="form-control" value={form.baseUrl} onChange={(e) => setForm((prev) => ({ ...prev, baseUrl: e.target.value }))} placeholder="https://api.kzcharm.com" />
                </div>
                <div className="form-group">
                  <label>封禁类型</label>
                  <select className="form-control" value={form.defaultBanType} onChange={(e) => setForm((prev) => ({ ...prev, defaultBanType: e.target.value }))}>
                    {banTypeOptions.map((option) => <option key={option.value} value={option.value}>{option.label}</option>)}
                  </select>
                </div>
              </div>

              <div className="form-group">
                <label>Bearer Token</label>
                <input
                  type="password"
                  className="form-control"
                  value={form.bearerToken}
                  onChange={(e) => setForm((prev) => ({ ...prev, bearerToken: e.target.value }))}
                  placeholder={editingTarget?.has_token ? '已保存 Token，留空则保持不变' : '填写外部 API Bearer Token'}
                />
              </div>

              <div style={{ display: 'flex', gap: 24, flexWrap: 'wrap' }}>
                <label style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
                  <span className="toggle-switch">
                    <input type="checkbox" checked={form.enabled} onChange={(e) => setForm((prev) => ({ ...prev, enabled: e.target.checked }))} />
                    <span className="toggle-slider" />
                  </span>
                  <span style={{ color: 'var(--text2)', fontSize: 14 }}>启用该 API</span>
                </label>
                <label style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
                  <span className="toggle-switch">
                    <input type="checkbox" checked={form.autoSync} onChange={(e) => setForm((prev) => ({ ...prev, autoSync: e.target.checked }))} />
                    <span className="toggle-slider" />
                  </span>
                  <span style={{ color: 'var(--text2)', fontSize: 14 }}>新增封禁时自动同步</span>
                </label>
              </div>

              <div className="form-group">
                <label>Notes 模板</label>
                <textarea className="form-control" rows={6} value={form.notesTemplate} onChange={(e) => setForm((prev) => ({ ...prev, notesTemplate: e.target.value }))} />
                <div style={{ fontSize: 12, color: 'var(--text3)', marginTop: 4 }}>
                  可用变量：{'{player}'} {'{steam_id}'} {'{ip_address}'} {'{reason}'} {'{operator}'} {'{source}'} {'{server_name}'}
                </div>
              </div>

              <div className="form-group">
                <label>Stats 模板</label>
                <textarea className="form-control" rows={3} value={form.statsTemplate} onChange={(e) => setForm((prev) => ({ ...prev, statsTemplate: e.target.value }))} placeholder="可选，留空则不发送 stats" />
              </div>
            </div>
            <div className="modal-footer">
              <button className="btn btn-outline" onClick={closeModal}>取消</button>
              <button className="btn btn-primary" onClick={handleSave} disabled={saving}>
                {saving ? '保存中...' : editingTarget ? '确认更新' : '确认添加'}
              </button>
            </div>
          </div>
        </div>
      ) : null}

      {dialog}
      <ToastContainer toasts={toasts} onDismiss={dismiss} />
    </div>
  );
}

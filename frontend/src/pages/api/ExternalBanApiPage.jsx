import React, { useEffect, useState } from 'react';
import { api } from '../../lib/api.js';
import { useAuth } from '../../state/auth.jsx';
import { useToast, ToastContainer } from '../../shared/Toast.jsx';
import { useConfirmDialog } from '../../shared/ConfirmModal.jsx';

const banTypeOptions = [
  { value: 'ban_evasion', label: 'Ban Evasion' },
  { value: 'bhop_hack', label: 'Bhop Hack' },
  { value: 'bhop_macro', label: 'Bhop Macro' },
  { value: 'exploiting', label: 'Exploiting' },
  { value: 'strafe_hack', label: 'Strafe Hack' },
  { value: 'strafe_macro', label: 'Strafe Macro' },
  { value: 'other', label: 'Other' },
];

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

  const [targets, setTargets] = useState([]);
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState('');
  const [modalOpen, setModalOpen] = useState(false);
  const [editingTarget, setEditingTarget] = useState(null);
  const [form, setForm] = useState(defaultForm);
  const [saving, setSaving] = useState(false);
  const [testingId, setTestingId] = useState(null);

  async function loadTargets() {
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
  }

  useEffect(() => { loadTargets(); }, [token]);

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

      <div className="card">
        <div className="card-body" className="p-0">
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

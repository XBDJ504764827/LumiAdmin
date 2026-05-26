import React, { useCallback, useState } from 'react';
import { api } from '../../lib/api.js';
import { useConfirmDialog } from '../../shared/ConfirmModal.jsx';
import { Modal } from '../../shared/Modal.jsx';
import { useAuth } from '../../state/auth.jsx';
import { useToast, ToastContainer } from '../../shared/Toast.jsx';
import { BAN_TYPE_OPTIONS, banModalSubmitText, banModalTitle, banRecordAction, buildBanFormFromRecord, buildCreateBanPayload, emptyBanForm, validateBanForm } from './banForm.js';
import { formatBanDuration, formatBanSource, formatExpiresAt } from './banDisplay.js';
import { SearchBar } from '../../shared/SearchBar.jsx';
import { Pagination } from '../../shared/Pagination.jsx';

function formatDateTime(isoString) {
  if (!isoString) return '-';
  try {
    const date = new Date(isoString);
    const y = date.getFullYear();
    const m = String(date.getMonth() + 1).padStart(2, '0');
    const d = String(date.getDate()).padStart(2, '0');
    const h = String(date.getHours()).padStart(2, '0');
    const min = String(date.getMinutes()).padStart(2, '0');
    const s = String(date.getSeconds()).padStart(2, '0');
    return `${y}-${m}-${d} ${h}:${min}:${s}`;
  } catch {
    return isoString;
  }
}

export function BanPage() {
  const { session } = useAuth();
  const { confirm, dialog } = useConfirmDialog();
  const { toast, toasts, dismiss: dismissToast } = useToast();
  const token = session?.token ?? null;
  const [refreshKey, setRefreshKey] = useState(0);
  const [search, setSearch] = useState('');
  const [status, setStatus] = useState(undefined);
  const [page, setPage] = useState(1);
  const [data, setData] = useState(null);
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState('');
  const [open, setOpen] = useState(false);
  const [modalMode, setModalMode] = useState('create');
  const [editingBanId, setEditingBanId] = useState(null);
  const [form, setForm] = useState(emptyBanForm);
  const [error, setError] = useState('');
  const [submitting, setSubmitting] = useState(false);

  const canManageAll = session?.role === 'developer' || session?.role === 'admin';
  const canCreate = canManageAll;

  const loadItems = useCallback(async () => {
    try {
      setLoading(true);
      setLoadError('');
      const params = { page, page_size: 20 };
      if (search) params.search = search;
      if (status) params.status = status;
      const result = await api.bans(token, params);
      setData(result);
    } catch {
      setData(null);
      setLoadError('加载封禁数据失败，请稍后重试');
    } finally {
      setLoading(false);
    }
  }, [token, page, search, status]);

  React.useEffect(() => { loadItems(); }, [loadItems]);

  function refresh() {
    setRefreshKey((v) => v + 1);
    loadItems();
  }

  function openCreateModal() {
    setModalMode('create');
    setEditingBanId(null);
    setForm(emptyBanForm);
    setError('');
    setOpen(true);
  }

  function openEditModal(item) {
    setModalMode('edit');
    setEditingBanId(item.id);
    setForm(buildBanFormFromRecord(item));
    setError('');
    setOpen(true);
  }

  function openRebanModal(item) {
    setModalMode('reban');
    setEditingBanId(null);
    setForm(buildBanFormFromRecord(item));
    setError('');
    setOpen(true);
  }

  function canUnban(item) {
    return canManageAll || (session?.role === 'normal' && item.operator_name === session?.displayName);
  }

  async function handleUnban(item) {
    try {
      await api.unban(token, item.id);
      refresh();
      toast({ title: '解封成功', message: `${item.player || item.steam_id} 已解封。` });
    } catch (requestError) {
      toast({ title: '解封失败', message: requestError.message, tone: 'danger' });
    }
  }

  async function handleDeleteBan(item) {
    const confirmed = await confirm({
      title: '删除封禁记录',
      message: `确定删除 ${item.player || item.steam_id} 的封禁记录吗？`,
    });
    if (!confirmed) return;

    try {
      await api.deleteBan(token, item.id);
      refresh();
      toast({ title: '删除成功', message: `已删除 ${item.player || item.steam_id} 的封禁记录。` });
    } catch (requestError) {
      toast({ title: '删除失败', message: requestError.message, tone: 'danger' });
    }
  }

  async function handleSaveBan() {
    const validationError = validateBanForm(form);
    if (validationError) {
      setError(validationError);
      return;
    }

    try {
      setSubmitting(true);
      setError('');
      const payload = buildCreateBanPayload(form);
      if (modalMode === 'edit' && editingBanId) {
        await api.updateBan(token, editingBanId, payload);
      } else {
        await api.createBan(token, payload);
      }
      setOpen(false);
      setModalMode('create');
      setEditingBanId(null);
      setForm(emptyBanForm);
      refresh();
      toast({ title: modalMode === 'edit' ? '保存成功' : '添加成功', message: modalMode === 'edit' ? '封禁记录已更新。' : '新封禁记录已添加。' });
    } catch (requestError) {
      setError(requestError.message);
    } finally {
      setSubmitting(false);
    }
  }

  const items = data?.items ?? [];
  const total = data?.total ?? 0;

  return (
    <div id="ban" className="content-section active">
      <div className="breadcrumb"><span>核心管理</span><span className="sep">›</span><span className="current">封禁管理</span></div>
      <div className="page-header">
        <div>
          <div className="page-title">玩家封禁管理</div>
          <div className="page-sub">针对违规玩家进行账号或 IP 级别的封禁限制。</div>
        </div>
        {canCreate ? <button className="btn btn-danger" onClick={openCreateModal}>手动添加封禁</button> : null}
      </div>

      <SearchBar
        value={search}
        onChange={(v) => { setSearch(v); setPage(1); }}
        placeholder="搜索 SteamID / 玩家名称..."
        statusOptions={[{ value: 'active', label: '生效中' }, { value: 'inactive', label: '已失效' }]}
        statusValue={status}
        onStatusChange={(v) => { setStatus(v); setPage(1); }}
      />

      <div className="card">
        <div className="card-body" style={{ padding: 0 }}>
          <div className="table-responsive">
            <table className="data-table">
              <thead>
                <tr><th>玩家名称</th><th>SteamID64</th><th>封禁属性</th><th>IP 地址</th><th>封禁理由</th><th>时长</th><th>到期时间</th><th>来源</th><th>封禁状态</th><th>封禁时间</th><th>解封时间</th><th>操作人</th><th style={{ textAlign: 'right' }}>操作</th></tr>
              </thead>
              <tbody>
                {loading ? <tr><td colSpan={13} style={{ textAlign: 'center', color: 'var(--text2)' }}>正在加载封禁数据...</td></tr> : null}
                {!loading && loadError ? <tr><td colSpan={13} style={{ textAlign: 'center', color: 'var(--danger)' }}>{loadError}</td></tr> : null}
                {!loading && !loadError && items.length === 0 ? <tr><td colSpan={13} style={{ textAlign: 'center', color: 'var(--text2)' }}>暂无封禁数据</td></tr> : null}
                {!loading && items.map((x) => (
                  <tr key={x.id}>
                    <td style={{ fontWeight: 600 }}>{x.player || '待自动获取'}</td>
                    <td className="steam-id">{x.steam_id}</td>
                    <td>{x.ban_type === 'ip' ? 'IP 封禁' : 'Steam 账号封禁'}</td>
                    <td className="steam-id">{x.ip_address || '待自动获取'}</td>
                    <td style={{ color: 'var(--text2)' }}>{x.reason}</td>
                    <td>{formatBanDuration(x.duration_minutes)}</td>
                    <td>{formatExpiresAt(x.expires_at)}</td>
                    <td>{formatBanSource(x.source, x.operator_name)}</td>
                    <td><span className={`status-pill ${x.status === 'active' ? 'pill-danger' : 'pill-offline'}`}>{x.status === 'active' ? '生效中' : '已失效'}</span></td>
                    <td style={{ color: 'var(--text3)' }}>{formatDateTime(x.created_at)}</td>
                    <td style={{ color: 'var(--text3)' }}>{x.removed_at ? formatDateTime(x.removed_at) : '-'}</td>
                    <td>{x.operator_name}</td>
                    <td style={{ textAlign: 'right' }}>
                      <div className="action-btn-group">
                        {canManageAll ? <button className="action-btn action-btn-accent" onClick={() => openEditModal(x)}>编辑</button> : null}
                        {canUnban(x) && banRecordAction(x) === 'unban' ? <button className="action-btn action-btn-success" onClick={() => handleUnban(x)}>解封</button> : null}
                        {canCreate && banRecordAction(x) === 'reban' ? <button className="action-btn action-btn-danger" onClick={() => openRebanModal(x)}>重新封禁</button> : null}
                        {canManageAll ? <button className="action-btn action-btn-danger" onClick={() => handleDeleteBan(x)}>删除</button> : null}
                      </div>
                    </td>
                  </tr>
                ))}
                {!loading && items.length === 0 ? <tr><td colSpan={13} style={{ textAlign: 'center', color: 'var(--text2)' }}>暂无封禁记录</td></tr> : null}
              </tbody>
            </table>
          </div>
        </div>
      </div>

      <Pagination page={page} pageSize={20} total={total} onChange={setPage} />

      <Modal
        open={open}
        title={banModalTitle(modalMode)}
        onClose={() => {
          setOpen(false);
          setModalMode('create');
          setEditingBanId(null);
          setForm(emptyBanForm);
          setError('');
        }}
        footer={<><button className="btn btn-outline" onClick={() => { setOpen(false); setModalMode('create'); setEditingBanId(null); setForm(emptyBanForm); setError(''); }}>取消</button><button className="btn btn-danger" onClick={handleSaveBan} disabled={submitting}>{banModalSubmitText(modalMode, submitting)}</button></>}
      >
        <div className="form-group"><label>玩家名称</label><input type="text" className="form-control" value={form.player} onChange={(event) => setForm((prev) => ({ ...prev, player: event.target.value }))} placeholder="留空后等待插件自动获取" /></div>
        <div className="form-group"><label>SteamID64 <span style={{ color: 'var(--accent)' }}>*</span></label><input type="text" className="form-control" value={form.steam_id} onChange={(event) => setForm((prev) => ({ ...prev, steam_id: event.target.value }))} placeholder="76561198000000000" /></div>
        <div className="form-group"><label>封禁属性 <span style={{ color: 'var(--accent)' }}>*</span></label><select className="form-control" value={form.ban_type} onChange={(event) => setForm((prev) => ({ ...prev, ban_type: event.target.value }))}><option value="">请选择封禁属性</option>{BAN_TYPE_OPTIONS.map((option) => <option key={option.value} value={option.value}>{option.label}</option>)}</select></div>
        <div className="form-group"><label>IP 地址</label><input type="text" className="form-control" value={form.ip_address} onChange={(event) => setForm((prev) => ({ ...prev, ip_address: event.target.value }))} placeholder="留空后等待插件自动获取" /></div>
        <div className="form-group"><label>封禁理由 <span style={{ color: 'var(--accent)' }}>*</span></label><textarea className="form-control" value={form.reason} onChange={(event) => setForm((prev) => ({ ...prev, reason: event.target.value }))} placeholder="请输入封禁理由" rows={3} /></div>
        {form.ban_type === 'steam' ? <div style={{ color: 'var(--text2)', fontSize: 13 }}>账号封禁：该 SteamID64 无法进入游戏服务器。</div> : null}
        {form.ban_type === 'ip' ? <div style={{ color: 'var(--text2)', fontSize: 13 }}>IP 封禁：该玩家下次进服后将自动填写 IP 并且阻止该 IP 的玩家进入服务器。</div> : null}
        {error ? <div style={{ color: 'var(--accent)' }}>{error}</div> : null}
      </Modal>
      {dialog}
      <ToastContainer toasts={toasts} onDismiss={dismissToast} />
    </div>
  );
}

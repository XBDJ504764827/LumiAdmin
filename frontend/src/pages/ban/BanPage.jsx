import React, { useCallback, useState } from 'react';
import { api } from '../../lib/api.js';
import { useApiQuery } from '../../shared/useApiQuery.js';
import { useConfirmDialog } from '../../shared/ConfirmModal.jsx';
import { InternalNoteInline } from '../../shared/InternalNote.jsx';
import { useAuth } from '../../state/auth.jsx';
import { useToast, ToastContainer } from '../../shared/Toast.jsx';
import { buildBanFormFromRecord } from './banForm.js';
import { formatBanDuration, formatBanSource, formatExpiresAt } from './banDisplay.js';
import { SearchBar } from '../../shared/SearchBar.jsx';
import { Pagination } from '../../shared/Pagination.jsx';
import { useLocation, useNavigate } from 'react-router-dom';
import { formatChinaDateTime } from '../../shared/time.js';
import { notifyPendingReviewsUpdated } from '../../hooks/usePendingReviewIndicators.js';
import { BanApiModal } from './BanApiModal.jsx';
import { BanDetailModal } from './BanDetailModal.jsx';
import { BanFormModal } from './BanFormModal.jsx';

function buildBanFormFromPlayerReport(report) {
  return {
    player: report.player ?? '',
    steam_id: report.steamId ?? '',
    ban_type: 'steam',
    ip_address: '',
    reason: report.reason ? `玩家举报：${report.reason}` : '',
  };
}

export function BanPage() {
  const { session } = useAuth();
  const { confirm, dialog } = useConfirmDialog();
  const { toast, toasts, dismiss: dismissToast } = useToast();
  const location = useLocation();
  const navigate = useNavigate();
  const token = session?.token ?? null;

  // 列表状态
  const [search, setSearch] = useState('');
  const [status, setStatus] = useState(undefined);
  const [page, setPage] = useState(1);

  // 使用 React Query 获取封禁列表
  const { data, isLoading, error: loadError, refetch } = useApiQuery(
    ['bans', { page, search, status }],
    (token) => api.bans(token, { page, page_size: 20, ...(search ? { search } : {}), ...(status ? { status } : {}) }),
  );

  // 表单弹窗状态
  const [formOpen, setFormOpen] = useState(false);
  const [formMode, setFormMode] = useState('create');
  const [editingBanId, setEditingBanId] = useState(null);
  const [prefillForm, setPrefillForm] = useState(null);
  const [reportReview, setReportReview] = useState(null);

  // 详情弹窗状态
  const [detailItem, setDetailItem] = useState(null);

  // API 弹窗状态
  const [apiModalOpen, setApiModalOpen] = useState(false);

  // 同步外部状态
  const [syncingExternalId, setSyncingExternalId] = useState(null);

  const canManageAll = session?.role === 'developer' || session?.role === 'admin';
  const canCreate = canManageAll;

  const items = data?.items ?? [];
  const total = data?.total ?? 0;

  // 从举报页跳转预填
  React.useEffect(() => {
    const prefill = location.state?.playerReportPrefill;
    if (!prefill || !canCreate) return;
    setFormMode('create');
    setEditingBanId(null);
    setPrefillForm(buildBanFormFromPlayerReport(prefill));
    setReportReview({
      reportId: prefill.reportId,
      player: prefill.player || prefill.steamId,
    });
    setFormOpen(true);
    navigate(location.pathname, { replace: true, state: null });
  }, [location.state, location.pathname, navigate, canCreate]);

  function openCreateModal() {
    setFormMode('create');
    setEditingBanId(null);
    setPrefillForm(null);
    setReportReview(null);
    setFormOpen(true);
  }

  async function openEditModal(item) {
    setFormMode('edit');
    setEditingBanId(item.id);
    setReportReview(null);
    try {
      const result = await api.getBan(token, item.id);
      setPrefillForm(buildBanFormFromRecord(result.item ?? item));
      setFormOpen(true);
    } catch (requestError) {
      toast({ title: '加载失败', message: requestError.message || '加载封禁详情失败。', tone: 'danger' });
    }
  }

  function openRebanModal(item) {
    setFormMode('reban');
    setEditingBanId(null);
    setPrefillForm(buildBanFormFromRecord(item));
    setReportReview(null);
    setFormOpen(true);
  }

  function handleFormSuccess({ mode, uploadedFiles, uploadWarning, reportReview: rr }) {
    refetch();
    if (rr?.reportId) {
      notifyPendingReviewsUpdated({ source: 'playerReport', action: 'ban' });
    }
    toast({
      title: mode === 'edit' ? '保存成功' : '添加成功',
      message: mode === 'edit'
        ? '封禁记录已更新。'
        : uploadedFiles
          ? '新封禁记录已添加，辅助文件已上传。'
          : uploadWarning
            ? `新封禁记录已添加，但辅助文件上传失败：${uploadWarning}`
            : rr?.reportId
              ? '新封禁记录已添加，玩家举报已标记为已封禁。'
              : '新封禁记录已添加。',
    });
  }

  function canUnban(item) {
    return canManageAll || (session?.role === 'normal' && item.operator_name === session?.displayName);
  }

  async function handleUnban(item) {
    try {
      await api.unban(token, item.id);
      refetch();
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
      refetch();
      toast({ title: '删除成功', message: `已删除 ${item.player || item.steam_id} 的封禁记录。` });
    } catch (requestError) {
      toast({ title: '删除失败', message: requestError.message, tone: 'danger' });
    }
  }

  async function handleSyncExternalBan(item) {
    const ok = await confirm({
      title: '同步到外部封禁 API',
      message: `确定将 ${item.player || item.steam_id} 的封禁记录同步到所有已启用的外部 API 吗？`,
      confirmText: '确认同步',
    });
    if (!ok) return;
    try {
      setSyncingExternalId(item.id);
      const result = await api.syncExternalBan(token, item.id);
      toast({
        title: '同步成功',
        message: result.result?.message || `${item.player || item.steam_id} 已同步到外部封禁 API。`,
      });
    } catch (requestError) {
      toast({ title: '同步失败', message: requestError.message, tone: 'danger' });
    } finally {
      setSyncingExternalId(null);
    }
  }

  return (
    <div id="ban" className="content-section active">
      <div className="breadcrumb"><span>核心管理</span><span className="sep">›</span><span className="current">封禁管理</span></div>
      <div className="page-header">
        <div>
          <div className="page-title">玩家封禁管理</div>
          <div className="page-sub">针对违规玩家进行账号或 IP 级别的封禁限制。</div>
        </div>
        <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap' }}>
          {canManageAll ? <button className="btn btn-outline" onClick={() => setApiModalOpen(true)}>API 接入</button> : null}
          {canCreate ? <button className="btn btn-danger" onClick={openCreateModal}>手动添加封禁</button> : null}
        </div>
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
        <div className="card-body p-0">
          <div className="table-responsive">
            <table className="data-table">
              <thead>
                <tr><th>玩家</th><th>SteamID64</th><th>封禁属性</th><th>封禁理由</th><th>时长 / 到期</th><th>状态</th><th>封禁时间</th><th className="text-right">操作</th></tr>
              </thead>
              <tbody>
                {isLoading ? <tr><td colSpan={8} style={{ textAlign: 'center', color: 'var(--text2)' }}>正在加载封禁数据...</td></tr> : null}
                {!isLoading && loadError ? <tr><td colSpan={8} style={{ textAlign: 'center', color: 'var(--danger)' }}>{loadError.message}</td></tr> : null}
                {!isLoading && items.map((x) => (
                  <tr key={x.id}>
                    <td>
                      <div className="fw-600" style={{ maxWidth: 160, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }} title={x.player || '待自动获取'}>{x.player || '待自动获取'}</div>
                      <InternalNoteInline steamid64={x.steam_id} />
                    </td>
                    <td className="steam-id">{x.steam_id}</td>
                    <td>{x.ban_type === 'ip' ? 'IP 封禁' : 'Steam 账号封禁'}</td>
                    <td style={{ color: 'var(--text2)', maxWidth: 260, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }} title={x.reason}>{x.reason}</td>
                    <td>
                      <div>{formatBanDuration(x.duration_minutes)}</div>
                      <div style={{ color: 'var(--text3)', fontSize: 12 }}>{formatExpiresAt(x.expires_at)}</div>
                    </td>
                    <td><span className={`status-pill ${x.status === 'active' ? 'pill-danger' : 'pill-offline'}`}>{x.status === 'active' ? '生效中' : '已失效'}</span></td>
                    <td className="text-muted-light">{formatChinaDateTime(x.created_at)}</td>
                    <td className="text-right">
                      <div className="action-btn-group">
                        <button className="action-btn" onClick={() => setDetailItem(x)}>详细</button>
                        {canManageAll ? <button className="action-btn action-btn-accent" onClick={() => openEditModal(x)}>编辑</button> : null}
                        {canManageAll && x.status === 'active' ? (
                          <button className="action-btn" onClick={() => handleSyncExternalBan(x)} disabled={syncingExternalId === x.id}>
                            {syncingExternalId === x.id ? '同步中' : '同步外部'}
                          </button>
                        ) : null}
                        {canUnban(x) && x.status === 'active' ? <button className="action-btn action-btn-success" onClick={() => handleUnban(x)}>解封</button> : null}
                        {canCreate && x.status === 'inactive' ? <button className="action-btn action-btn-danger" onClick={() => openRebanModal(x)}>重新封禁</button> : null}
                        {canManageAll ? <button className="action-btn action-btn-danger" onClick={() => handleDeleteBan(x)}>删除</button> : null}
                      </div>
                    </td>
                  </tr>
                ))}
                {!isLoading && !loadError && items.length === 0 ? <tr><td colSpan={8} style={{ textAlign: 'center', color: 'var(--text2)' }}>暂无封禁记录</td></tr> : null}
              </tbody>
            </table>
          </div>
        </div>
      </div>

      <Pagination page={page} pageSize={20} total={total} onChange={setPage} />

      <BanFormModal
        open={formOpen}
        mode={formMode}
        editingBanId={editingBanId}
        reportReview={reportReview}
        prefillForm={prefillForm}
        onClose={() => setFormOpen(false)}
        onSuccess={handleFormSuccess}
        token={token}
      />

      <BanDetailModal
        open={Boolean(detailItem)}
        item={detailItem}
        onClose={() => setDetailItem(null)}
        canManageAll={canManageAll}
      />

      <BanApiModal
        open={apiModalOpen}
        onClose={() => setApiModalOpen(false)}
      />

      {dialog}
      <ToastContainer toasts={toasts} onDismiss={dismissToast} />
    </div>
  );
}

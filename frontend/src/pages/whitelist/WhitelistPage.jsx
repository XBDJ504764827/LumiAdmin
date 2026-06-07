import React, { useCallback, useEffect, useRef, useState } from 'react';
import { api } from '../../lib/api.js';
import { useConfirmDialog } from '../../shared/ConfirmModal.jsx';
import { useAuth } from '../../state/auth.jsx';
import { useToast, ToastContainer } from '../../shared/Toast.jsx';
import { SearchBar } from '../../shared/SearchBar.jsx';
import { Pagination } from '../../shared/Pagination.jsx';
import { formatChinaDateTime } from '../../shared/time.js';
import { notifyPendingReviewsUpdated, usePendingReviewIndicators } from '../../hooks/usePendingReviewIndicators.js';
import { fetchGlobalBansBatch, parseBanData, inferGlobalBanRisk } from './whitelistGlobalBans.js';
import { ManualCreateModal, RejectModal, ApproveModal, BanDetailModal, PlayerDetailModal } from './WhitelistModals.jsx';
import { InternalNoteInline } from '../../shared/InternalNote.jsx';

const emptyManualForm = { nickname: '', steam_input: '' };
const APPROVE_REVIEW_SECONDS = 5;
const PLAYER_LINK_TARGETS = [
  { key: 'gokz', label: 'GOKZ.TOP', href: (steamid64) => `https://kzcharm.com/profile/${steamid64}` },
  { key: 'kzgo', label: 'KZGO.EU', href: (steamid64) => `https://kzgo.eu/players/${steamid64}` },
];
const PLAYER_CONTEXT_MENU_SIZE = { width: 180, height: 112 };

function emptyApproveModal() {
  return { open: false, item: null, reason: '', error: '', bans: [], secondsRemaining: APPROVE_REVIEW_SECONDS };
}

// ---------------------------------------------------------------------------
// 表格内联辅助函数（消除三个 tab 分支的重复 JSX）
// ---------------------------------------------------------------------------

function renderNicknameCell(item, globalBans, openBanDetail) {
  const itemBans = globalBans[item.steamid64];
  const hasGlobalBan = Array.isArray(itemBans) && itemBans.length > 0;
  return (
    <td className="fw-600" data-player-info="true">
      <div className="nickname-cell">{item.nickname}</div>
      {hasGlobalBan && (
        <button className="global-ban-btn" onClick={() => openBanDetail(item.steamid64)}>
          <span className="global-ban-icon">⚠</span>
          <span>全球封禁</span>
          <span className="global-ban-count">{itemBans.length}</span>
        </button>
      )}
      <InternalNoteInline steamid64={item.steamid64} />
    </td>
  );
}

function renderSteamNameCell(item, canRefreshSteam, refreshing, onRefresh) {
  return (
    <td className={item.steam_persona_name ? 'text-accent' : 'text-muted-light'} data-player-info="true">
      <div className="flex items-center gap-4">
        <span>{item.steam_persona_name ?? '-'}</span>
        {canRefreshSteam ? (
          <button
            className="action-btn"
            style={{ padding: '2px 6px' }}
            onClick={() => onRefresh(item)}
            disabled={refreshing}
            title="刷新Steam名称"
          >↻</button>
        ) : null}
      </div>
    </td>
  );
}

function rowClassName(item, globalBans) {
  const itemBans = globalBans[item.steamid64];
  return Array.isArray(itemBans) && itemBans.length > 0 ? 'row-global-ban' : '';
}

// ---------------------------------------------------------------------------
// 主组件
// ---------------------------------------------------------------------------

export function WhitelistPage() {
  const { session } = useAuth();
  const { confirm, dialog } = useConfirmDialog();
  const { toast, toasts, dismiss: dismissToast } = useToast();
  const { counts: pendingCounts } = usePendingReviewIndicators();
  const token = session?.token ?? null;
  function getSavedTab() {
    try { return localStorage.getItem('whitelist_tab') || 'pending'; } catch { return 'pending'; }
  }
  const [tab, setTab] = useState(getSavedTab);
  const [search, setSearch] = useState('');
  const [page, setPage] = useState(1);
  const [data, setData] = useState(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState('');
  const [manualModalOpen, setManualModalOpen] = useState(false);
  const [rejectModal, setRejectModal] = useState({ open: false, item: null, reason: '', error: '' });
  const [approveModal, setApproveModal] = useState(emptyApproveModal);
  const [manualForm, setManualForm] = useState(emptyManualForm);
  const [manualError, setManualError] = useState('');
  const [submitting, setSubmitting] = useState(false);
  const [globalBansVersion, setGlobalBansVersion] = useState(0);
  const globalBansRef = useRef({});
  const [globalBansLoading, setGlobalBansLoading] = useState(false);
  const fetchedSteamIdsRef = useRef(new Set());
  const [banDetailModal, setBanDetailModal] = useState({ open: false, steamid64: '', bans: [] });
  const [detailModal, setDetailModal] = useState({ open: false, item: null });
  const [refreshing, setRefreshing] = useState(false);
  const [playerContextMenu, setPlayerContextMenu] = useState({
    open: false, x: 0, y: 0, steamid64: '', nickname: '',
  });

  const canManualCreate = session?.role === 'developer' || session?.role === 'admin';
  const canReview = ['developer', 'admin', 'normal'].includes(session?.role);
  const canRevoke = session?.role === 'developer' || session?.role === 'admin';
  const canRefreshSteam = session?.role === 'developer';

  // ---------------------------------------------------------------------------
  // 数据加载
  // ---------------------------------------------------------------------------

  const loadItems = useCallback(async () => {
    try {
      setLoading(true);
      setError('');
      const params = { page, page_size: 20, status: tab };
      if (search) params.search = search;
      const response = await api.whitelist(token, params);
      setData(response);
    } catch (loadError) {
      setError(loadError.message);
      setData(null);
    } finally {
      setLoading(false);
    }
  }, [token, tab, page, search]);

  useEffect(() => { loadItems(); }, [loadItems]);

  useEffect(() => {
    if (!approveModal.open || approveModal.secondsRemaining <= 0) return undefined;
    const timer = window.setTimeout(() => {
      setApproveModal((prev) => {
        if (!prev.open || prev.secondsRemaining <= 0) return prev;
        return { ...prev, secondsRemaining: prev.secondsRemaining - 1 };
      });
    }, 1000);
    return () => window.clearTimeout(timer);
  }, [approveModal.open, approveModal.secondsRemaining]);

  useEffect(() => { try { localStorage.setItem('whitelist_tab', tab); } catch {} }, [tab]);

  useEffect(() => {
    if (!playerContextMenu.open) return undefined;
    const closeMenu = () => setPlayerContextMenu((prev) => ({ ...prev, open: false }));
    const handleKeyDown = (event) => { if (event.key === 'Escape') closeMenu(); };
    window.addEventListener('click', closeMenu);
    window.addEventListener('resize', closeMenu);
    window.addEventListener('scroll', closeMenu, true);
    window.addEventListener('keydown', handleKeyDown);
    return () => {
      window.removeEventListener('click', closeMenu);
      window.removeEventListener('resize', closeMenu);
      window.removeEventListener('scroll', closeMenu, true);
      window.removeEventListener('keydown', handleKeyDown);
    };
  }, [playerContextMenu.open]);

  useEffect(() => {
    if (!data?.items) return;
    const toCheck = data.items.map(item => item.steamid64).filter(id => id && !fetchedSteamIdsRef.current.has(id));
    if (toCheck.length === 0) return;
    toCheck.forEach(id => fetchedSteamIdsRef.current.add(id));
    let cancelled = false;
    setGlobalBansLoading(true);
    const BATCH_SIZE = 20;
    const batches = [];
    for (let i = 0; i < toCheck.length; i += BATCH_SIZE) {
      batches.push(toCheck.slice(i, i + BATCH_SIZE));
    }
    (async () => {
      for (const batch of batches) {
        if (cancelled) break;
        try {
          const results = await fetchGlobalBansBatch(batch);
          for (const [steamid64, banData] of Object.entries(results)) {
            globalBansRef.current[steamid64] = parseBanData(banData);
          }
        } catch { /* 单批失败不阻断后续批次 */ }
      }
      if (!cancelled) { setGlobalBansVersion(v => v + 1); setGlobalBansLoading(false); }
    })();
    return () => { cancelled = true; };
  }, [tab, data?.items]);

  // ---------------------------------------------------------------------------
  // 操作处理
  // ---------------------------------------------------------------------------

  function switchTab(newTab) {
    setTab(newTab);
    setPage(1);
    fetchedSteamIdsRef.current.clear();
    globalBansRef.current = {};
    setGlobalBansVersion(0);
  }

  async function handleApprove(item) {
    const itemBans = globalBans[item.steamid64];
    const hasGlobalBan = Array.isArray(itemBans) && itemBans.length > 0;
    if (hasGlobalBan) {
      setApproveModal({ ...emptyApproveModal(), open: true, item, bans: itemBans });
      return;
    }
    try {
      setSubmitting(true);
      await api.approveWhitelist(token, item.id);
      await loadItems();
      notifyPendingReviewsUpdated({ source: 'whitelist', action: 'approve' });
      toast({ title: '审核通过', message: `${item.nickname} 的白名单申请已通过。` });
    } catch (actionError) {
      toast({ title: '操作失败', message: actionError.message, tone: 'danger' });
    } finally { setSubmitting(false); }
  }

  async function handleApproveWithReason() {
    if (!approveModal.item) return;
    if (approveModal.secondsRemaining > 0) {
      setApproveModal((prev) => ({ ...prev, error: `请先查看全球封禁详情，${prev.secondsRemaining} 秒后才能确认通过。` }));
      return;
    }
    if (!approveModal.reason.trim()) {
      setApproveModal((prev) => ({ ...prev, error: '该玩家有全球封禁记录，请说明通过理由。' }));
      return;
    }
    try {
      setSubmitting(true);
      await api.approveWhitelist(token, approveModal.item.id, { reason: approveModal.reason.trim() });
      setApproveModal(emptyApproveModal());
      await loadItems();
      notifyPendingReviewsUpdated({ source: 'whitelist', action: 'approve' });
      toast({ title: '审核通过', message: `${approveModal.item.nickname} 的白名单申请已通过。` });
    } catch (actionError) {
      setApproveModal((prev) => ({ ...prev, error: actionError.message }));
    } finally { setSubmitting(false); }
  }

  function openRejectModal(item) {
    setRejectModal({ open: true, item, reason: '', error: '' });
  }

  async function handleReject() {
    if (!rejectModal.item) return;
    if (!rejectModal.reason.trim()) {
      setRejectModal((prev) => ({ ...prev, error: '请输入拒绝理由。' }));
      return;
    }
    try {
      setSubmitting(true);
      await api.rejectWhitelist(token, rejectModal.item.id, { reason: rejectModal.reason.trim() });
      setRejectModal({ open: false, item: null, reason: '', error: '' });
      await loadItems();
      notifyPendingReviewsUpdated({ source: 'whitelist', action: 'reject' });
      toast({ title: '已拒绝', message: `已拒绝 ${rejectModal.item.nickname} 的白名单申请。` });
    } catch (actionError) {
      setRejectModal((prev) => ({ ...prev, error: actionError.message }));
    } finally { setSubmitting(false); }
  }

  async function handleRestore(item) {
    try {
      setSubmitting(true);
      await api.restoreWhitelist(token, item.id);
      await loadItems();
      notifyPendingReviewsUpdated({ source: 'whitelist', action: 'restore' });
      toast({ title: '恢复成功', message: `${item.nickname} 的白名单已恢复。` });
    } catch (actionError) {
      toast({ title: '操作失败', message: actionError.message, tone: 'danger' });
    } finally { setSubmitting(false); }
  }

  async function handleRevoke(item) {
    const confirmed = await confirm({
      title: '删除白名单审核',
      message: '确定删除该玩家的白名单审核吗？删除后玩家需要重新申请。',
    });
    if (!confirmed) return;
    try {
      setSubmitting(true);
      await api.revokeWhitelist(token, item.id);
      await loadItems();
      notifyPendingReviewsUpdated({ source: 'whitelist', action: 'revoke' });
      toast({ title: '删除成功', message: `${item.nickname} 的白名单审核已删除。` });
    } catch (actionError) {
      toast({ title: '操作失败', message: actionError.message, tone: 'danger' });
    } finally { setSubmitting(false); }
  }

  async function handleManualCreate() {
    if (!manualForm.nickname.trim()) { setManualError('请输入玩家名称。'); return; }
    if (!manualForm.steam_input.trim()) { setManualError('请输入玩家标识。'); return; }
    try {
      setSubmitting(true);
      setManualError('');
      await api.createManualWhitelist(token, {
        nickname: manualForm.nickname.trim(),
        steam_input: manualForm.steam_input.trim(),
      });
      setManualModalOpen(false);
      setManualForm(emptyManualForm);
      await loadItems();
      notifyPendingReviewsUpdated({ source: 'whitelist', action: 'manual_create' });
      toast({ title: '添加成功', message: `已手动添加 ${manualForm.nickname.trim()} 的白名单。` });
    } catch (actionError) {
      setManualError(actionError.message);
    } finally { setSubmitting(false); }
  }

  function openBanDetail(steamid64) {
    setBanDetailModal({ open: true, steamid64, bans: globalBans[steamid64] || [] });
  }

  function openPlayerContextMenu(event, item) {
    if (!item.steamid64) return;
    event.preventDefault();
    event.stopPropagation();
    const maxX = window.innerWidth - PLAYER_CONTEXT_MENU_SIZE.width - 8;
    const maxY = window.innerHeight - PLAYER_CONTEXT_MENU_SIZE.height - 8;
    setPlayerContextMenu({
      open: true,
      x: Math.max(8, Math.min(event.clientX, maxX)),
      y: Math.max(8, Math.min(event.clientY, maxY)),
      steamid64: item.steamid64,
      nickname: item.nickname || item.steamid64,
    });
  }

  function handlePendingRowContextMenu(event, item) {
    if (event.target.closest('[data-player-info="true"], button, a, input, textarea, select')) return;
    openPlayerContextMenu(event, item);
  }

  function openPlayerLink(target) {
    if (!playerContextMenu.steamid64) return;
    window.open(target.href(playerContextMenu.steamid64), '_blank', 'noopener,noreferrer');
    setPlayerContextMenu((prev) => ({ ...prev, open: false }));
  }

  async function handleRefreshSteamName(item) {
    try {
      setRefreshing(true);
      const result = await api.refreshSingleSteamName(token, item.id);
      await loadItems();
      toast({ title: '刷新成功', message: `Steam名称已更新为: ${result.steam_persona_name ?? '(未获取到)'}` });
    } catch (actionError) {
      toast({ title: '刷新失败', message: actionError.message, tone: 'danger' });
    } finally { setRefreshing(false); }
  }

  async function handleRefreshAllSteamNames() {
    try {
      setRefreshing(true);
      const result = await api.refreshAllSteamNames(token, tab);
      await loadItems();
      toast({ title: '批量刷新完成', message: `成功更新了 ${result.updated_count} 条记录的Steam名称。` });
    } catch (actionError) {
      toast({ title: '批量刷新失败', message: actionError.message, tone: 'danger' });
    } finally { setRefreshing(false); }
  }

  // ---------------------------------------------------------------------------
  // 派生值
  // ---------------------------------------------------------------------------

  const globalBans = globalBansRef.current;
  const items = data?.items ?? [];
  const total = data?.total ?? 0;
  const hasPendingWhitelist = (pendingCounts.whitelist ?? 0) > 0;
  const approveGlobalBanRisk = inferGlobalBanRisk(approveModal.bans);

  // ---------------------------------------------------------------------------
  // 渲染
  // ---------------------------------------------------------------------------

  const closeRejectModal = () => setRejectModal({ open: false, item: null, reason: '', error: '' });
  const closeApproveModal = () => setApproveModal(emptyApproveModal());
  const closeBanDetailModal = () => setBanDetailModal({ open: false, steamid64: '', bans: [] });
  const closeDetailModal = () => setDetailModal({ open: false, item: null });
  const closeManualModal = () => { setManualModalOpen(false); setManualForm(emptyManualForm); setManualError(''); };
  const setRejectReason = (reason) => setRejectModal((prev) => ({ ...prev, reason, error: '' }));
  const setApproveReason = (reason) => setApproveModal((prev) => ({ ...prev, reason, error: '' }));

  return (
    <div id="whitelist" className="content-section active">
      <div className="breadcrumb"><span>核心管理</span><span className="sep">›</span><span className="current">白名单管理</span></div>
      <div className="page-header">
        <div>
          <div className="page-title">白名单审核大厅</div>
          <div className="page-sub">处理玩家申请，或手动添加特定玩家。</div>
        </div>
        <div className="flex gap-10">
          {canManualCreate ? <button className="btn btn-accent" onClick={() => setManualModalOpen(true)}>手动添加白名单</button> : null}
          {canRefreshSteam ? <button className="btn btn-outline" onClick={handleRefreshAllSteamNames} disabled={refreshing}>{refreshing ? '刷新中...' : '刷新Steam名称'}</button> : null}
        </div>
      </div>

      <div className="tabs">
        <button className={`tab ${tab === 'pending' ? 'active' : ''}`} onClick={() => switchTab('pending')}>
          <span>待审核</span>
          {hasPendingWhitelist ? <span className="tab-pending-dot" title={`有 ${pendingCounts.whitelist} 条待审核白名单`} /> : null}
        </button>
        <button className={`tab ${tab === 'approved' ? 'active' : ''}`} onClick={() => switchTab('approved')}>已通过</button>
        <button className={`tab ${tab === 'rejected' ? 'active' : ''}`} onClick={() => switchTab('rejected')}>未通过</button>
      </div>

      <SearchBar value={search} onChange={(v) => { setSearch(v); setPage(1); }} placeholder="搜索 SteamID64 / 玩家名称..." />

      <div className="card">
        <div className="card-header">
          <div>
            <div className="card-title">
              申请列表
              {globalBansLoading ? <span style={{ fontSize: 12, fontWeight: 400, color: 'var(--text3)', marginLeft: 8 }}>正在加载全球封禁记录...</span> : null}
            </div>
            <div className="card-sub">当前白名单申请记录</div>
          </div>
        </div>
        <div className="card-body p-0">
          {loading ? <div className="p-20">正在加载白名单数据...</div> : null}
          {!loading && error ? <div style={{ padding: 20, color: 'var(--accent)' }}>{error}</div> : null}
          {!loading && !error ? (
            <div className="table-responsive">
              <table className="data-table">
                <thead>
                  {tab === 'pending' ? (
                    <tr><th>游戏昵称</th><th>Steam 名称</th><th>SteamID64</th><th>SteamID2</th><th>SteamID3</th><th>申请时间</th><th className="text-right">操作</th></tr>
                  ) : null}
                  {tab === 'approved' ? (
                    <tr><th>游戏昵称</th><th>Steam 名称</th><th>SteamID64</th><th>SteamID2</th><th>SteamID3</th><th>申请时间</th><th>通过时间</th><th>审核管理员</th><th>通过理由</th><th className="text-right">操作</th></tr>
                  ) : null}
                  {tab === 'rejected' ? (
                    <tr><th>游戏昵称</th><th>Steam 名称</th><th>SteamID64</th><th>SteamID2</th><th>SteamID3</th><th>拒绝理由</th><th>申请时间</th><th>拒绝时间</th><th>审核管理员</th><th className="text-right">操作</th></tr>
                  ) : null}
                </thead>
                <tbody>
                  {items.length === 0 ? (
                    <tr><td colSpan={tab === 'pending' ? 7 : 10} style={{ padding: 20, color: 'var(--text3)' }}>当前分区暂无记录。</td></tr>
                  ) : null}
                  {tab === 'pending' ? items.map((item) => (
                    <tr key={item.id} className={rowClassName(item, globalBans)} onContextMenu={(event) => handlePendingRowContextMenu(event, item)}>
                      {renderNicknameCell(item, globalBans, openBanDetail)}
                      {renderSteamNameCell(item, canRefreshSteam, refreshing, handleRefreshSteamName)}
                      <td className="steam-id" data-player-info="true">{item.steamid64}</td>
                      <td className="steam-id" data-player-info="true">{item.steamid ?? '-'}</td>
                      <td className="steam-id" data-player-info="true">{item.steamid3 ?? '-'}</td>
                      <td className="text-muted-light" data-player-info="true">{formatChinaDateTime(item.applied_at)}</td>
                      <td className="text-right">
                        {canReview ? <div className="action-btn-group">
                          <button className="action-btn action-btn-accent" onClick={() => setDetailModal({ open: true, item })}>详细</button>
                          <button className="action-btn action-btn-success" onClick={() => handleApprove(item)} disabled={submitting}>通过</button>
                          <button className="action-btn action-btn-danger" onClick={() => openRejectModal(item)} disabled={submitting}>拒绝</button>
                        </div> : null}
                      </td>
                    </tr>
                  )) : null}
                  {tab === 'approved' ? items.map((item) => (
                    <tr key={item.id} className={rowClassName(item, globalBans)}>
                      {renderNicknameCell(item, globalBans, openBanDetail)}
                      {renderSteamNameCell(item, canRefreshSteam, refreshing, handleRefreshSteamName)}
                      <td className="steam-id">{item.steamid64}</td>
                      <td className="steam-id">{item.steamid ?? '-'}</td>
                      <td className="steam-id">{item.steamid3 ?? '-'}</td>
                      <td className="text-muted-light">{formatChinaDateTime(item.applied_at)}</td>
                      <td className="text-muted-light">{formatChinaDateTime(item.approved_at)}</td>
                      <td>{item.approved_by ?? '-'}</td>
                      <td style={{ color: item.approval_reason ? 'var(--accent2)' : 'var(--text3)', maxWidth: 200, wordBreak: 'break-word' }}>{item.approval_reason ?? '-'}</td>
                      <td className="text-right">
                        {canRevoke ? <button className="action-btn action-btn-danger" onClick={() => handleRevoke(item)} disabled={submitting}>删除审核</button> : null}
                      </td>
                    </tr>
                  )) : null}
                  {tab === 'rejected' ? items.map((item) => (
                    <tr key={item.id} className={rowClassName(item, globalBans)}>
                      {renderNicknameCell(item, globalBans, openBanDetail)}
                      {renderSteamNameCell(item, canRefreshSteam, refreshing, handleRefreshSteamName)}
                      <td className="steam-id">{item.steamid64}</td>
                      <td className="steam-id">{item.steamid ?? '-'}</td>
                      <td className="steam-id">{item.steamid3 ?? '-'}</td>
                      <td>{item.rejection_reason ?? '-'}</td>
                      <td className="text-muted-light">{formatChinaDateTime(item.applied_at)}</td>
                      <td className="text-muted-light">{formatChinaDateTime(item.rejected_at)}</td>
                      <td>{item.rejected_by ?? '-'}</td>
                      <td className="text-right">
                        {canReview ? <button className="action-btn action-btn-success" onClick={() => handleRestore(item)} disabled={submitting}>恢复通过</button> : null}
                      </td>
                    </tr>
                  )) : null}
                </tbody>
              </table>
            </div>
          ) : null}
        </div>
      </div>

      <Pagination page={page} pageSize={20} total={total} onChange={setPage} />

      {playerContextMenu.open ? (
        <div
          className="player-context-menu"
          style={{ left: playerContextMenu.x, top: playerContextMenu.y }}
          role="menu"
          onClick={(event) => event.stopPropagation()}
        >
          <div className="player-context-menu-title">{playerContextMenu.nickname}</div>
          <div className="player-context-menu-id">{playerContextMenu.steamid64}</div>
          <div className="player-context-menu-separator" />
          {PLAYER_LINK_TARGETS.map((target) => (
            <button key={target.key} type="button" className="player-context-menu-item" role="menuitem" onClick={() => openPlayerLink(target)}>
              {target.label}
            </button>
          ))}
        </div>
      ) : null}

      <ManualCreateModal
        open={manualModalOpen}
        onClose={closeManualModal}
        form={manualForm}
        setForm={setManualForm}
        error={manualError}
        onSubmit={handleManualCreate}
        submitting={submitting}
      />
      <RejectModal
        open={rejectModal.open}
        onClose={closeRejectModal}
        reason={rejectModal.reason}
        setReason={setRejectReason}
        error={rejectModal.error}
        onSubmit={handleReject}
        submitting={submitting}
      />
      <ApproveModal
        open={approveModal.open}
        onClose={closeApproveModal}
        item={approveModal.item}
        bans={approveModal.bans}
        risk={approveGlobalBanRisk}
        reason={approveModal.reason}
        setReason={setApproveReason}
        error={approveModal.error}
        secondsRemaining={approveModal.secondsRemaining}
        onSubmit={handleApproveWithReason}
        submitting={submitting}
      />
      <BanDetailModal
        open={banDetailModal.open}
        onClose={closeBanDetailModal}
        steamid64={banDetailModal.steamid64}
        bans={banDetailModal.bans}
      />
      <PlayerDetailModal
        open={detailModal.open}
        onClose={closeDetailModal}
        item={detailModal.item}
      />

      {dialog}
      <ToastContainer toasts={toasts} onDismiss={dismissToast} />
    </div>
  );
}

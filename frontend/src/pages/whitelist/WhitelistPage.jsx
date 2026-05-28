import React, { useCallback, useEffect, useRef, useState } from 'react';
import { api } from '../../lib/api.js';
import { useConfirmDialog } from '../../shared/ConfirmModal.jsx';
import { Modal } from '../../shared/Modal.jsx';
import { useAuth } from '../../state/auth.jsx';
import { useToast, ToastContainer } from '../../shared/Toast.jsx';
import { SearchBar } from '../../shared/SearchBar.jsx';
import { Pagination } from '../../shared/Pagination.jsx';
import { formatChinaDateTime } from '../../shared/time.js';
import { notifyPendingReviewsUpdated, usePendingReviewIndicators } from '../../hooks/usePendingReviewIndicators.js';

const GLOBAL_BANS_SESSION_CACHE = new Map();

// 批量查询全球封禁记录（通过后端代理）
async function fetchGlobalBansBatch(steamids) {
  const results = {};
  const missingSteamIds = [];

  for (const steamid of new Set(steamids.filter(Boolean))) {
    if (GLOBAL_BANS_SESSION_CACHE.has(steamid)) {
      results[steamid] = GLOBAL_BANS_SESSION_CACHE.get(steamid);
    } else {
      missingSteamIds.push(steamid);
    }
  }

  if (missingSteamIds.length === 0) return results;

  try {
    const response = await fetch('/api/public/global-bans/batch', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ steamids: missingSteamIds }),
    });
    if (!response.ok) return results;
    const data = await response.json();
    const fetchedResults = data.results || {};
    for (const [steamid, value] of Object.entries(fetchedResults)) {
      GLOBAL_BANS_SESSION_CACHE.set(steamid, value);
      results[steamid] = value;
    }
    return results;
  } catch {
    return results;
  }
}

// 解析封禁数据
// KZTimerGlobal（主 API）返回数组 [{...}, ...]
// GOKZ.TOP（备用 API）返回 { data: [...], count: N }
function parseBanData(data) {
  if (Array.isArray(data) && data.length > 0) return data;
  if (Array.isArray(data.data) && data.data.length > 0) return data.data;
  return [];
}

const emptyManualForm = {
  nickname: '',
  steam_input: '',
};

const APPROVE_REVIEW_SECONDS = 5;
const STATUS_MAP = { pending: 'pending', approved: 'approved', rejected: 'rejected' };
const PLAYER_LINK_TARGETS = [
  {
    key: 'gokz',
    label: 'GOKZ.TOP',
    href: (steamid64) => `https://kzcharm.com/profile/${steamid64}`,
  },
  {
    key: 'kzgo',
    label: 'KZGO.EU',
    href: (steamid64) => `https://kzgo.eu/players/${steamid64}`,
  },
];
const PLAYER_CONTEXT_MENU_SIZE = { width: 180, height: 112 };

function emptyApproveModal() {
  return {
    open: false,
    item: null,
    reason: '',
    error: '',
    bans: [],
    secondsRemaining: APPROVE_REVIEW_SECONDS,
  };
}

function GlobalBanRecordList({ bans }) {
  if (!bans.length) {
    return <div className="global-ban-empty">暂无封禁记录</div>;
  }

  return (
    <div className="global-ban-list">
      {bans.map((ban, index) => (
        <div key={index} className="global-ban-item">
          <div className="global-ban-item-header">
            <span className="global-ban-type">{ban.ban_type || '作弊'}</span>
            {ban.expires_on ? (
              <span className="global-ban-temporary">临时</span>
            ) : (
              <span className="global-ban-permanent">永久</span>
            )}
          </div>
          <div className="global-ban-item-body">
            {ban.player_name && (
              <div className="global-ban-field">
                <span className="global-ban-label">玩家</span>
                <span className="global-ban-value">{ban.player_name}</span>
              </div>
            )}
            {ban.notes && (
              <div className="global-ban-field">
                <span className="global-ban-label">备注</span>
                <span className="global-ban-value">{ban.notes}</span>
              </div>
            )}
            {ban.stats && (
              <div className="global-ban-field">
                <span className="global-ban-label">统计</span>
                <span className="global-ban-value global-ban-stats">{ban.stats}</span>
              </div>
            )}
            {ban.created_on && (
              <div className="global-ban-field">
                <span className="global-ban-label">封禁时间</span>
                <span className="global-ban-value">{formatChinaDateTime(ban.created_on)}</span>
              </div>
            )}
            {ban.expires_on && (
              <div className="global-ban-field">
                <span className="global-ban-label">到期时间</span>
                <span className="global-ban-value">{formatChinaDateTime(ban.expires_on)}</span>
              </div>
            )}
            {ban.server_name && (
              <div className="global-ban-field">
                <span className="global-ban-label">服务器</span>
                <span className="global-ban-value">{ban.server_name}</span>
              </div>
            )}
          </div>
        </div>
      ))}
    </div>
  );
}

function parseGlobalBanStats(stats) {
  if (!stats || typeof stats !== 'string') return {};
  const perfsMatch = stats.match(/Perfs:\s*(\d+)\s*\/\s*(\d+)/i);
  const averageMatch = stats.match(/Average:\s*(-?\d+(?:\.\d+)?)/i);
  return {
    perfs: perfsMatch ? Number(perfsMatch[1]) : null,
    perfTotal: perfsMatch ? Number(perfsMatch[2]) : null,
    average: averageMatch ? Number(averageMatch[1]) : null,
  };
}

function isPermanentGlobalBan(ban) {
  if (!ban?.expires_on) return true;
  const value = String(ban.expires_on);
  return value.startsWith('9999') || value.startsWith('2099');
}

function isActiveGlobalBan(ban) {
  if (!ban?.expires_on || isPermanentGlobalBan(ban)) return true;
  const expiresAt = new Date(ban.expires_on).getTime();
  return Number.isFinite(expiresAt) && expiresAt > Date.now();
}

function inferGlobalBanRisk(bans) {
  if (!Array.isArray(bans) || bans.length === 0) return null;

  const typeScores = new Map();
  const reasons = [];
  let totalScore = 0;
  let strongestScore = 0;
  let permanentOrActiveCount = 0;
  let hackSignalCount = 0;
  let macroSignalCount = 0;
  let expiredFiniteCount = 0;

  for (const ban of bans) {
    const banType = String(ban?.ban_type ?? '').toLowerCase();
    const notes = String(ban?.notes ?? '').toLowerCase();
    const { perfs, perfTotal, average } = parseGlobalBanStats(ban?.stats);
    const hasPermanentOrActive = isActiveGlobalBan(ban);
    const perfsRatio = perfTotal ? perfs / perfTotal : 0;

    let recordType = banType || 'bhop异常';
    let recordScore = 0;
    const recordReasons = [];

    if (banType.includes('hack') || notes.includes("1's or 2's") || notes.includes('1s or 2s')) {
      recordType = banType || 'bhop_hack';
      recordScore += 5;
      hackSignalCount += 1;
      recordReasons.push(`命中 ${recordType} / 低滚轮模式特征`);
    }

    if (banType.includes('macro') || notes.includes('high scroll pattern')) {
      recordType = banType || 'bhop_macro';
      recordScore += 4;
      macroSignalCount += 1;
      recordReasons.push(`命中 ${recordType} / 高滚轮模式特征`);
    }

    if (average !== null && average <= 3) {
      recordType = banType || 'bhop_hack';
      recordScore += 3;
      hackSignalCount += 1;
      recordReasons.push(`滚轮平均值 ${average.toFixed(2)} 偏低`);
    }

    if (average !== null && average >= 14) {
      recordType = banType || 'bhop_macro';
      recordScore += 2;
      macroSignalCount += 1;
      recordReasons.push(`滚轮平均值 ${average.toFixed(2)} 偏高`);
    }

    if (perfsRatio >= 0.6) {
      recordScore += 2;
      recordReasons.push(`Perfs 命中 ${perfs}/${perfTotal}`);
    }

    if (hasPermanentOrActive) {
      recordScore += 3;
      permanentOrActiveCount += 1;
      recordReasons.push('封禁永久或仍未到期');
    } else if (ban?.expires_on) {
      expiredFiniteCount += 1;
    }

    if (recordScore > 0) {
      typeScores.set(recordType, (typeScores.get(recordType) ?? 0) + recordScore);
      totalScore += recordScore;
      strongestScore = Math.max(strongestScore, recordScore);
      reasons.push(...recordReasons.slice(0, 3));
    }
  }

  if (bans.length >= 2) {
    totalScore += 2;
    reasons.push(`存在 ${bans.length} 条全球封禁记录`);
  }

  const sortedTypes = [...typeScores.entries()].sort((a, b) => b[1] - a[1]);
  const label = sortedTypes[0]?.[0] ?? 'bhop异常';
  const allExpiredFinite = expiredFiniteCount === bans.length;
  const hasStrongSuspicion = totalScore >= 8 || strongestScore >= 7 || permanentOrActiveCount > 0 || hackSignalCount > 0 || macroSignalCount >= 2;

  if (hasStrongSuspicion) {
    return {
      tone: 'danger',
      label,
      title: `系统判断该玩家高度疑似 ${label}，请谨慎审核！`,
      reasons: [...new Set(reasons)].slice(0, 4),
    };
  }

  if (allExpiredFinite) {
    return {
      tone: 'warning',
      label: '误封嫌疑',
      title: '系统判断该玩家全球封禁存在误封嫌疑！',
      reasons: ['全球封禁均已到期', '未命中明确的永久封禁、hack 或重复宏特征', ...new Set(reasons)].slice(0, 4),
    };
  }

  return {
    tone: 'warning',
    label: '需要人工复核',
    title: '系统无法确认该全球封禁风险，请人工复核封禁详情。',
    reasons: [...new Set(reasons)].slice(0, 4),
  };
}

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
  const [globalBansVersion, setGlobalBansVersion] = useState(0); // 递增触发渲染
  const globalBansRef = useRef({}); // steamid64 -> ban records
  const [globalBansLoading, setGlobalBansLoading] = useState(false); // 全球封禁加载状态
  const fetchedSteamIdsRef = useRef(new Set()); // 已查询过的 steamid64
  const [banDetailModal, setBanDetailModal] = useState({ open: false, steamid64: '', bans: [] });
  const [refreshing, setRefreshing] = useState(false); // Steam名称刷新状态
  const [playerContextMenu, setPlayerContextMenu] = useState({
    open: false,
    x: 0,
    y: 0,
    steamid64: '',
    nickname: '',
  });

  const canManualCreate = session?.role === 'developer' || session?.role === 'admin';
  const canReview = ['developer', 'admin', 'normal'].includes(session?.role);
  const canRevoke = session?.role === 'developer' || session?.role === 'admin';
  const canRefreshSteam = session?.role === 'developer'; // 只有开发管理员可以刷新Steam名称

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

  useEffect(() => {
    loadItems();
  }, [loadItems]);

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

  useEffect(() => {
    try { localStorage.setItem('whitelist_tab', tab); } catch {}
  }, [tab]);

  useEffect(() => {
    if (!playerContextMenu.open) return undefined;

    const closeMenu = () => setPlayerContextMenu((prev) => ({ ...prev, open: false }));
    const handleKeyDown = (event) => {
      if (event.key === 'Escape') closeMenu();
    };

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

  // 批量加载当前页所有玩家的全球封禁记录
  useEffect(() => {
    if (!data?.items) return;

    const toCheck = data.items
      .map(item => item.steamid64)
      .filter(id => id && !fetchedSteamIdsRef.current.has(id));

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
          for (const [steamid64, data] of Object.entries(results)) {
            globalBansRef.current[steamid64] = parseBanData(data);
          }
        } catch {
          // 单批失败不阻断后续批次
        }
      }
      if (!cancelled) {
        setGlobalBansVersion(v => v + 1);
        setGlobalBansLoading(false);
      }
    })();

    return () => { cancelled = true; };
  }, [tab, data?.items]);

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
      setApproveModal({
        ...emptyApproveModal(),
        open: true,
        item,
        bans: itemBans,
      });
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
    } finally {
      setSubmitting(false);
    }
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
      await api.approveWhitelist(token, approveModal.item.id, {
        reason: approveModal.reason.trim(),
      });
      setApproveModal(emptyApproveModal());
      await loadItems();
      notifyPendingReviewsUpdated({ source: 'whitelist', action: 'approve' });
      toast({ title: '审核通过', message: `${approveModal.item.nickname} 的白名单申请已通过。` });
    } catch (actionError) {
      setApproveModal((prev) => ({ ...prev, error: actionError.message }));
    } finally {
      setSubmitting(false);
    }
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
      await api.rejectWhitelist(token, rejectModal.item.id, {
        reason: rejectModal.reason.trim(),
      });
      setRejectModal({ open: false, item: null, reason: '', error: '' });
      await loadItems();
      notifyPendingReviewsUpdated({ source: 'whitelist', action: 'reject' });
      toast({ title: '已拒绝', message: `已拒绝 ${rejectModal.item.nickname} 的白名单申请。` });
    } catch (actionError) {
      setRejectModal((prev) => ({ ...prev, error: actionError.message }));
    } finally {
      setSubmitting(false);
    }
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
    } finally {
      setSubmitting(false);
    }
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
    } finally {
      setSubmitting(false);
    }
  }

  async function handleManualCreate() {
    if (!manualForm.nickname.trim()) {
      setManualError('请输入玩家名称。');
      return;
    }
    if (!manualForm.steam_input.trim()) {
      setManualError('请输入玩家标识。');
      return;
    }

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
    } finally {
      setSubmitting(false);
    }
  }

  function openBanDetail(steamid64) {
    const bans = globalBans[steamid64] || [];
    setBanDetailModal({ open: true, steamid64, bans });
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
    if (event.target.closest('[data-player-info="true"], button, a, input, textarea, select')) {
      return;
    }
    openPlayerContextMenu(event, item);
  }

  function openPlayerLink(target) {
    if (!playerContextMenu.steamid64) return;
    window.open(target.href(playerContextMenu.steamid64), '_blank', 'noopener,noreferrer');
    setPlayerContextMenu((prev) => ({ ...prev, open: false }));
  }

  // 刷新单条记录的Steam名称
  async function handleRefreshSteamName(item) {
    try {
      setRefreshing(true);
      const result = await api.refreshSingleSteamName(token, item.id);
      await loadItems();
      toast({ title: '刷新成功', message: `Steam名称已更新为: ${result.steam_persona_name ?? '(未获取到)'}` });
    } catch (actionError) {
      toast({ title: '刷新失败', message: actionError.message, tone: 'danger' });
    } finally {
      setRefreshing(false);
    }
  }

  // 批量刷新当前标签页所有记录的Steam名称
  async function handleRefreshAllSteamNames() {
    try {
      setRefreshing(true);
      const result = await api.refreshAllSteamNames(token, tab);
      await loadItems();
      toast({ title: '批量刷新完成', message: `成功更新了 ${result.updated_count} 条记录的Steam名称。` });
    } catch (actionError) {
      toast({ title: '批量刷新失败', message: actionError.message, tone: 'danger' });
    } finally {
      setRefreshing(false);
    }
  }

  // globalBansVersion 变化时重新读取 ref，触发表格重渲染
  const globalBans = globalBansRef.current;

  const items = data?.items ?? [];
  const total = data?.total ?? 0;
  const hasPendingWhitelist = (pendingCounts.whitelist ?? 0) > 0;
  const approveGlobalBanRisk = inferGlobalBanRisk(approveModal.bans);

  return (
    <div id="whitelist" className="content-section active">
      <div className="breadcrumb"><span>核心管理</span><span className="sep">›</span><span className="current">白名单管理</span></div>
      <div className="page-header">
        <div>
          <div className="page-title">白名单审核大厅</div>
          <div className="page-sub">处理玩家申请，或手动添加特定玩家。</div>
        </div>
        <div style={{ display: 'flex', gap: 10 }}>
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

      <SearchBar
        value={search}
        onChange={(v) => { setSearch(v); setPage(1); }}
        placeholder="搜索 SteamID64 / 玩家名称..."
      />

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
        <div className="card-body" style={{ padding: 0 }}>
          {loading ? <div style={{ padding: 20 }}>正在加载白名单数据...</div> : null}
          {!loading && error ? <div style={{ padding: 20, color: 'var(--accent)' }}>{error}</div> : null}
          {!loading && !error ? (
            <div className="table-responsive">
              <table className="data-table">
                <thead>
                  {tab === 'pending' ? (
                    <tr><th>游戏昵称</th><th>Steam 名称</th><th>SteamID64</th><th>SteamID2</th><th>SteamID3</th><th>申请时间</th><th style={{ textAlign: 'right' }}>操作</th></tr>
                  ) : null}
                  {tab === 'approved' ? (
                    <tr><th>游戏昵称</th><th>Steam 名称</th><th>SteamID64</th><th>SteamID2</th><th>SteamID3</th><th>申请时间</th><th>通过时间</th><th>审核管理员</th><th>通过理由</th><th style={{ textAlign: 'right' }}>操作</th></tr>
                  ) : null}
                  {tab === 'rejected' ? (
                    <tr><th>游戏昵称</th><th>Steam 名称</th><th>SteamID64</th><th>SteamID2</th><th>SteamID3</th><th>拒绝理由</th><th>申请时间</th><th>拒绝时间</th><th>审核管理员</th><th style={{ textAlign: 'right' }}>操作</th></tr>
                  ) : null}
                </thead>
                <tbody>
                  {items.length === 0 ? (
                    <tr><td colSpan={tab === 'pending' ? 7 : tab === 'approved' ? 10 : 10} style={{ padding: 20, color: 'var(--text3)' }}>当前分区暂无记录。</td></tr>
                  ) : null}
                  {tab === 'pending' ? items.map((item) => {
                    const itemBans = globalBans[item.steamid64];
                    const hasGlobalBan = Array.isArray(itemBans) && itemBans.length > 0;

                    return (
                      <tr
                        key={item.id}
                        className={hasGlobalBan ? 'row-global-ban' : ''}
                        onContextMenu={(event) => handlePendingRowContextMenu(event, item)}
                      >
                        <td style={{ fontWeight: 600 }} data-player-info="true">
<div className="nickname-cell">{item.nickname}</div>
                          {hasGlobalBan && (
                            <button className="global-ban-btn" onClick={() => openBanDetail(item.steamid64)}>
                              <span className="global-ban-icon">⚠</span>
                              <span>全球封禁</span>
                              <span className="global-ban-count">{itemBans.length}</span>
                            </button>
                          )}
                        </td>
                        <td style={{ color: item.steam_persona_name ? 'var(--accent2)' : 'var(--text3)' }} data-player-info="true">
                            <div style={{ display: 'flex', alignItems: 'center', gap: 4 }}>
                              <span>{item.steam_persona_name ?? '-'}</span>
                              {canRefreshSteam ? (
                                <button
                                  className="action-btn"
                                  style={{ padding: '2px 6px' }}
                                  onClick={() => handleRefreshSteamName(item)}
                                  disabled={refreshing}
                                  title="刷新Steam名称"
                                >↻</button>
                              ) : null}
                            </div>
                          </td>
                        <td className="steam-id" data-player-info="true">{item.steamid64}</td>
                        <td className="steam-id" data-player-info="true">{item.steamid ?? '-'}</td>
                        <td className="steam-id" data-player-info="true">{item.steamid3 ?? '-'}</td>
                        <td style={{ color: 'var(--text3)' }} data-player-info="true">{formatChinaDateTime(item.applied_at)}</td>
                        <td style={{ textAlign: 'right' }}>
                          {canReview ? <div className="action-btn-group">
                            <button className="action-btn action-btn-success" onClick={() => handleApprove(item)} disabled={submitting}>通过</button>
                            <button className="action-btn action-btn-danger" onClick={() => openRejectModal(item)} disabled={submitting}>拒绝</button>
                          </div> : null}
                        </td>
                      </tr>
                    );
                  }) : null}
                  {tab === 'approved' ? items.map((item) => {
                    const itemBans = globalBans[item.steamid64];
                    const hasGlobalBan = Array.isArray(itemBans) && itemBans.length > 0;
                    return (
                      <tr key={item.id} className={hasGlobalBan ? 'row-global-ban' : ''}>
                        <td style={{ fontWeight: 600 }}>
<div className="nickname-cell">{item.nickname}</div>
                          {hasGlobalBan && (
                            <button className="global-ban-btn" onClick={() => openBanDetail(item.steamid64)}>
                              <span className="global-ban-icon">⚠</span>
                              <span>全球封禁</span>
                              <span className="global-ban-count">{itemBans.length}</span>
                            </button>
                          )}
                        </td>
                      <td style={{ color: item.steam_persona_name ? 'var(--accent2)' : 'var(--text3)' }}>
                            <div style={{ display: 'flex', alignItems: 'center', gap: 4 }}>
                              <span>{item.steam_persona_name ?? '-'}</span>
                              {canRefreshSteam ? (
                                <button
                                  className="action-btn"
                                  style={{ padding: '2px 6px' }}
                                  onClick={() => handleRefreshSteamName(item)}
                                  disabled={refreshing}
                                  title="刷新Steam名称"
                                >↻</button>
                              ) : null}
                            </div>
                          </td>
                      <td className="steam-id">{item.steamid64}</td>
                      <td className="steam-id">{item.steamid ?? '-'}</td>
                      <td className="steam-id">{item.steamid3 ?? '-'}</td>
                      <td style={{ color: 'var(--text3)' }}>{formatChinaDateTime(item.applied_at)}</td>
                      <td style={{ color: 'var(--text3)' }}>{formatChinaDateTime(item.approved_at)}</td>
                      <td>{item.approved_by ?? '-'}</td>
                      <td style={{ color: item.approval_reason ? 'var(--accent2)' : 'var(--text3)', maxWidth: 200, wordBreak: 'break-word' }}>{item.approval_reason ?? '-'}</td>
                      <td style={{ textAlign: 'right' }}>
                        {canRevoke ? <button className="action-btn action-btn-danger" onClick={() => handleRevoke(item)} disabled={submitting}>删除审核</button> : null}
                      </td>
                    </tr>
                  );
                }) : null}
                  {tab === 'rejected' ? items.map((item) => {
                    const itemBans = globalBans[item.steamid64];
                    const hasGlobalBan = Array.isArray(itemBans) && itemBans.length > 0;
                    return (
                      <tr key={item.id} className={hasGlobalBan ? 'row-global-ban' : ''}>
                        <td style={{ fontWeight: 600 }}>
<div className="nickname-cell">{item.nickname}</div>
                          {hasGlobalBan && (
                            <button className="global-ban-btn" onClick={() => openBanDetail(item.steamid64)}>
                              <span className="global-ban-icon">⚠</span>
                              <span>全球封禁</span>
                              <span className="global-ban-count">{itemBans.length}</span>
                            </button>
                          )}
                        </td>
                      <td style={{ color: item.steam_persona_name ? 'var(--accent2)' : 'var(--text3)' }}>
                            <div style={{ display: 'flex', alignItems: 'center', gap: 4 }}>
                              <span>{item.steam_persona_name ?? '-'}</span>
                              {canRefreshSteam ? (
                                <button
                                  className="action-btn"
                                  style={{ padding: '2px 6px' }}
                                  onClick={() => handleRefreshSteamName(item)}
                                  disabled={refreshing}
                                  title="刷新Steam名称"
                                >↻</button>
                              ) : null}
                            </div>
                          </td>
                      <td className="steam-id">{item.steamid64}</td>
                      <td className="steam-id">{item.steamid ?? '-'}</td>
                      <td className="steam-id">{item.steamid3 ?? '-'}</td>
                      <td>{item.rejection_reason ?? '-'}</td>
                      <td style={{ color: 'var(--text3)' }}>{formatChinaDateTime(item.applied_at)}</td>
                      <td style={{ color: 'var(--text3)' }}>{formatChinaDateTime(item.rejected_at)}</td>
                      <td>{item.rejected_by ?? '-'}</td>
                      <td style={{ textAlign: 'right' }}>
                        {canReview ? <button className="action-btn action-btn-success" onClick={() => handleRestore(item)} disabled={submitting}>恢复通过</button> : null}
                      </td>
                    </tr>
                  );
                }) : null}
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
            <button
              key={target.key}
              type="button"
              className="player-context-menu-item"
              role="menuitem"
              onClick={() => openPlayerLink(target)}
            >
              {target.label}
            </button>
          ))}
        </div>
      ) : null}

      <Modal
        open={manualModalOpen}
        title="手动添加白名单"
        onClose={() => {
          setManualModalOpen(false);
          setManualForm(emptyManualForm);
          setManualError('');
        }}
        footer={(
          <>
            <button className="btn btn-outline" onClick={() => setManualModalOpen(false)}>取消</button>
            <button className="btn btn-primary" onClick={handleManualCreate} disabled={submitting}>添加</button>
          </>
        )}
      >
        <div className="form-group"><label>玩家名称</label><input type="text" className="form-control" value={manualForm.nickname} onChange={(event) => setManualForm((prev) => ({ ...prev, nickname: event.target.value }))} placeholder="玩家名称" /></div>
        <div className="form-group"><label>玩家标识</label><input type="text" className="form-control" value={manualForm.steam_input} onChange={(event) => setManualForm((prev) => ({ ...prev, steam_input: event.target.value }))} placeholder="SteamID64 / SteamID / Steam 个人主页链接" /></div>
        {manualError ? <div style={{ color: 'var(--accent)' }}>{manualError}</div> : null}
      </Modal>

      <Modal
        open={rejectModal.open}
        title="拒绝白名单申请"
        onClose={() => setRejectModal({ open: false, item: null, reason: '', error: '' })}
        footer={(
          <>
            <button className="btn btn-outline" onClick={() => setRejectModal({ open: false, item: null, reason: '', error: '' })}>取消</button>
            <button className="btn btn-primary" onClick={handleReject} disabled={submitting}>确认拒绝</button>
          </>
        )}
      >
        <div className="form-group"><label>拒绝理由</label><textarea className="form-control" rows={4} value={rejectModal.reason} onChange={(event) => setRejectModal((prev) => ({ ...prev, reason: event.target.value, error: '' }))} placeholder="请输入拒绝理由" /></div>
        {rejectModal.error ? <div style={{ color: 'var(--accent)' }}>{rejectModal.error}</div> : null}
      </Modal>

      <Modal
        open={approveModal.open}
        title={
          <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
            <span style={{ fontSize: 20, color: 'var(--accent)' }}>⚠</span>
            <span>通过白名单申请（全球封禁）</span>
          </div>
        }
        onClose={() => setApproveModal(emptyApproveModal())}
        footer={(
          <>
            <button className="btn btn-outline" onClick={() => setApproveModal(emptyApproveModal())}>取消</button>
            <button
              className="btn btn-primary"
              onClick={handleApproveWithReason}
              disabled={submitting || approveModal.secondsRemaining > 0}
            >
              {approveModal.secondsRemaining > 0 ? `${approveModal.secondsRemaining} 秒后可通过` : submitting ? '处理中...' : '确认通过'}
            </button>
          </>
        )}
      >
        <div className="global-ban-alert" style={{ marginBottom: 12 }}>
          <div className="global-ban-alert-icon">⚠</div>
          <div className="global-ban-alert-text">
            该玩家在全球 KZ 封禁库中有 <strong>{approveModal.bans.length}</strong> 条封禁记录。请完整查看下方封禁详情，倒计时结束并填写通过理由后才可正式通过。
          </div>
        </div>
        {approveGlobalBanRisk ? (
          <div className={`global-ban-risk global-ban-risk-${approveGlobalBanRisk.tone}`}>
            <div className="global-ban-risk-title">{approveGlobalBanRisk.title}</div>
            {approveGlobalBanRisk.reasons.length > 0 ? (
              <div className="global-ban-risk-reasons">
                {approveGlobalBanRisk.reasons.map((reason) => (
                  <span key={reason}>{reason}</span>
                ))}
              </div>
            ) : null}
          </div>
        ) : null}
        <div className="global-ban-info">
          <div><strong>玩家:</strong> {approveModal.item?.nickname ?? '-'}</div>
          <div><strong>SteamID64:</strong> <code>{approveModal.item?.steamid64 ?? '-'}</code></div>
        </div>
        <div style={{ marginBottom: 16 }}>
          <GlobalBanRecordList bans={approveModal.bans} />
        </div>
        <div className="form-group"><label>通过理由</label><textarea className="form-control" rows={4} value={approveModal.reason} onChange={(event) => setApproveModal((prev) => ({ ...prev, reason: event.target.value, error: '' }))} placeholder="请说明为什么在有全球封禁记录的情况下仍然通过" /></div>
        {approveModal.error ? <div style={{ color: 'var(--accent)' }}>{approveModal.error}</div> : null}
      </Modal>

      {/* 全球封禁详情弹窗 */}
      <Modal
        open={banDetailModal.open}
        title={
          <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
            <span style={{ fontSize: 20, color: 'var(--accent)' }}>⚠</span>
            <span>全球封禁记录</span>
          </div>
        }
        onClose={() => setBanDetailModal({ open: false, steamid64: '', bans: [] })}
        footer={<button className="btn btn-primary" onClick={() => setBanDetailModal({ open: false, steamid64: '', bans: [] })}>关闭</button>}
      >
        <div className="global-ban-detail">
          <div className="global-ban-alert">
            <div className="global-ban-alert-icon">⚠</div>
            <div className="global-ban-alert-text">
              该玩家在全球 KZ 封禁库中有 <strong>{banDetailModal.bans.length}</strong> 条封禁记录，请谨慎审核！
            </div>
          </div>
          <div className="global-ban-info">
            <div><strong>SteamID64:</strong> <code>{banDetailModal.steamid64}</code></div>
          </div>
          {banDetailModal.bans.length > 0 ? (
            <div className="global-ban-list">
              {banDetailModal.bans.map((ban, index) => (
                <div key={index} className="global-ban-item">
                  <div className="global-ban-item-header">
                    <span className="global-ban-type">{ban.ban_type || '作弊'}</span>
                    {ban.expires_on ? (
                      <span className="global-ban-temporary">临时</span>
                    ) : (
                      <span className="global-ban-permanent">永久</span>
                    )}
                  </div>
                  <div className="global-ban-item-body">
                    {ban.player_name && (
                      <div className="global-ban-field">
                        <span className="global-ban-label">玩家</span>
                        <span className="global-ban-value">{ban.player_name}</span>
                      </div>
                    )}
                    {ban.notes && (
                      <div className="global-ban-field">
                        <span className="global-ban-label">备注</span>
                        <span className="global-ban-value">{ban.notes}</span>
                      </div>
                    )}
                    {ban.stats && (
                      <div className="global-ban-field">
                        <span className="global-ban-label">统计</span>
                        <span className="global-ban-value global-ban-stats">{ban.stats}</span>
                      </div>
                    )}
                    {ban.created_on && (
                      <div className="global-ban-field">
                        <span className="global-ban-label">封禁时间</span>
                        <span className="global-ban-value">{formatChinaDateTime(ban.created_on)}</span>
                      </div>
                    )}
                    {ban.expires_on && (
                      <div className="global-ban-field">
                        <span className="global-ban-label">到期时间</span>
                        <span className="global-ban-value">{formatChinaDateTime(ban.expires_on)}</span>
                      </div>
                    )}
                    {ban.server_name && (
                      <div className="global-ban-field">
                        <span className="global-ban-label">服务器</span>
                        <span className="global-ban-value">{ban.server_name}</span>
                      </div>
                    )}
                  </div>
                </div>
              ))}
            </div>
          ) : (
            <div className="global-ban-empty">暂无封禁记录</div>
          )}
        </div>
      </Modal>

      {dialog}
      <ToastContainer toasts={toasts} onDismiss={dismissToast} />
    </div>
  );
}

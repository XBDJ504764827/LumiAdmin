import React, { useCallback, useEffect, useRef, useState } from 'react';
import { api } from '../../lib/api.js';
import { useConfirmDialog } from '../../shared/ConfirmModal.jsx';
import { Modal } from '../../shared/Modal.jsx';
import { useAuth } from '../../state/auth.jsx';
import { useToast, ToastContainer } from '../../shared/Toast.jsx';
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

// 批量查询全球封禁记录（通过后端代理）
async function fetchGlobalBansBatch(steamids) {
  try {
    const response = await fetch('/api/public/global-bans/batch', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ steamids }),
    });
    if (!response.ok) return {};
    const data = await response.json();
    return data.results || {};
  } catch {
    return {};
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

const STATUS_MAP = { pending: 'pending', approved: 'approved', rejected: 'rejected' };

export function WhitelistPage() {
  const { session } = useAuth();
  const { confirm, dialog } = useConfirmDialog();
  const { toast, toasts, dismiss: dismissToast } = useToast();
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
  const [approveModal, setApproveModal] = useState({ open: false, item: null, reason: '', error: '' });
  const [manualForm, setManualForm] = useState(emptyManualForm);
  const [manualError, setManualError] = useState('');
  const [submitting, setSubmitting] = useState(false);
  const [globalBansVersion, setGlobalBansVersion] = useState(0); // 递增触发渲染
  const globalBansRef = useRef({}); // steamid64 -> ban records
  const [globalBansLoading, setGlobalBansLoading] = useState(false); // 全球封禁加载状态
  const fetchedSteamIdsRef = useRef(new Set()); // 已查询过的 steamid64
  const [banDetailModal, setBanDetailModal] = useState({ open: false, steamid64: '', bans: [] });
  const [refreshing, setRefreshing] = useState(false); // Steam名称刷新状态

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
    try { localStorage.setItem('whitelist_tab', tab); } catch {}
  }, [tab]);

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
      setApproveModal({ open: true, item, reason: '', error: '' });
      return;
    }
    try {
      setSubmitting(true);
      await api.approveWhitelist(token, item.id);
      await loadItems();
      toast({ title: '审核通过', message: `${item.nickname} 的白名单申请已通过。` });
    } catch (actionError) {
      toast({ title: '操作失败', message: actionError.message, tone: 'danger' });
    } finally {
      setSubmitting(false);
    }
  }

  async function handleApproveWithReason() {
    if (!approveModal.item) return;
    if (!approveModal.reason.trim()) {
      setApproveModal((prev) => ({ ...prev, error: '该玩家有全球封禁记录，请说明通过理由。' }));
      return;
    }

    try {
      setSubmitting(true);
      await api.approveWhitelist(token, approveModal.item.id, {
        reason: approveModal.reason.trim(),
      });
      setApproveModal({ open: false, item: null, reason: '', error: '' });
      await loadItems();
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
        <button className={`tab ${tab === 'pending' ? 'active' : ''}`} onClick={() => switchTab('pending')}>待审核</button>
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
                        <td style={{ color: 'var(--text3)' }}>{formatDateTime(item.applied_at)}</td>
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
                      <td style={{ color: 'var(--text3)' }}>{formatDateTime(item.applied_at)}</td>
                      <td style={{ color: 'var(--text3)' }}>{formatDateTime(item.approved_at)}</td>
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
                      <td style={{ color: 'var(--text3)' }}>{formatDateTime(item.applied_at)}</td>
                      <td style={{ color: 'var(--text3)' }}>{formatDateTime(item.rejected_at)}</td>
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
        onClose={() => setApproveModal({ open: false, item: null, reason: '', error: '' })}
        footer={(
          <>
            <button className="btn btn-outline" onClick={() => setApproveModal({ open: false, item: null, reason: '', error: '' })}>取消</button>
            <button className="btn btn-primary" onClick={handleApproveWithReason} disabled={submitting}>确认通过</button>
          </>
        )}
      >
        <div className="global-ban-alert" style={{ marginBottom: 12 }}>
          <div className="global-ban-alert-icon">⚠</div>
          <div className="global-ban-alert-text">
            该玩家在全球 KZ 封禁库中有封禁记录，请说明通过理由。
          </div>
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
                        <span className="global-ban-value">{formatDateTime(ban.created_on)}</span>
                      </div>
                    )}
                    {ban.expires_on && (
                      <div className="global-ban-field">
                        <span className="global-ban-label">到期时间</span>
                        <span className="global-ban-value">{formatDateTime(ban.expires_on)}</span>
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

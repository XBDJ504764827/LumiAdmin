import React, { useCallback, useState } from 'react';
import { api } from '../../lib/api.js';
import { useAuth } from '../../state/auth.jsx';
import { useToast, ToastContainer } from '../../shared/Toast.jsx';
import { useConfirmDialog } from '../../shared/ConfirmModal.jsx';
import { Modal } from '../../shared/Modal.jsx';
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
    return `${y}-${m}-${d} ${h}:${min}`;
  } catch {
    return isoString;
  }
}

// 解析全球封禁数据
function parseBanData(data) {
  if (Array.isArray(data) && data.length > 0) return data;
  if (data && Array.isArray(data.data) && data.data.length > 0) return data.data;
  return [];
}

// 查询单个玩家的全球封禁记录
async function fetchGlobalBans(steamid64) {
  try {
    const response = await fetch(`/api/public/global-bans/${encodeURIComponent(steamid64)}`);
    if (!response.ok) return [];
    const data = await response.json();
    return parseBanData(data);
  } catch {
    return [];
  }
}

const STATUS_MAP = {
  pending: { label: '待审核', pill: 'pill-warning' },
  approved: { label: '已通过', pill: 'pill-success' },
  rejected: { label: '已驳回', pill: 'pill-offline' },
};

export function BanAppealPage() {
  const { session } = useAuth();
  const { confirm, dialog } = useConfirmDialog();
  const { toast, toasts, dismiss: dismissToast } = useToast();
  const token = session?.token ?? null;

  const [search, setSearch] = useState('');
  const [status, setStatus] = useState(undefined);
  const [page, setPage] = useState(1);
  const [data, setData] = useState(null);
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState('');

  // 审核弹窗
  const [reviewOpen, setReviewOpen] = useState(false);
  const [reviewMode, setReviewMode] = useState('approve');
  const [reviewItem, setReviewItem] = useState(null);
  const [reviewNote, setReviewNote] = useState('');
  const [submitting, setSubmitting] = useState(false);

  // 详情弹窗
  const [detailOpen, setDetailOpen] = useState(false);
  const [detailItem, setDetailItem] = useState(null);
  const [globalBans, setGlobalBans] = useState([]);
  const [globalBansLoading, setGlobalBansLoading] = useState(false);

  const canReview = session?.role === 'developer' || session?.role === 'admin';

  const loadItems = useCallback(async () => {
    try {
      setLoading(true);
      setLoadError('');
      const params = { page, page_size: 20 };
      if (search) params.search = search;
      if (status) params.status = status;
      const result = await api.banAppeals(token, params);
      setData(result);
    } catch {
      setData(null);
      setLoadError('加载申诉数据失败，请稍后重试');
    } finally {
      setLoading(false);
    }
  }, [token, page, search, status]);

  React.useEffect(() => { loadItems(); }, [loadItems]);

  function refresh() {
    loadItems();
  }

  async function openDetail(item) {
    setDetailItem(item);
    setDetailOpen(true);
    setGlobalBans([]);
    setGlobalBansLoading(true);
    const bans = await fetchGlobalBans(item.steam_id);
    setGlobalBans(bans);
    setGlobalBansLoading(false);
  }

  function openReview(item, mode) {
    setReviewItem(item);
    setReviewMode(mode);
    setReviewNote('');
    setReviewOpen(true);
  }

  async function handleReview() {
    if (!reviewItem) return;
    const action = reviewMode === 'approve' ? '通过' : '驳回';
    const confirmed = await confirm({
      title: `${action}封禁申诉`,
      message: `确定${action}玩家 ${reviewItem.player_name} 的封禁申诉吗？${reviewMode === 'approve' ? '通过后该封禁将被解除。' : ''}`,
    });
    if (!confirmed) return;

    try {
      setSubmitting(true);
      const body = reviewNote.trim() ? { review_note: reviewNote.trim() } : {};
      if (reviewMode === 'approve') {
        await api.approveBanAppeal(token, reviewItem.id, body);
      } else {
        await api.rejectBanAppeal(token, reviewItem.id, body);
      }
      setReviewOpen(false);
      setReviewItem(null);
      refresh();
      toast({
        title: `${action}成功`,
        message: reviewMode === 'approve'
          ? `已通过 ${reviewItem.player_name} 的申诉并解除封禁。`
          : `已驳回 ${reviewItem.player_name} 的申诉。`,
      });
    } catch (e) {
      toast({ title: `${action}失败`, message: e.message, tone: 'danger' });
    } finally {
      setSubmitting(false);
    }
  }

  const items = data?.items ?? [];
  const total = data?.total ?? 0;

  return (
    <div id="ban-appeal" className="content-section active">
      <div className="breadcrumb"><span>核心管理</span><span className="sep">›</span><span className="current">封禁申诉</span></div>
      <div className="page-header">
        <div>
          <div className="page-title">封禁申诉管理</div>
          <div className="page-sub">审核玩家提交的封禁申诉，通过后将自动解除对应封禁。</div>
        </div>
      </div>

      <SearchBar
        value={search}
        onChange={(v) => { setSearch(v); setPage(1); }}
        placeholder="搜索 SteamID / 玩家名称..."
        statusOptions={[
          { value: 'pending', label: '待审核' },
          { value: 'approved', label: '已通过' },
          { value: 'rejected', label: '已驳回' },
        ]}
        statusValue={status}
        onStatusChange={(v) => { setStatus(v); setPage(1); }}
      />

      <div className="card">
        <div className="card-body" style={{ padding: 0 }}>
          <div className="table-responsive">
            <table className="data-table">
              <thead>
                <tr>
                  <th>玩家名称</th>
                  <th>SteamID64</th>
                  <th>申诉理由</th>
                  <th>封禁原因</th>
                  <th>状态</th>
                  <th>申诉时间</th>
                  <th>审核人</th>
                  <th>审核时间</th>
                  <th style={{ textAlign: 'right' }}>操作</th>
                </tr>
              </thead>
              <tbody>
                {loading ? (
                  <tr><td colSpan={9} style={{ textAlign: 'center', color: 'var(--text2)' }}>正在加载申诉数据...</td></tr>
                ) : null}
                {!loading && loadError ? (
                  <tr><td colSpan={9} style={{ textAlign: 'center', color: 'var(--danger)' }}>{loadError}</td></tr>
                ) : null}
                {!loading && items.map((item) => {
                  const st = STATUS_MAP[item.status] || { label: item.status, pill: '' };
                  return (
                    <tr key={item.id}>
                      <td style={{ fontWeight: 600 }}>{item.player_name}</td>
                      <td className="steam-id">{item.steam_id}</td>
                      <td style={{ color: 'var(--text2)', maxWidth: 200, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                        <span title={item.appeal_reason}>{item.appeal_reason}</span>
                      </td>
                      <td style={{ color: 'var(--text2)', maxWidth: 160, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                        <span title={item.ban_reason}>{item.ban_reason || '-'}</span>
                      </td>
                      <td><span className={`status-pill ${st.pill}`}>{st.label}</span></td>
                      <td style={{ color: 'var(--text3)' }}>{formatDateTime(item.created_at)}</td>
                      <td>{item.reviewed_by || '-'}</td>
                      <td style={{ color: 'var(--text3)' }}>{item.reviewed_at ? formatDateTime(item.reviewed_at) : '-'}</td>
                      <td style={{ textAlign: 'right' }}>
                        <div className="action-btn-group">
                          <button className="action-btn action-btn-accent" onClick={() => openDetail(item)}>详情</button>
                          {canReview && item.status === 'pending' ? (
                            <>
                              <button className="action-btn action-btn-success" onClick={() => openReview(item, 'approve')}>通过</button>
                              <button className="action-btn action-btn-danger" onClick={() => openReview(item, 'reject')}>驳回</button>
                            </>
                          ) : null}
                        </div>
                      </td>
                    </tr>
                  );
                })}
                {!loading && items.length === 0 ? (
                  <tr><td colSpan={9} style={{ textAlign: 'center', color: 'var(--text2)' }}>暂无申诉记录</td></tr>
                ) : null}
              </tbody>
            </table>
          </div>
        </div>
      </div>

      <Pagination page={page} pageSize={20} total={total} onChange={setPage} />

      {/* 详情弹窗 */}
      <Modal
        open={detailOpen}
        title="申诉详情"
        onClose={() => { setDetailOpen(false); setDetailItem(null); setGlobalBans([]); }}
        footer={<button className="btn btn-outline" onClick={() => { setDetailOpen(false); setDetailItem(null); setGlobalBans([]); }}>关闭</button>}
      >
        {detailItem ? (
          <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
            <div className="form-group">
              <label style={{ marginBottom: 4 }}>玩家信息</label>
              <div style={{ color: 'var(--text2)', fontSize: 13 }}>
                <div>名称：{detailItem.player_name}</div>
                <div>SteamID64：{detailItem.steam_id}</div>
              </div>
            </div>
            <div className="form-group">
              <label style={{ marginBottom: 4 }}>封禁信息</label>
              <div style={{ color: 'var(--text2)', fontSize: 13 }}>
                <div>封禁原因：{detailItem.ban_reason || '-'}</div>
                <div>封禁类型：{detailItem.ban_type === 'steam' ? 'Steam 封禁' : detailItem.ban_type === 'ip' ? 'IP 封禁' : detailItem.ban_type || '-'}</div>
                <div>操作人：{detailItem.ban_operator_name || '-'}</div>
                <div>服务器：{detailItem.ban_server_name || '-'}</div>
              </div>
            </div>
            <div className="form-group">
              <label style={{ marginBottom: 4 }}>申诉理由</label>
              <div style={{ color: 'var(--text2)', fontSize: 13, whiteSpace: 'pre-wrap', background: 'var(--surface2)', padding: 8, borderRadius: 6 }}>
                {detailItem.appeal_reason}
              </div>
            </div>
            <div className="form-group">
              <label style={{ marginBottom: 4 }}>处理状态</label>
              <div>
                <span className={`status-pill ${STATUS_MAP[detailItem.status]?.pill || ''}`}>
                  {STATUS_MAP[detailItem.status]?.label || detailItem.status}
                </span>
              </div>
              {detailItem.reviewed_by ? (
                <div style={{ color: 'var(--text3)', fontSize: 12, marginTop: 4 }}>
                  由 {detailItem.reviewed_by} 于 {formatDateTime(detailItem.reviewed_at)} 处理
                </div>
              ) : null}
              {detailItem.review_note ? (
                <div style={{ color: 'var(--text2)', fontSize: 13, marginTop: 4, whiteSpace: 'pre-wrap', background: 'var(--surface2)', padding: 8, borderRadius: 6 }}>
                  审核备注：{detailItem.review_note}
                </div>
              ) : null}
            </div>

            {/* 全球封禁记录 */}
            <div className="form-group">
              <label style={{ marginBottom: 4 }}>全球封禁记录</label>
              {globalBansLoading ? (
                <div style={{ color: 'var(--text3)', fontSize: 13, padding: '8px 0' }}>正在查询全球封禁记录...</div>
              ) : globalBans.length > 0 ? (
                <div className="global-ban-detail" style={{ marginTop: 4 }}>
                  <div className="global-ban-alert">
                    <div className="global-ban-alert-icon">⚠</div>
                    <div className="global-ban-alert-text">
                      该玩家在全球 KZ 封禁库中有 <strong>{globalBans.length}</strong> 条封禁记录，请谨慎审核！
                    </div>
                  </div>
                  <div className="global-ban-list">
                    {globalBans.map((ban, index) => (
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
                </div>
              ) : (
                <div style={{ color: 'var(--text3)', fontSize: 13, padding: '8px 0' }}>未查询到全球封禁记录。</div>
              )}
            </div>
          </div>
        ) : null}
      </Modal>

      {/* 审核弹窗 */}
      <Modal
        open={reviewOpen}
        title={reviewMode === 'approve' ? '通过封禁申诉' : '驳回封禁申诉'}
        onClose={() => { setReviewOpen(false); setReviewItem(null); }}
        footer={
          <>
            <button className="btn btn-outline" onClick={() => { setReviewOpen(false); setReviewItem(null); }}>取消</button>
            <button
              className={`btn ${reviewMode === 'approve' ? 'btn-success' : 'btn-danger'}`}
              onClick={handleReview}
              disabled={submitting}
            >
              {submitting ? '处理中...' : reviewMode === 'approve' ? '确认通过并解封' : '确认驳回'}
            </button>
          </>
        }
      >
        {reviewItem ? (
          <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
            <div style={{ color: 'var(--text2)', fontSize: 13 }}>
              <div><strong>玩家：</strong>{reviewItem.player_name}（{reviewItem.steam_id}）</div>
              <div><strong>封禁原因：</strong>{reviewItem.ban_reason || '-'}</div>
              <div><strong>申诉理由：</strong>{reviewItem.appeal_reason}</div>
            </div>
            {reviewMode === 'approve' && (
              <div style={{ color: 'var(--warning-text)', fontSize: 13, background: 'var(--warning-bg)', padding: 8, borderRadius: 6 }}>
                通过申诉后将自动解除该玩家的封禁记录。
              </div>
            )}
            <div className="form-group">
              <label>审核备注</label>
              <textarea
                className="form-control"
                value={reviewNote}
                onChange={(e) => setReviewNote(e.target.value)}
                placeholder={reviewMode === 'approve' ? '可选，填写通过申诉的原因...' : '可选，填写驳回申诉的原因...'}
                rows={3}
                style={{ resize: 'vertical' }}
              />
            </div>
          </div>
        ) : null}
      </Modal>

      {dialog}
      <ToastContainer toasts={toasts} onDismiss={dismissToast} />
    </div>
  );
}

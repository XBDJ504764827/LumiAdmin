import { useRef, useState } from 'react';
import { api } from '../../lib/api.js';
import { useApiQuery } from '../../shared/useApiQuery.js';
import { useAuth } from '../../state/auth.jsx';
import { useToast, ToastContainer } from '../../shared/Toast.jsx';
import { useConfirmDialog } from '../../shared/ConfirmModal.jsx';
import { InternalNoteBadge, InternalNoteInline } from '../../shared/InternalNote.jsx';
import { Modal } from '../../shared/Modal.jsx';
import { SearchBar } from '../../shared/SearchBar.jsx';
import { Pagination } from '../../shared/Pagination.jsx';
import { StatusPill } from '../../shared/StatusPill.jsx';
import { FileItem, fileActionLabel } from '../../shared/FilePreview.jsx';
import { formatChinaDateTime } from '../../shared/time.js';
import { notifyPendingReviewsUpdated, usePendingReviewIndicators } from '../../hooks/usePendingReviewIndicators.js';

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
  pending: { label: '待审核', pill: 'warning' },
  approved: { label: '已通过', pill: 'success' },
  rejected: { label: '已驳回', pill: 'offline' },
};
const STATUS_FILTERS = [
  { value: undefined, label: '全部状态' },
  { value: 'pending', label: '待审核' },
  { value: 'approved', label: '已通过' },
  { value: 'rejected', label: '已驳回' },
];

export function BanAppealPage() {
  const { session } = useAuth();
  const { confirm, dialog } = useConfirmDialog();
  const { toast, toasts, dismiss: dismissToast } = useToast();
  const { counts: pendingCounts } = usePendingReviewIndicators();
  const token = session?.token ?? null;

  const [search, setSearch] = useState('');
  const [status, setStatus] = useState(undefined);
  const [page, setPage] = useState(1);

  const { data, isLoading, error: loadError, refetch } = useApiQuery(
    ['banAppeals', { page, search, status }],
    (token) => api.banAppeals(token, { page, page_size: 20, ...(search ? { search } : {}), ...(status ? { status } : {}) }),
  );

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
  const [appealFiles, setAppealFiles] = useState([]);
  const [appealFilesLoading, setAppealFilesLoading] = useState(false);

  const canReview = session?.role === 'developer' || session?.role === 'admin';
  const reviewFileRef = useRef(null);
  const [reviewFiles, setReviewFiles] = useState([]);

  function refresh() {
    refetch();
  }

  async function openDetail(item) {
    setDetailItem(item);
    setDetailOpen(true);
    setGlobalBans([]);
    setGlobalBansLoading(true);
    setAppealFiles([]);
    setAppealFilesLoading(true);

    const bans = await fetchGlobalBans(item.steam_id);
    setGlobalBans(bans);
    setGlobalBansLoading(false);

    try {
      const filesData = await api.listAdminAppealFiles(token, item.id);
      setAppealFiles(filesData.files ?? []);
    } catch {
      setAppealFiles([]);
    } finally {
      setAppealFilesLoading(false);
    }
  }

  function openReview(item, mode) {
    setReviewItem(item);
    setReviewMode(mode);
    setReviewNote('');
    setReviewFiles([]);
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

      // Upload files first if any
      if (reviewFiles.length > 0) {
        const formData = new FormData();
        for (const f of reviewFiles) formData.append('files', f, f.name);
        try {
          await api.uploadAdminAppealFiles(token, reviewItem.id, formData);
        } catch (e) {
          toast({ title: '文件上传失败', message: e.message, tone: 'danger' });
          setSubmitting(false);
          return;
        }
      }

      const body = reviewNote.trim() ? { review_note: reviewNote.trim() } : {};
      if (reviewMode === 'approve') {
        await api.approveBanAppeal(token, reviewItem.id, body);
      } else {
        await api.rejectBanAppeal(token, reviewItem.id, body);
      }
      setReviewOpen(false);
      setReviewItem(null);
      setReviewFiles([]);
      refresh();
      notifyPendingReviewsUpdated({ source: 'banAppeal', action: reviewMode });
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
  const hasPendingAppeals = (pendingCounts.banAppeal ?? 0) > 0;

  return (
    <div id="ban-appeal" className="content-section active">
      <div className="breadcrumb"><span>核心管理</span><span className="sep">›</span><span className="current">封禁申诉</span></div>
      <div className="page-header">
        <div>
          <div className="page-title">封禁申诉管理</div>
          <div className="page-sub">审核玩家提交的封禁申诉，通过后将自动解除对应封禁。</div>
        </div>
      </div>

      <div className="tabs">
        {STATUS_FILTERS.map((filter) => (
          <button
            key={filter.value ?? 'all'}
            className={`tab ${status === filter.value ? 'active' : ''}`}
            onClick={() => { setStatus(filter.value); setPage(1); }}
          >
            <span>{filter.label}</span>
            {filter.value === 'pending' && hasPendingAppeals ? (
              <span className="tab-pending-dot" title={`有 ${pendingCounts.banAppeal} 条待审核封禁申诉`} />
            ) : null}
          </button>
        ))}
      </div>

      <SearchBar
        value={search}
        onChange={(v) => { setSearch(v); setPage(1); }}
        placeholder="搜索 SteamID / 玩家名称..."
        aria-label="搜索封禁申诉"
      />

      <div className="card">
        <div className="card-body p-0">
          <div className="table-responsive">
            <table className="data-table" role="table" aria-label="封禁申诉列表">
              <thead>
                <tr>
                  <th scope="col">玩家名称</th>
                  <th scope="col">SteamID64</th>
                  <th scope="col">申诉理由</th>
                  <th scope="col">封禁原因</th>
                  <th scope="col">状态</th>
                  <th scope="col">申诉时间</th>
                  <th scope="col">审核人</th>
                  <th scope="col">审核时间</th>
                  <th scope="col" className="text-right">操作</th>
                </tr>
              </thead>
              <tbody>
                {isLoading ? (
                  <tr><td colSpan={9} className="text-center text-muted">正在加载申诉数据...</td></tr>
                ) : null}
                {!isLoading && loadError ? (
                  <tr><td colSpan={9} className="text-center text-danger">{loadError.message}</td></tr>
                ) : null}
                {!isLoading && items.map((item) => {
                  const st = STATUS_MAP[item.status] || { label: item.status, pill: '' };
                  return (
                    <tr key={item.id}>
                      <td className="fw-600">
                        {item.player_name}
                        <InternalNoteInline steamid64={item.steam_id} />
                      </td>
                      <td className="steam-id">{item.steam_id}</td>
                      <td className="text-muted text-ellipsis" style={{ maxWidth: 200 }}>
                        <span title={item.appeal_reason}>{item.appeal_reason}</span>
                      </td>
                      <td className="text-muted text-ellipsis" style={{ maxWidth: 160 }}>
                        <span title={item.ban_reason}>{item.ban_reason || '-'}</span>
                      </td>
                      <td><StatusPill kind={st.pill}>{st.label}</StatusPill></td>
                      <td className="text-muted-light">{formatChinaDateTime(item.created_at, { seconds: false })}</td>
                      <td>{item.reviewed_by || '-'}</td>
                      <td className="text-muted-light">{item.reviewed_at ? formatChinaDateTime(item.reviewed_at, { seconds: false }) : '-'}</td>
                      <td className="text-right">
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
                {!isLoading && items.length === 0 ? (
                  <tr><td colSpan={9} className="text-center text-muted">暂无申诉记录</td></tr>
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
          <div className="flex-col gap-12">
            <div className="detail-field">
              <div className="detail-field-label">玩家信息</div>
              <div className="detail-field-value">
                <div>名称：{detailItem.player_name}</div>
                <div>SteamID64：{detailItem.steam_id}</div>
              </div>
            </div>
            <div className="detail-field">
              <div className="detail-field-label">封禁信息</div>
              <div className="detail-field-value">
                <div>封禁原因：{detailItem.ban_reason || '-'}</div>
                <div>封禁类型：{detailItem.ban_type === 'steam' ? 'Steam 封禁' : detailItem.ban_type === 'ip' ? 'IP 封禁' : detailItem.ban_type || '-'}</div>
                <div>操作人：{detailItem.ban_operator_name || '-'}</div>
                <div>服务器：{detailItem.ban_server_name || '-'}</div>
              </div>
            </div>
            <div className="detail-field">
              <div className="detail-field-label">申诉理由</div>
              <div className="detail-field-value-block">{detailItem.appeal_reason}</div>
            </div>
            <div className="detail-field">
              <div className="detail-field-label">处理状态</div>
              <div>
                <StatusPill kind={STATUS_MAP[detailItem.status]?.pill || ''}>
                  {STATUS_MAP[detailItem.status]?.label || detailItem.status}
                </StatusPill>
              </div>
              {detailItem.reviewed_by ? (
                <div className="text-muted-light fs-12 mt-4">
                  由 {detailItem.reviewed_by} 于 {formatChinaDateTime(detailItem.reviewed_at, { seconds: false })} 处理
                </div>
              ) : null}
              {detailItem.review_note ? (
                <div className="review-note">审核备注：{detailItem.review_note}</div>
              ) : null}
            </div>

            {/* 全球封禁记录 */}
            <div className="detail-field">
              <div className="detail-field-label">全球封禁记录</div>
              {globalBansLoading ? (
                <div className="text-muted-light fs-13 p-8">正在查询全球封禁记录...</div>
              ) : globalBans.length > 0 ? (
                <div className="global-ban-detail mt-4">
                  <div className="global-ban-alert">
                    <div className="global-ban-alert-icon" aria-hidden="true">⚠</div>
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
                              <span className="global-ban-value">{formatChinaDateTime(ban.created_on, { seconds: false })}</span>
                            </div>
                          )}
                          {ban.expires_on && (
                            <div className="global-ban-field">
                              <span className="global-ban-label">到期时间</span>
                              <span className="global-ban-value">{formatChinaDateTime(ban.expires_on, { seconds: false })}</span>
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
                <div className="text-muted-light fs-13 p-8">未查询到全球封禁记录。</div>
              )}
            </div>

            {/* 申诉辅助文件 */}
            <div className="detail-field">
              <div className="detail-field-label">辅助文件</div>
              {appealFilesLoading ? (
                <div className="text-muted-light fs-13 p-8">正在加载文件列表...</div>
              ) : appealFiles.length > 0 ? (
                <div className="flex-col gap-10">
                  {appealFiles.map((file) => (
                    <FileItem key={file.id} file={file}>
                      <div style={{ display: 'flex', alignItems: 'center', gap: 6, flexShrink: 0 }}>
                        {file.uploaded_by ? (
                          <span className="status-pill pill-idle" style={{ fontSize: 11 }}>{file.uploaded_by}</span>
                        ) : null}
                        {file.url ? (
                          <a
                            href={file.url}
                            target="_blank"
                            rel="noopener noreferrer"
                            className="action-btn flex-shrink-0"
                            aria-label={`${fileActionLabel(file.category)} ${file.file_name}`}
                          >
                            {fileActionLabel(file.category)}
                          </a>
                        ) : (
                          <span className="text-muted-light fs-11 flex-shrink-0">不可用</span>
                        )}
                      </div>
                    </FileItem>
                  ))}
                </div>
              ) : (
                <div className="text-muted-light fs-13 p-8">该申诉未上传辅助文件。</div>
              )}
            </div>
            <InternalNoteBadge steamid64={detailItem?.steam_id} />
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
          <div className="flex-col gap-12">
            <div className="text-muted fs-13">
              <div><strong>玩家：</strong>{reviewItem.player_name}（{reviewItem.steam_id}）</div>
              <div><strong>封禁原因：</strong>{reviewItem.ban_reason || '-'}</div>
              <div><strong>申诉理由：</strong>{reviewItem.appeal_reason}</div>
            </div>
            {reviewMode === 'approve' && (
              <div className="bg-warning text-warning fs-13 rounded-md" style={{ padding: 8 }}>
                通过申诉后将自动解除该玩家的封禁记录。
              </div>
            )}
            <div className="form-group">
              <label htmlFor="review-note">审核备注</label>
              <textarea
                id="review-note"
                className="form-control"
                value={reviewNote}
                onChange={(e) => setReviewNote(e.target.value)}
                placeholder={reviewMode === 'approve' ? '可选，填写通过申诉的原因...' : '可选，填写驳回申诉的原因...'}
                rows={3}
                style={{ resize: 'vertical' }}
              />
            </div>
            <div className="form-group">
              <label>证据文件（选填）</label>
              <div className="form-hint" style={{ marginBottom: 8 }}>可上传截图、录像或录音作为审核依据。</div>
              <input
                ref={reviewFileRef}
                type="file"
                accept=".mp4,.avi,.mov,.webm,.mkv,.jpg,.jpeg,.png,.gif,.webp,.bmp,.mp3,.wav,.ogg,.m4a,.flac"
                multiple
                style={{ display: 'none' }}
                onChange={(e) => {
                  const selected = Array.from(e.target.files || []);
                  if (selected.length) setReviewFiles((prev) => [...prev, ...selected]);
                  if (reviewFileRef.current) reviewFileRef.current.value = '';
                }}
              />
              <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap', marginBottom: reviewFiles.length > 0 ? 10 : 0 }}>
                <button type="button" className="btn btn-outline btn-sm fs-12" onClick={() => reviewFileRef.current?.click()} disabled={submitting}>
                  🖼 选择图片
                </button>
                <button type="button" className="btn btn-outline btn-sm fs-12" onClick={() => { reviewFileRef.current.accept = '.mp4,.avi,.mov,.webm,.mkv'; reviewFileRef.current?.click(); reviewFileRef.current.accept = '.mp4,.avi,.mov,.webm,.mkv,.jpg,.jpeg,.png,.gif,.webp,.bmp,.mp3,.wav,.ogg,.m4a,.flac'; }} disabled={submitting}>
                  🎬 选择录像
                </button>
                <button type="button" className="btn btn-outline btn-sm fs-12" onClick={() => { reviewFileRef.current.accept = '.mp3,.wav,.ogg,.m4a,.flac'; reviewFileRef.current?.click(); reviewFileRef.current.accept = '.mp4,.avi,.mov,.webm,.mkv,.jpg,.jpeg,.png,.gif,.webp,.bmp,.mp3,.wav,.ogg,.m4a,.flac'; }} disabled={submitting}>
                  🎵 选择录音
                </button>
              </div>
              {reviewFiles.length > 0 && (
                <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
                  {reviewFiles.map((file, idx) => (
                    <div key={`${file.name}-${idx}`} style={{
                      display: 'flex', alignItems: 'center', justifyContent: 'space-between',
                      padding: '8px 10px', background: 'var(--surface2)', borderRadius: 6, fontSize: 13,
                    }}>
                      <div style={{ display: 'flex', alignItems: 'center', gap: 8, minWidth: 0, flex: 1 }}>
                        <span>{file.name.match(/\.(mp4|avi|mov|webm|mkv)$/i) ? '🎬' : file.name.match(/\.(mp3|wav|ogg|m4a|flac)$/i) ? '🎵' : '🖼'}</span>
                        <span style={{ overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{file.name}</span>
                        <span style={{ color: 'var(--text3)', fontSize: 11, flexShrink: 0 }}>{(file.size / 1024 / 1024).toFixed(1)} MB</span>
                      </div>
                      <button type="button" onClick={() => setReviewFiles((prev) => prev.filter((_, i) => i !== idx))} style={{ background: 'none', border: 'none', color: 'var(--text3)', cursor: 'pointer', fontSize: 14, padding: '0 4px' }}>✕</button>
                    </div>
                  ))}
                </div>
              )}
            </div>
          </div>
        ) : null}
      </Modal>

      {dialog}
      <ToastContainer toasts={toasts} onDismiss={dismissToast} />
    </div>
  );
}

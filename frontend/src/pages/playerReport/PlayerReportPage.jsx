import React, { useCallback, useState } from 'react';
import { api } from '../../lib/api.js';
import { useAuth } from '../../state/auth.jsx';
import { useToast, ToastContainer } from '../../shared/Toast.jsx';
import { useConfirmDialog } from '../../shared/ConfirmModal.jsx';
import { Modal } from '../../shared/Modal.jsx';
import { SearchBar } from '../../shared/SearchBar.jsx';
import { Pagination } from '../../shared/Pagination.jsx';

const STATUS_MAP = {
  pending: { label: '待审核', pill: 'pill-warning' },
  approved: { label: '已通过', pill: 'pill-success' },
  rejected: { label: '已驳回', pill: 'pill-offline' },
};

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

function formatFileSize(bytes) {
  const value = Number(bytes);
  if (!Number.isFinite(value)) return '-';
  if (value < 1024) return `${value} B`;
  if (value < 1024 * 1024) return `${(value / 1024).toFixed(1)} KB`;
  return `${(value / 1024 / 1024).toFixed(1)} MB`;
}

function fileTypeLabel(category) {
  if (category === 'video') return '录像';
  if (category === 'image') return '图片';
  if (category === 'audio') return '录音';
  return '文件';
}

function fileActionLabel(category) {
  if (category === 'image') return '打开原图';
  if (category === 'video') return '播放录像';
  if (category === 'audio') return '播放录音';
  return '下载文件';
}

function renderFilePreview(file) {
  if (!file.url) return null;

  if (file.category === 'video') {
    return (
      <video
        src={file.url}
        controls
        preload="metadata"
        style={{ width: '100%', maxHeight: 360, marginTop: 10, borderRadius: 8, background: '#000' }}
      >
        当前浏览器不支持播放该视频，请下载原文件查看。
      </video>
    );
  }

  if (file.category === 'audio') {
    return (
      <audio src={file.url} controls preload="metadata" style={{ width: '100%', marginTop: 10 }}>
        当前浏览器不支持播放该音频，请下载原文件查看。
      </audio>
    );
  }

  if (file.category === 'image') {
    return (
      <a href={file.url} target="_blank" rel="noopener noreferrer" style={{ display: 'block', marginTop: 10 }}>
        <img
          src={file.url}
          alt={file.file_name}
          loading="lazy"
          style={{
            display: 'block',
            width: '100%',
            maxHeight: 360,
            objectFit: 'contain',
            borderRadius: 8,
            background: 'var(--surface1)',
          }}
        />
      </a>
    );
  }

  return null;
}

export function PlayerReportPage() {
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

  const [detailOpen, setDetailOpen] = useState(false);
  const [detailItem, setDetailItem] = useState(null);
  const [reportFiles, setReportFiles] = useState([]);
  const [filesLoading, setFilesLoading] = useState(false);

  const [reviewOpen, setReviewOpen] = useState(false);
  const [reviewItem, setReviewItem] = useState(null);
  const [reviewStatus, setReviewStatus] = useState('approved');
  const [reviewNote, setReviewNote] = useState('');
  const [submitting, setSubmitting] = useState(false);

  const canReview = session?.role === 'developer' || session?.role === 'admin';

  const loadItems = useCallback(async () => {
    if (!token) return;
    try {
      setLoading(true);
      setLoadError('');
      const params = { page, page_size: 20 };
      if (search) params.search = search;
      if (status) params.status = status;
      const result = await api.playerReports(token, params);
      setData(result);
    } catch {
      setData(null);
      setLoadError('加载玩家举报失败，请稍后重试');
    } finally {
      setLoading(false);
    }
  }, [token, page, search, status]);

  React.useEffect(() => { loadItems(); }, [loadItems]);

  async function openDetail(item) {
    setDetailItem(item);
    setDetailOpen(true);
    setReportFiles([]);
    setFilesLoading(true);

    try {
      const result = await api.listPlayerReportFiles(token, item.id);
      setReportFiles(result.files ?? []);
    } catch {
      setReportFiles([]);
    } finally {
      setFilesLoading(false);
    }
  }

  function closeDetail() {
    setDetailOpen(false);
    setDetailItem(null);
    setReportFiles([]);
  }

  function openReview(item, nextStatus) {
    setReviewItem(item);
    setReviewStatus(nextStatus);
    setReviewNote('');
    setReviewOpen(true);
  }

  async function handleReview() {
    if (!reviewItem) return;
    const action = reviewStatus === 'approved' ? '通过' : '驳回';
    const confirmed = await confirm({
      title: `${action}玩家举报`,
      message: `确定${action}关于 ${reviewItem.target_player_name || reviewItem.target_steam_id} 的玩家举报吗？`,
    });
    if (!confirmed) return;

    try {
      setSubmitting(true);
      await api.reviewPlayerReport(token, reviewItem.id, {
        status: reviewStatus,
        review_note: reviewNote.trim() || null,
      });
      setReviewOpen(false);
      setReviewItem(null);
      await loadItems();
      toast({ title: `${action}成功`, message: `已${action}该玩家举报。` });
    } catch (e) {
      toast({ title: `${action}失败`, message: e.message, tone: 'danger' });
    } finally {
      setSubmitting(false);
    }
  }

  const items = data?.items ?? [];
  const total = data?.total ?? 0;

  return (
    <div id="player-report" className="content-section active">
      <div className="breadcrumb"><span>核心管理</span><span className="sep">›</span><span className="current">玩家举报</span></div>
      <div className="page-header">
        <div>
          <div className="page-title">玩家举报管理</div>
          <div className="page-sub">审核公开页提交的玩家举报与录像、录音、图片证据。</div>
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
                  <th>被举报玩家</th>
                  <th>SteamID64</th>
                  <th>举报理由</th>
                  <th>联系方式</th>
                  <th>状态</th>
                  <th>提交时间</th>
                  <th>审核人</th>
                  <th>审核时间</th>
                  <th style={{ textAlign: 'right' }}>操作</th>
                </tr>
              </thead>
              <tbody>
                {loading ? (
                  <tr><td colSpan={9} style={{ textAlign: 'center', color: 'var(--text2)' }}>正在加载玩家举报...</td></tr>
                ) : null}
                {!loading && loadError ? (
                  <tr><td colSpan={9} style={{ textAlign: 'center', color: 'var(--danger)' }}>{loadError}</td></tr>
                ) : null}
                {!loading && items.map((item) => {
                  const st = STATUS_MAP[item.status] || { label: item.status, pill: '' };
                  return (
                    <tr key={item.id}>
                      <td style={{ fontWeight: 600 }}>{item.target_player_name || '-'}</td>
                      <td className="steam-id">{item.target_steam_id}</td>
                      <td style={{ color: 'var(--text2)', maxWidth: 240, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                        <span title={item.report_reason}>{item.report_reason}</span>
                      </td>
                      <td style={{ color: 'var(--text2)', maxWidth: 160, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                        <span title={item.reporter_contact || ''}>{item.reporter_contact || '-'}</span>
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
                              <button className="action-btn action-btn-success" onClick={() => openReview(item, 'approved')}>通过</button>
                              <button className="action-btn action-btn-danger" onClick={() => openReview(item, 'rejected')}>驳回</button>
                            </>
                          ) : null}
                        </div>
                      </td>
                    </tr>
                  );
                })}
                {!loading && items.length === 0 ? (
                  <tr><td colSpan={9} style={{ textAlign: 'center', color: 'var(--text2)' }}>暂无玩家举报</td></tr>
                ) : null}
              </tbody>
            </table>
          </div>
        </div>
      </div>

      <Pagination page={page} pageSize={20} total={total} onChange={setPage} />

      <Modal
        open={detailOpen}
        title="举报详情"
        onClose={closeDetail}
        wide
        footer={<button className="btn btn-outline" onClick={closeDetail}>关闭</button>}
      >
        {detailItem ? (
          <div style={{ display: 'flex', flexDirection: 'column', gap: 14 }}>
            <div className="ban-detail-section">
              <div className="ban-detail-title">被举报玩家</div>
              <div className="ban-detail-grid">
                <div><span>玩家名称</span><strong>{detailItem.target_player_name || '-'}</strong></div>
                <div><span>SteamID64</span><strong>{detailItem.target_steam_id}</strong></div>
                <div><span>联系方式</span><strong>{detailItem.reporter_contact || '-'}</strong></div>
                <div><span>提交时间</span><strong>{formatDateTime(detailItem.created_at)}</strong></div>
              </div>
            </div>

            <div className="ban-detail-section">
              <div className="ban-detail-title">举报理由</div>
              <div style={{ color: 'var(--text2)', fontSize: 13, whiteSpace: 'pre-wrap', background: 'var(--surface2)', padding: 10, borderRadius: 8 }}>
                {detailItem.report_reason}
              </div>
            </div>

            <div className="ban-detail-section">
              <div className="ban-detail-title">处理状态</div>
              <div>
                <span className={`status-pill ${STATUS_MAP[detailItem.status]?.pill || ''}`}>
                  {STATUS_MAP[detailItem.status]?.label || detailItem.status}
                </span>
              </div>
              {detailItem.reviewed_by ? (
                <div style={{ color: 'var(--text3)', fontSize: 12, marginTop: 6 }}>
                  由 {detailItem.reviewed_by} 于 {formatDateTime(detailItem.reviewed_at)} 处理
                </div>
              ) : null}
              {detailItem.review_note ? (
                <div style={{ color: 'var(--text2)', fontSize: 13, marginTop: 8, whiteSpace: 'pre-wrap', background: 'var(--surface2)', padding: 10, borderRadius: 8 }}>
                  审核备注：{detailItem.review_note}
                </div>
              ) : null}
            </div>

            <div className="ban-detail-section">
              <div className="ban-detail-title">证据文件</div>
              {filesLoading ? (
                <div style={{ color: 'var(--text3)', fontSize: 13, padding: '8px 0' }}>正在加载证据文件...</div>
              ) : reportFiles.length > 0 ? (
                <div style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
                  {reportFiles.map((file) => (
                    <div key={file.id} style={{ padding: '10px 14px', background: 'var(--surface2)', borderRadius: 8, fontSize: 13 }}>
                      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: 12 }}>
                        <div style={{ minWidth: 0, flex: 1 }}>
                          <div style={{ display: 'flex', alignItems: 'center', gap: 8, minWidth: 0 }}>
                            <span className="status-pill">{fileTypeLabel(file.category)}</span>
                            <strong style={{ overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{file.file_name}</strong>
                          </div>
                          <div style={{ fontSize: 11, color: 'var(--text3)', marginTop: 4 }}>
                            {formatFileSize(file.file_size)}
                            {file.content_type ? ` · ${file.content_type}` : ''}
                          </div>
                        </div>
                        {file.url ? (
                          <a href={file.url} target="_blank" rel="noopener noreferrer" className="action-btn" style={{ flexShrink: 0, textDecoration: 'none' }}>
                            {fileActionLabel(file.category)}
                          </a>
                        ) : (
                          <span style={{ fontSize: 11, color: 'var(--text3)', flexShrink: 0 }}>不可用</span>
                        )}
                      </div>
                      {renderFilePreview(file)}
                    </div>
                  ))}
                </div>
              ) : (
                <div style={{ color: 'var(--text3)', fontSize: 13, padding: '8px 0' }}>该举报未上传证据文件。</div>
              )}
            </div>
          </div>
        ) : null}
      </Modal>

      <Modal
        open={reviewOpen}
        title={reviewStatus === 'approved' ? '通过玩家举报' : '驳回玩家举报'}
        onClose={() => { setReviewOpen(false); setReviewItem(null); }}
        footer={
          <>
            <button className="btn btn-outline" onClick={() => { setReviewOpen(false); setReviewItem(null); }}>取消</button>
            <button
              className={`btn ${reviewStatus === 'approved' ? 'btn-success' : 'btn-danger'}`}
              onClick={handleReview}
              disabled={submitting}
            >
              {submitting ? '处理中...' : reviewStatus === 'approved' ? '确认通过' : '确认驳回'}
            </button>
          </>
        }
      >
        {reviewItem ? (
          <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
            <div style={{ color: 'var(--text2)', fontSize: 13 }}>
              <div><strong>玩家：</strong>{reviewItem.target_player_name || '-'}（{reviewItem.target_steam_id}）</div>
              <div><strong>举报理由：</strong>{reviewItem.report_reason}</div>
            </div>
            <div className="form-group">
              <label>审核备注</label>
              <textarea
                className="form-control"
                value={reviewNote}
                onChange={(e) => setReviewNote(e.target.value)}
                placeholder={reviewStatus === 'approved' ? '可选，填写通过原因或后续处理记录...' : '可选，填写驳回原因...'}
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

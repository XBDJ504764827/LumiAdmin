import React, { useCallback, useState } from 'react';
import { api } from '../../lib/api.js';
import { useAuth } from '../../state/auth.jsx';
import { useToast, ToastContainer } from '../../shared/Toast.jsx';
import { useConfirmDialog } from '../../shared/ConfirmModal.jsx';
import { Modal } from '../../shared/Modal.jsx';
import { SearchBar } from '../../shared/SearchBar.jsx';
import { Pagination } from '../../shared/Pagination.jsx';
import { StatusPill } from '../../shared/StatusPill.jsx';
import { FilePreview, FileItem, fileTypeLabel, fileActionLabel, formatFileSize } from '../../shared/FilePreview.jsx';
import { useNavigate } from 'react-router-dom';
import { formatChinaDateTime } from '../../shared/time.js';
import { notifyPendingReviewsUpdated, usePendingReviewIndicators } from '../../hooks/usePendingReviewIndicators.js';

const STATUS_MAP = {
  pending: { label: '待审核', pill: 'warning' },
  approved: { label: '已封禁', pill: 'success' },
  rejected: { label: '已驳回', pill: 'offline' },
};
const STATUS_FILTERS = [
  { value: undefined, label: '全部状态' },
  { value: 'pending', label: '待审核' },
  { value: 'approved', label: '已封禁' },
  { value: 'rejected', label: '已驳回' },
];

export function PlayerReportPage() {
  const { session } = useAuth();
  const { confirm, dialog } = useConfirmDialog();
  const { toast, toasts, dismiss: dismissToast } = useToast();
  const navigate = useNavigate();
  const { counts: pendingCounts } = usePendingReviewIndicators();
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
  const [reviewStatus, setReviewStatus] = useState('rejected');
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

  function openBanFromReport(item) {
    navigate('/ban', {
      state: {
        playerReportPrefill: {
          reportId: item.id,
          player: item.target_player_name || '',
          steamId: item.target_steam_id,
          reason: item.report_reason,
        },
      },
    });
  }

  async function handleReview() {
    if (!reviewItem) return;
    const confirmed = await confirm({
      title: '驳回玩家举报',
      message: `确定驳回关于 ${reviewItem.target_player_name || reviewItem.target_steam_id} 的玩家举报吗？`,
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
      notifyPendingReviewsUpdated({ source: 'playerReport', action: reviewStatus });
      toast({ title: '驳回成功', message: '已驳回该玩家举报。' });
    } catch (e) {
      toast({ title: '驳回失败', message: e.message, tone: 'danger' });
    } finally {
      setSubmitting(false);
    }
  }

  const items = data?.items ?? [];
  const total = data?.total ?? 0;
  const hasPendingReports = (pendingCounts.playerReport ?? 0) > 0;

  return (
    <div id="player-report" className="content-section active">
      <div className="breadcrumb"><span>核心管理</span><span className="sep">›</span><span className="current">玩家举报</span></div>
      <div className="page-header">
        <div>
          <div className="page-title">玩家举报管理</div>
          <div className="page-sub">审核公开页提交的玩家举报与录像、录音、图片证据。</div>
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
            {filter.value === 'pending' && hasPendingReports ? (
              <span className="tab-pending-dot" title={`有 ${pendingCounts.playerReport} 条待审核玩家举报`} />
            ) : null}
          </button>
        ))}
      </div>

      <SearchBar
        value={search}
        onChange={(v) => { setSearch(v); setPage(1); }}
        placeholder="搜索 SteamID / 玩家名称..."
        aria-label="搜索玩家举报"
      />

      <div className="card">
        <div className="card-body p-0">
          <div className="table-responsive">
            <table className="data-table" role="table" aria-label="玩家举报列表">
              <thead>
                <tr>
                  <th scope="col">被举报玩家</th>
                  <th scope="col">SteamID64</th>
                  <th scope="col">举报理由</th>
                  <th scope="col">联系方式</th>
                  <th scope="col">状态</th>
                  <th scope="col">提交时间</th>
                  <th scope="col">审核人</th>
                  <th scope="col">审核时间</th>
                  <th scope="col" className="text-right">操作</th>
                </tr>
              </thead>
              <tbody>
                {loading ? (
                  <tr><td colSpan={9} className="text-center text-muted">正在加载玩家举报...</td></tr>
                ) : null}
                {!loading && loadError ? (
                  <tr><td colSpan={9} className="text-center text-danger">{loadError}</td></tr>
                ) : null}
                {!loading && items.map((item) => {
                  const st = STATUS_MAP[item.status] || { label: item.status, pill: '' };
                  return (
                    <tr key={item.id}>
                      <td className="fw-600">{item.target_player_name || '-'}</td>
                      <td className="steam-id">{item.target_steam_id}</td>
                      <td className="text-muted text-ellipsis" style={{ maxWidth: 240 }}>
                        <span title={item.report_reason}>{item.report_reason}</span>
                      </td>
                      <td className="text-muted text-ellipsis" style={{ maxWidth: 160 }}>
                        <span title={item.reporter_contact || ''}>{item.reporter_contact || '-'}</span>
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
                              <button className="action-btn action-btn-danger" onClick={() => openBanFromReport(item)}>封禁</button>
                              <button className="action-btn action-btn-danger" onClick={() => openReview(item, 'rejected')}>驳回</button>
                            </>
                          ) : null}
                        </div>
                      </td>
                    </tr>
                  );
                })}
                {!loading && items.length === 0 ? (
                  <tr><td colSpan={9} className="text-center text-muted">暂无玩家举报</td></tr>
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
        title="举报详情"
        onClose={closeDetail}
        footer={
          <>
            <button className="btn btn-outline" onClick={closeDetail}>关闭</button>
            {canReview && detailItem?.status === 'pending' ? (
              <>
                <button className="btn btn-danger" onClick={() => { const item = detailItem; closeDetail(); openBanFromReport(item); }}>封禁</button>
                <button className="btn btn-outline" onClick={() => { const item = detailItem; closeDetail(); openReview(item, 'rejected'); }}>驳回</button>
              </>
            ) : null}
          </>
        }
      >
        {detailItem ? (
          <div className="flex-col gap-12">
            <div className="detail-field">
              <div className="detail-field-label">玩家信息</div>
              <div className="detail-field-value">
                <div>名称：{detailItem.target_player_name || '-'}</div>
                <div>SteamID64：{detailItem.target_steam_id}</div>
                <div>联系方式：{detailItem.reporter_contact || '-'}</div>
              </div>
            </div>

            <div className="detail-field">
              <div className="detail-field-label">举报信息</div>
              <div className="detail-field-value">
                <div>提交时间：{formatChinaDateTime(detailItem.created_at, { seconds: false })}</div>
                <div>证据文件：{reportFiles.length} 个</div>
              </div>
            </div>

            <div className="detail-field">
              <div className="detail-field-label">举报理由</div>
              <div className="detail-field-value-block">{detailItem.report_reason}</div>
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

            <div className="detail-field">
              <label className="detail-field-label">证据文件</label>
              {filesLoading ? (
                <div className="text-muted-light fs-13 p-8">正在加载证据文件...</div>
              ) : reportFiles.length > 0 ? (
                <div className="flex-col gap-10">
                  {reportFiles.map((file) => (
                    <FileItem key={file.id} file={file}>
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
                    </FileItem>
                  ))}
                </div>
              ) : (
                <div className="text-muted-light fs-13 p-8">该举报未上传证据文件。</div>
              )}
            </div>
          </div>
        ) : null}
      </Modal>

      {/* 驳回弹窗 */}
      <Modal
        open={reviewOpen}
        title="驳回玩家举报"
        onClose={() => { setReviewOpen(false); setReviewItem(null); }}
        footer={
          <>
            <button className="btn btn-outline" onClick={() => { setReviewOpen(false); setReviewItem(null); }}>取消</button>
            <button
              className="btn btn-danger"
              onClick={handleReview}
              disabled={submitting}
            >
              {submitting ? '处理中...' : '确认驳回'}
            </button>
          </>
        }
      >
        {reviewItem ? (
          <div className="flex-col gap-12">
            <div className="text-muted fs-13">
              <div><strong>玩家：</strong>{reviewItem.target_player_name || '-'}（{reviewItem.target_steam_id}）</div>
              <div><strong>举报理由：</strong>{reviewItem.report_reason}</div>
            </div>
            <div className="form-group">
              <label htmlFor="report-review-note">审核备注</label>
              <textarea
                id="report-review-note"
                className="form-control"
                value={reviewNote}
                onChange={(e) => setReviewNote(e.target.value)}
                placeholder="可选，填写驳回原因..."
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

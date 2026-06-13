import React, { useState } from 'react';
import { api } from '../../lib/api.js';
import { useApiQuery } from '../../shared/useApiQuery.js';
import { useAuth } from '../../state/auth.jsx';
import { useToast, ToastContainer } from '../../shared/Toast.jsx';
import { notifyPendingReviewsUpdated } from '../../hooks/usePendingReviewIndicators.js';
import { Modal } from '../../shared/Modal.jsx';
import { SearchBar } from '../../shared/SearchBar.jsx';
import { Pagination } from '../../shared/Pagination.jsx';
import { StatusPill } from '../../shared/StatusPill.jsx';
import { TableLoading, TableError, TableEmpty } from '../../shared/TableState.jsx';
import { formatChinaDateTime } from '../../shared/time.js';

const TYPE_MAP = {
  missing: { label: '地图缺失', pill: 'warning' },
  broken: { label: '地图异常', pill: 'danger' },
  request: { label: '需要添加', pill: 'info' },
};

const STATUS_MAP = {
  pending: { label: '待处理', pill: 'warning' },
  resolved: { label: '已处理', pill: 'success' },
  rejected: { label: '已驳回', pill: 'offline' },
};

const STATUS_FILTERS = [
  { value: undefined, label: '全部状态' },
  { value: 'pending', label: '待处理' },
  { value: 'resolved', label: '已处理' },
  { value: 'rejected', label: '已驳回' },
];

export function MapFeedbackPage() {
  const { session } = useAuth();
  const token = session?.token ?? null;
  const { toast, toasts, dismiss } = useToast();

  const [search, setSearch] = useState('');
  const [status, setStatus] = useState(undefined);
  const [page, setPage] = useState(1);

  // 审核弹窗
  const [reviewItem, setReviewItem] = useState(null);
  const [reviewStatus, setReviewStatus] = useState('');
  const [reviewNote, setReviewNote] = useState('');
  const [reviewing, setReviewing] = useState(false);

  // 详情弹窗
  const [detailItem, setDetailItem] = useState(null);

  const { data, isLoading, error, refetch } = useApiQuery(
    ['mapFeedback', { page, search, status }],
    (token) => api.mapFeedback(token, { page, page_size: 20, ...(search ? { search } : {}), ...(status ? { status } : {}) }),
  );

  const items = data?.items ?? [];
  const total = data?.total ?? 0;

  async function openReviewModal(item) {
    setReviewItem(item);
    setReviewStatus('');
    setReviewNote('');
  }

  async function handleReview() {
    if (!reviewItem || !reviewStatus || reviewing) return;
    setReviewing(true);
    try {
      await api.reviewMapFeedback(token, reviewItem.id, {
        status: reviewStatus,
        review_note: reviewNote.trim() || null,
      });
      toast({ title: '审核成功', message: '地图反馈已处理。' });
      notifyPendingReviewsUpdated({ source: 'mapFeedback', action: 'review' });
      setReviewItem(null);
      refetch();
    } catch (e) {
      toast({ title: '审核失败', message: e.message, tone: 'danger' });
    } finally {
      setReviewing(false);
    }
  }

  return (
    <div className="content-section active">
      <div className="breadcrumb">
        <span>日志与审计</span><span className="sep">›</span>
        <span className="current">地图反馈</span>
      </div>
      <div className="page-header">
        <div>
          <div className="page-title">地图反馈</div>
          <div className="page-sub">玩家通过公开页面提交的地图缺失、异常和添加请求。</div>
        </div>
      </div>

      <SearchBar
        value={search}
        onChange={(v) => { setSearch(v); setPage(1); }}
        placeholder="搜索详细内容 / SteamID / 联系方式..."
        statusOptions={STATUS_FILTERS}
        statusValue={status}
        onStatusChange={(v) => { setStatus(v); setPage(1); }}
      />

      <div className="card">
        <div className="card-body p-0">
          <div className="table-responsive">
            <table className="data-table">
              <thead>
                <tr>
                  <th>类型</th>
                  <th>详细内容</th>
                  <th>SteamID</th>
                  <th>联系方式</th>
                  <th>状态</th>
                  <th>提交时间</th>
                  <th className="text-right">操作</th>
                </tr>
              </thead>
              <tbody>
                {isLoading ? <TableLoading colSpan={7} text="正在加载地图反馈..." /> : null}
                {!isLoading && error ? <TableError colSpan={7} message={error.message} /> : null}
                {!isLoading && !error && items.length === 0 ? <TableEmpty colSpan={7} text="暂无地图反馈" /> : null}
                {!isLoading && !error ? items.map((item) => {
                  const typeInfo = TYPE_MAP[item.feedback_type] || { label: item.feedback_type, pill: 'default' };
                  const statusInfo = STATUS_MAP[item.status] || { label: item.status, pill: 'default' };
                  return (
                    <tr key={item.id}>
                      <td><StatusPill kind={typeInfo.pill}>{typeInfo.label}</StatusPill></td>
                      <td style={{ maxWidth: 300 }}>
                        <div className="text-ellipsis-260" title={item.detail}>{item.detail}</div>
                      </td>
                      <td>
                        {item.steam_id
                          ? <div>
                              <code className="steam-id">{item.steam_id}</code>
                              {item.steam_persona_name ? <div style={{ fontSize: 11, color: 'var(--text3)' }}>{item.steam_persona_name}</div> : null}
                            </div>
                          : '-'}
                      </td>
                      <td>{item.contact || '-'}</td>
                      <td><StatusPill kind={statusInfo.pill}>{statusInfo.label}</StatusPill></td>
                      <td style={{ whiteSpace: 'nowrap' }}>{formatChinaDateTime(item.created_at, { seconds: false })}</td>
                      <td className="text-right">
                        <div className="action-btn-group">
                          <button className="action-btn" onClick={() => setDetailItem(item)}>详细</button>
                          {item.status === 'pending'
                            ? <button className="action-btn action-btn-accent" onClick={() => openReviewModal(item)}>审核</button>
                            : null}
                        </div>
                      </td>
                    </tr>
                  );
                }) : null}
              </tbody>
            </table>
          </div>
        </div>
      </div>

      <Pagination page={page} pageSize={20} total={total} onChange={setPage} />

      {/* 详情弹窗 */}
      {detailItem ? (
        <Modal
          open={true}
          title="地图反馈详情"
          onClose={() => setDetailItem(null)}
          footer={<button className="btn btn-outline" onClick={() => setDetailItem(null)}>关闭</button>}
        >
          <div className="detail-grid">
            <span className="detail-label">类型</span>
            <span className="detail-value">
              <StatusPill kind={(TYPE_MAP[detailItem.feedback_type] || {}).pill || 'default'}>
                {(TYPE_MAP[detailItem.feedback_type] || {}).label || detailItem.feedback_type}
              </StatusPill>
            </span>

            <span className="detail-label">状态</span>
            <span className="detail-value">
              <StatusPill kind={(STATUS_MAP[detailItem.status] || {}).pill || 'default'}>
                {(STATUS_MAP[detailItem.status] || {}).label || detailItem.status}
              </StatusPill>
            </span>

            <div className="detail-divider" />

            <span className="detail-label">详细内容</span>
            <span className="detail-value pre">{detailItem.detail}</span>

            {detailItem.steam_id ? (
              <>
                <span className="detail-label">SteamID64</span>
                <span className="detail-value mono">{detailItem.steam_id}</span>
              </>
            ) : null}
            {detailItem.steam_persona_name ? (
              <>
                <span className="detail-label">Steam 名称</span>
                <span className="detail-value">{detailItem.steam_persona_name}</span>
              </>
            ) : null}
            {detailItem.contact ? (
              <>
                <span className="detail-label">联系方式</span>
                <span className="detail-value">{detailItem.contact}</span>
              </>
            ) : null}

            <div className="detail-divider" />

            <span className="detail-label">提交时间</span>
            <span className="detail-value">{formatChinaDateTime(detailItem.created_at)}</span>

            {detailItem.reviewed_by ? (
              <>
                <span className="detail-label">处理人</span>
                <span className="detail-value">{detailItem.reviewed_by}</span>
              </>
            ) : null}
            {detailItem.reviewed_at ? (
              <>
                <span className="detail-label">处理时间</span>
                <span className="detail-value">{formatChinaDateTime(detailItem.reviewed_at)}</span>
              </>
            ) : null}
            {detailItem.review_note ? (
              <>
                <span className="detail-label">处理备注</span>
                <span className="detail-value pre">{detailItem.review_note}</span>
              </>
            ) : null}
          </div>
        </Modal>
      ) : null}

      {/* 审核弹窗 */}
      {reviewItem ? (
        <Modal
          open={true}
          title="审核地图反馈"
          onClose={() => setReviewItem(null)}
          footer={
            <>
              <button className="btn btn-outline" onClick={() => setReviewItem(null)}>取消</button>
              <button className="btn btn-primary" onClick={handleReview} disabled={!reviewStatus || reviewing}>
                {reviewing ? '处理中...' : '确认'}
              </button>
            </>
          }
        >
          <div className="detail-grid" style={{ marginBottom: 16 }}>
            <span className="detail-label">类型</span>
            <span className="detail-value">
              <StatusPill kind={(TYPE_MAP[reviewItem.feedback_type] || {}).pill || 'default'}>
                {(TYPE_MAP[reviewItem.feedback_type] || {}).label || reviewItem.feedback_type}
              </StatusPill>
            </span>
            <span className="detail-label">详细内容</span>
            <span className="detail-value pre">{reviewItem.detail}</span>
          </div>
          <div className="form-group">
            <label>处理结果 <span className="text-accent">*</span></label>
            <select className="form-control" value={reviewStatus} onChange={(e) => setReviewStatus(e.target.value)}>
              <option value="">请选择</option>
              <option value="resolved">已处理（已添加/已修复）</option>
              <option value="rejected">驳回（无法处理）</option>
            </select>
          </div>
          <div className="form-group">
            <label>处理备注</label>
            <textarea
              className="form-control"
              rows={3}
              value={reviewNote}
              onChange={(e) => setReviewNote(e.target.value)}
              placeholder="可选，向玩家说明处理结果"
            />
          </div>
        </Modal>
      ) : null}

      <ToastContainer toasts={toasts} onDismiss={dismiss} />
    </div>
  );
}

import { useMemo, useState } from 'react';
import { api } from '../../lib/api.js';
import { useApiQuery } from '../../shared/useApiQuery.js';
import { useAuth } from '../../state/store.js';
import { useToast } from '../../shared/Toast.jsx';
import { useConfirmDialog } from '../../shared/ConfirmModal.jsx';
import { Modal } from '../../shared/Modal.jsx';
import { SearchBar } from '../../shared/SearchBar.jsx';
import { Pagination } from '../../shared/Pagination.jsx';
import { StatusPill } from '../../shared/StatusPill.jsx';
import { TableLoading, TableError, TableEmpty } from '../../shared/TableState.jsx';
import { FileItem, fileActionLabel } from '../../shared/FilePreview.jsx';
import { formatChinaDateTime } from '../../shared/time.js';
import { notifyPendingReviewsUpdated, usePendingReviewIndicators } from '../../hooks/usePendingReviewIndicators.js';
import { buildRulePayload, createEmptyRuleForm, ruleMapLabel, ruleToForm } from './abnormalRuleForm.js';

const RECORD_STATUS = {
  pending: { label: '待审核', pill: 'warning' },
  approved: { label: '已放行', pill: 'success' },
  rejected: { label: '已作废', pill: 'offline' },
  approved_but_submit_failed: { label: '提交失败', pill: 'danger' },
};

const GLOBAL_STATUS = {
  not_submitted: { label: '未提交', pill: 'offline' },
  queued: { label: '等待提交', pill: 'info' },
  submitting: { label: '提交中', pill: 'warning' },
  submitted: { label: '已提交', pill: 'success' },
  failed: { label: '提交失败', pill: 'danger' },
  discarded: { label: '已丢弃', pill: 'offline' },
};

const STATUS_FILTERS = [
  { value: undefined, label: '全部状态' },
  { value: 'pending', label: '待审核' },
  { value: 'approved', label: '已放行' },
  { value: 'rejected', label: '已作废' },
  { value: 'approved_but_submit_failed', label: '提交失败' },
];

const MODE_OPTIONS = [
  { value: '', label: '全部模式' },
  { value: 'vnl', label: 'VNL' },
  { value: 'skz', label: 'SKZ' },
  { value: 'kzt', label: 'KZT' },
];

const TIME_TYPE_OPTIONS = [
  { value: '', label: '全部类型' },
  { value: 'tp', label: 'TP' },
  { value: 'pro', label: 'PRO' },
];

function statusConfig(map, value) {
  return map[value] || { label: value || '-', pill: 'muted' };
}

function formatRunTime(seconds) {
  const value = Number(seconds);
  if (!Number.isFinite(value)) return '-';
  const minutes = Math.floor(value / 60);
  const secs = value - minutes * 60;
  return `${String(minutes).padStart(2, '0')}:${secs.toFixed(2).padStart(5, '0')}`;
}

function formatMode(value) {
  return value ? String(value).toUpperCase() : '-';
}

function formatTimeType(value) {
  if (!value) return '-';
  return String(value).toUpperCase();
}

function replayFileFromRecord(item, replayUrl) {
  if (!item || !replayUrl) return null;
  return {
    id: item.id,
    file_name: item.replay_file_name || `${item.map_name}.replay`,
    file_size: item.replay_file_size,
    content_type: item.replay_content_type,
    category: item.replay_category || 'replay',
    url: replayUrl,
  };
}

export function AbnormalRecordPage() {
  const { session } = useAuth();
  const token = session?.token ?? null;
  const { toast } = useToast();
  const { confirm, dialog } = useConfirmDialog();
  const { counts: pendingCounts } = usePendingReviewIndicators();

  const [tab, setTab] = useState('records');
  const [search, setSearch] = useState('');
  const [status, setStatus] = useState('pending');
  const [page, setPage] = useState(1);

  const recordsQuery = useApiQuery(
    ['abnormalRecords', { page, search, status }],
    (token) => api.abnormalRecords(token, {
      page,
      page_size: 20,
      ...(search ? { search } : {}),
      ...(status ? { status } : {}),
    }),
    { enabled: tab === 'records' },
  );

  const [ruleSearch, setRuleSearch] = useState('');
  const [rulePage, setRulePage] = useState(1);
  const rulesQuery = useApiQuery(
    ['abnormalRecordRules', { rulePage, ruleSearch }],
    (token) => api.abnormalRecordRules(token, {
      page: rulePage,
      page_size: 20,
      ...(ruleSearch ? { search: ruleSearch } : {}),
    }),
    { enabled: tab === 'rules' },
  );

  const [detailOpen, setDetailOpen] = useState(false);
  const [detailItem, setDetailItem] = useState(null);
  const [replayUrl, setReplayUrl] = useState(null);
  const [replayLoading, setReplayLoading] = useState(false);

  const [reviewOpen, setReviewOpen] = useState(false);
  const [reviewItem, setReviewItem] = useState(null);
  const [reviewAction, setReviewAction] = useState('reject');
  const [reviewNote, setReviewNote] = useState('');
  const [banReason, setBanReason] = useState('利用地图 BUG/插件 BUG 刷榜');
  const [banDuration, setBanDuration] = useState(0);
  const [submitting, setSubmitting] = useState(false);

  const [ruleOpen, setRuleOpen] = useState(false);
  const [ruleEditing, setRuleEditing] = useState(null);
  const [ruleForm, setRuleForm] = useState(createEmptyRuleForm);

  const records = recordsQuery.data?.items ?? [];
  const recordTotal = recordsQuery.data?.total ?? 0;
  const rules = rulesQuery.data?.items ?? [];
  const ruleTotal = rulesQuery.data?.total ?? 0;
  const hasPending = (pendingCounts.abnormalRecord ?? 0) > 0;

  async function openDetail(item) {
    setDetailItem(item);
    setDetailOpen(true);
    setReplayUrl(null);
    if (!item.replay_storage_key) return;
    try {
      setReplayLoading(true);
      const result = await api.getAbnormalRecordReplayUrl(token, item.id);
      setReplayUrl(result.url || null);
    } catch {
      setReplayUrl(null);
    } finally {
      setReplayLoading(false);
    }
  }

  function closeDetail() {
    setDetailOpen(false);
    setDetailItem(null);
    setReplayUrl(null);
  }

  function openReview(item, action = 'reject') {
    setReviewItem(item);
    setReviewAction(action);
    setReviewNote('');
    setBanReason('利用地图 BUG/插件 BUG 刷榜');
    setBanDuration(0);
    setReviewOpen(true);
  }

  async function handleReviewSubmit() {
    if (!reviewItem) return;
    const actionText = reviewAction === 'approve' ? '放行并提交全球榜单' : reviewAction === 'reject_ban' ? '作废并封禁' : '作废成绩';
    const confirmed = await confirm({
      title: '确认裁决',
      message: `确定对 ${reviewItem.player_name || reviewItem.steam_id64} 的异常记录执行“${actionText}”吗？`,
    });
    if (!confirmed) return;

    try {
      setSubmitting(true);
      if (reviewAction === 'approve') {
        await api.approveAbnormalRecord(token, reviewItem.id, { review_note: reviewNote.trim() || null });
      } else if (reviewAction === 'reject_ban') {
        await api.rejectAndBanAbnormalRecord(token, reviewItem.id, {
          review_note: reviewNote.trim() || null,
          reason: banReason.trim(),
          duration_minutes: Number(banDuration) || 0,
        });
      } else {
        await api.rejectAbnormalRecord(token, reviewItem.id, { review_note: reviewNote.trim() || null });
      }
      setReviewOpen(false);
      setReviewItem(null);
      closeDetail();
      await recordsQuery.refetch();
      notifyPendingReviewsUpdated({ source: 'abnormalRecord', action: reviewAction });
      toast({ title: '裁决已提交', message: `已执行：${actionText}` });
    } catch (e) {
      toast({ title: '裁决失败', message: e.message, tone: 'danger' });
    } finally {
      setSubmitting(false);
    }
  }

  async function handleRetrySubmit(item) {
    const confirmed = await confirm({
      title: '重试提交',
      message: `确定重新将 ${item.player_name || item.steam_id64} 的记录加入全球提交队列吗？`,
    });
    if (!confirmed) return;
    try {
      await api.retryAbnormalRecordSubmit(token, item.id);
      await recordsQuery.refetch();
      toast({ title: '已加入队列', message: '插件下次轮询时会重新提交该记录。' });
    } catch (e) {
      toast({ title: '重试失败', message: e.message, tone: 'danger' });
    }
  }

  function openRuleForm(rule = null, scope = 'single_map') {
    setRuleEditing(rule);
    setRuleForm(rule ? ruleToForm(rule) : createEmptyRuleForm(scope));
    setRuleOpen(true);
  }

  async function handleRuleSubmit() {
    const { error, payload: body } = buildRulePayload(ruleForm);
    if (error) {
      toast({ title: '无法保存规则', message: error, tone: 'danger' });
      return;
    }
    try {
      setSubmitting(true);
      if (ruleEditing) {
        await api.updateAbnormalRecordRule(token, ruleEditing.id, body);
      } else {
        await api.createAbnormalRecordRule(token, body);
      }
      setRuleOpen(false);
      setRuleEditing(null);
      await rulesQuery.refetch();
      toast({ title: '规则已保存', message: ruleMapLabel(body.map_name) });
    } catch (e) {
      toast({ title: '保存失败', message: e.message, tone: 'danger' });
    } finally {
      setSubmitting(false);
    }
  }

  async function handleDeleteRule(rule) {
    const confirmed = await confirm({
      title: '删除规则',
      message: `确定删除 ${ruleMapLabel(rule.map_name)} 的异常时间规则吗？`,
    });
    if (!confirmed) return;
    try {
      await api.deleteAbnormalRecordRule(token, rule.id);
      await rulesQuery.refetch();
      toast({ title: '规则已删除', message: ruleMapLabel(rule.map_name) });
    } catch (e) {
      toast({ title: '删除失败', message: e.message, tone: 'danger' });
    }
  }

  const replayFile = useMemo(() => replayFileFromRecord(detailItem, replayUrl), [detailItem, replayUrl]);

  return (
    <div id="abnormal-records" className="content-section active">
      <div className="breadcrumb"><span>核心管理</span><span className="sep">›</span><span className="current">异常记录审核</span></div>
      <div className="page-header">
        <div>
          <div className="page-title">异常记录池与成绩审核</div>
          <div className="page-sub">拦截完成时间异常的成绩，审核录像后决定是否放行至全球榜单。</div>
        </div>
      </div>

      <div className="tabs">
        <button className={`tab ${tab === 'records' ? 'active' : ''}`} onClick={() => setTab('records')}>
          <span>异常记录池</span>
          {hasPending ? <span className="tab-pending-dot" title={`有 ${pendingCounts.abnormalRecord} 条待审核异常记录`} /> : null}
        </button>
        <button className={`tab ${tab === 'rules' ? 'active' : ''}`} onClick={() => setTab('rules')}>地图异常时间</button>
      </div>

      {tab === 'records' ? (
        <>
          <div className="tabs">
            {STATUS_FILTERS.map((filter) => (
              <button
                key={filter.value ?? 'all'}
                className={`tab ${status === filter.value ? 'active' : ''}`}
                onClick={() => { setStatus(filter.value); setPage(1); }}
              >
                <span>{filter.label}</span>
                {filter.value === 'pending' && hasPending ? (
                  <span className="tab-pending-dot" title={`有 ${pendingCounts.abnormalRecord} 条待审核异常记录`} />
                ) : null}
              </button>
            ))}
          </div>

          <SearchBar
            value={search}
            onChange={(value) => { setSearch(value); setPage(1); }}
            placeholder="搜索玩家 / SteamID / 地图 / 服务器..."
            aria-label="搜索异常记录"
          />

          <div className="card">
            <div className="card-body p-0">
              <div className="table-responsive">
                <table className="data-table mobile-card-table" role="table" aria-label="异常记录列表">
                  <thead>
                    <tr>
                      <th scope="col">玩家</th>
                      <th scope="col">地图</th>
                      <th scope="col">模式</th>
                      <th scope="col">异常时间</th>
                      <th scope="col">阈值</th>
                      <th scope="col">录像</th>
                      <th scope="col">状态</th>
                      <th scope="col">全球提交</th>
                      <th scope="col">上传时间</th>
                      <th scope="col" className="text-right">操作</th>
                    </tr>
                  </thead>
                  <tbody>
                    {recordsQuery.isLoading ? <TableLoading colSpan={10} text="正在加载异常记录..." /> : null}
                    {!recordsQuery.isLoading && recordsQuery.error ? <TableError colSpan={10} message={recordsQuery.error.message} /> : null}
                    {!recordsQuery.isLoading && records.map((item) => {
                      const st = statusConfig(RECORD_STATUS, item.status);
                      const gst = statusConfig(GLOBAL_STATUS, item.global_submit_status);
                      return (
                        <tr key={item.id}>
                          <td className="fw-600 mobile-card-primary" data-label="玩家">
                            {item.player_name || '-'}
                            <div className="steam-id">{item.steam_id64}</div>
                          </td>
                          <td className="steam-id" data-label="地图">{item.map_name}</td>
                          <td data-label="模式">{formatMode(item.mode)} · {formatTimeType(item.time_type)} · C{item.course}</td>
                          <td style={{ color: 'var(--danger-text)', fontWeight: 700 }} data-label="异常时间">{formatRunTime(item.run_time_seconds)}</td>
                          <td className="text-muted-light" data-label="阈值">{formatRunTime(item.threshold_seconds)}</td>
                          <td data-label="录像">
                            <span className={`tag-badge ${item.replay_storage_key ? 'info' : 'muted'}`}>
                              {item.replay_storage_key ? 'R2 已存证' : '待上传'}
                            </span>
                          </td>
                          <td data-label="状态"><StatusPill kind={st.pill}>{st.label}</StatusPill></td>
                          <td data-label="全球提交"><StatusPill kind={gst.pill}>{gst.label}</StatusPill></td>
                          <td className="text-muted-light" data-label="上传时间">{formatChinaDateTime(item.created_at, { seconds: false })}</td>
                          <td className="text-right mobile-card-actions" data-label="操作">
                            <div className="action-btn-group">
                              <button className="action-btn action-btn-accent" onClick={() => openDetail(item)}>详情</button>
                              {item.status === 'approved_but_submit_failed' ? (
                                <button className="action-btn action-btn-success" onClick={() => handleRetrySubmit(item)}>重试</button>
                              ) : null}
                            </div>
                          </td>
                        </tr>
                      );
                    })}
                    {!recordsQuery.isLoading && !recordsQuery.error && records.length === 0 ? <TableEmpty colSpan={10} text="暂无异常记录" /> : null}
                  </tbody>
                </table>
              </div>
            </div>
          </div>

          <Pagination page={page} pageSize={20} total={recordTotal} onChange={setPage} />
        </>
      ) : (
        <>
          <SearchBar
            value={ruleSearch}
            onChange={(value) => { setRuleSearch(value); setRulePage(1); }}
            placeholder="搜索地图名称..."
            aria-label="搜索异常时间规则"
            actions={(
              <>
                <button type="button" className="btn btn-outline btn-sm" onClick={() => openRuleForm(null, 'all_maps')}>设置全部地图默认阈值</button>
                <button type="button" className="btn btn-primary btn-sm" onClick={() => openRuleForm()}>新增单地图规则</button>
              </>
            )}
          />

          <div className="card">
            <div className="card-body p-0">
              <div className="table-responsive">
                <table className="data-table mobile-card-table" role="table" aria-label="地图异常时间规则">
                  <thead>
                    <tr>
                      <th scope="col">地图</th>
                      <th scope="col">Course</th>
                      <th scope="col">模式</th>
                      <th scope="col">类型</th>
                      <th scope="col">异常阈值</th>
                      <th scope="col">状态</th>
                      <th scope="col">更新人</th>
                      <th scope="col">更新时间</th>
                      <th scope="col" className="text-right">操作</th>
                    </tr>
                  </thead>
                  <tbody>
                    {rulesQuery.isLoading ? <TableLoading colSpan={9} text="正在加载异常时间规则..." /> : null}
                    {!rulesQuery.isLoading && rulesQuery.error ? <TableError colSpan={9} message={rulesQuery.error.message} /> : null}
                    {!rulesQuery.isLoading && rules.map((rule) => (
                      <tr key={rule.id}>
                        <td className="steam-id mobile-card-primary" data-label="地图">{ruleMapLabel(rule.map_name)}</td>
                        <td data-label="Course">C{rule.course}</td>
                        <td data-label="模式">{rule.mode ? formatMode(rule.mode) : '全部'}</td>
                        <td data-label="类型">{rule.time_type ? formatTimeType(rule.time_type) : '全部'}</td>
                        <td style={{ color: 'var(--danger-text)', fontWeight: 700 }} data-label="异常阈值">{formatRunTime(rule.threshold_seconds)}</td>
                        <td data-label="状态"><StatusPill kind={rule.enabled ? 'success' : 'offline'}>{rule.enabled ? '启用' : '停用'}</StatusPill></td>
                        <td data-label="更新人">{rule.updated_by || rule.created_by || '-'}</td>
                        <td className="text-muted-light" data-label="更新时间">{formatChinaDateTime(rule.updated_at, { seconds: false })}</td>
                        <td className="text-right mobile-card-actions" data-label="操作">
                          <div className="action-btn-group">
                            <button className="action-btn action-btn-accent" onClick={() => openRuleForm(rule)}>编辑</button>
                            <button className="action-btn action-btn-danger" onClick={() => handleDeleteRule(rule)}>删除</button>
                          </div>
                        </td>
                      </tr>
                    ))}
                    {!rulesQuery.isLoading && !rulesQuery.error && rules.length === 0 ? <TableEmpty colSpan={9} text="暂无异常时间规则" /> : null}
                  </tbody>
                </table>
              </div>
            </div>
          </div>

          <Pagination page={rulePage} pageSize={20} total={ruleTotal} onChange={setRulePage} />
        </>
      )}

      <Modal
        open={detailOpen}
        title="异常记录详情"
        onClose={closeDetail}
        extraWide
        footer={
          detailItem?.status === 'pending' ? (
            <>
              <button className="btn btn-outline" onClick={closeDetail}>关闭</button>
              <button className="btn btn-danger" onClick={() => { const item = detailItem; closeDetail(); openReview(item, 'reject'); }}>作废</button>
              <button className="btn btn-danger" onClick={() => { const item = detailItem; closeDetail(); openReview(item, 'reject_ban'); }}>作废并封禁</button>
              <button className="btn btn-primary" onClick={() => { const item = detailItem; closeDetail(); openReview(item, 'approve'); }}>放行</button>
            </>
          ) : (
            <button className="btn btn-outline" onClick={closeDetail}>关闭</button>
          )
        }
      >
        {detailItem ? (
          <div className="flex-col gap-12">
            <div className="detail-field">
              <div className="detail-field-label">玩家与服务器</div>
              <div className="detail-field-value">
                <div>玩家：{detailItem.player_name || '-'}</div>
                <div>SteamID64：{detailItem.steam_id64}</div>
                <div>服务器：{detailItem.server_name || '-'}{detailItem.server_port ? `:${detailItem.server_port}` : ''}</div>
              </div>
            </div>
            <div className="detail-field">
              <div className="detail-field-label">成绩数据</div>
              <div className="detail-field-value">
                <div>地图：{detailItem.map_name}</div>
                <div>模式：{formatMode(detailItem.mode)} · {formatTimeType(detailItem.time_type)} · Course {detailItem.course}</div>
                <div>完赛时间：{formatRunTime(detailItem.run_time_seconds)} / 阈值：{formatRunTime(detailItem.threshold_seconds)}</div>
                <div>TP 次数：{detailItem.teleports}</div>
              </div>
            </div>
            <div className="detail-field">
              <div className="detail-field-label">录像存证</div>
              {replayLoading ? (
                <div className="text-muted-light fs-13 p-8">正在加载录像...</div>
              ) : replayFile ? (
                <FileItem file={replayFile}>
                  <div style={{ display: 'flex', alignItems: 'center', gap: 6, flexShrink: 0 }}>
                    <a href={replayFile.url} target="_blank" rel="noopener noreferrer" className="action-btn flex-shrink-0">
                      {fileActionLabel(replayFile.category)}
                    </a>
                  </div>
                </FileItem>
              ) : (
                <div className="text-muted-light fs-13 p-8">该记录尚未上传录像。</div>
              )}
            </div>
            <div className="detail-field">
              <div className="detail-field-label">处理状态</div>
              <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap' }}>
                <StatusPill kind={statusConfig(RECORD_STATUS, detailItem.status).pill}>{statusConfig(RECORD_STATUS, detailItem.status).label}</StatusPill>
                <StatusPill kind={statusConfig(GLOBAL_STATUS, detailItem.global_submit_status).pill}>{statusConfig(GLOBAL_STATUS, detailItem.global_submit_status).label}</StatusPill>
              </div>
              {detailItem.reviewed_by ? (
                <div className="text-muted-light fs-12 mt-4">
                  由 {detailItem.reviewed_by} 于 {formatChinaDateTime(detailItem.reviewed_at, { seconds: false })} 处理
                </div>
              ) : null}
              {detailItem.review_note ? <div className="review-note">审核备注：{detailItem.review_note}</div> : null}
              {detailItem.global_submit_error ? <div className="review-note">提交错误：{detailItem.global_submit_error}</div> : null}
            </div>
          </div>
        ) : null}
      </Modal>

      <Modal
        open={reviewOpen}
        title="异常记录审核裁决"
        onClose={() => { setReviewOpen(false); setReviewItem(null); }}
        footer={
          <>
            <button className="btn btn-outline" onClick={() => { setReviewOpen(false); setReviewItem(null); }}>取消</button>
            <button className="btn btn-primary" onClick={handleReviewSubmit} disabled={submitting}>{submitting ? '处理中...' : '确认并执行'}</button>
          </>
        }
      >
        {reviewItem ? (
          <div className="flex-col gap-12">
            <div className="detail-field">
              <div className="detail-field-value">
                <div>玩家：{reviewItem.player_name || '-'}（{reviewItem.steam_id64}）</div>
                <div>地图：{reviewItem.map_name}</div>
                <div>异常成绩：{formatRunTime(reviewItem.run_time_seconds)} / 阈值：{formatRunTime(reviewItem.threshold_seconds)}</div>
              </div>
            </div>
            <div className="form-group">
              <label>裁决结果</label>
              <select className="form-control" value={reviewAction} onChange={(event) => setReviewAction(event.target.value)}>
                <option value="reject">作废成绩</option>
                <option value="reject_ban">作废成绩并同步封禁</option>
                <option value="approve">录像合规，放行至全球榜单</option>
              </select>
            </div>
            {reviewAction === 'reject_ban' ? (
              <div className="card" style={{ marginBottom: 0 }}>
                <div className="card-body">
                  <div className="form-group">
                    <label>封禁时长</label>
                    <select className="form-control" value={banDuration} onChange={(event) => setBanDuration(Number(event.target.value))}>
                      <option value={0}>永久封禁</option>
                      <option value={10080}>7 天</option>
                      <option value={43200}>30 天</option>
                    </select>
                  </div>
                  <div className="form-group" style={{ marginBottom: 0 }}>
                    <label>封禁理由</label>
                    <input className="form-control" value={banReason} onChange={(event) => setBanReason(event.target.value)} />
                  </div>
                </div>
              </div>
            ) : null}
            <div className="form-group">
              <label>审核备注</label>
              <textarea className="form-control" rows={3} value={reviewNote} onChange={(event) => setReviewNote(event.target.value)} placeholder="可选，填写内部审核批注..." />
            </div>
          </div>
        ) : null}
      </Modal>

      <Modal
        open={ruleOpen}
        title={ruleEditing ? '编辑异常时间规则' : '新增异常时间规则'}
        onClose={() => { setRuleOpen(false); setRuleEditing(null); }}
        footer={
          <>
            <button className="btn btn-outline" onClick={() => { setRuleOpen(false); setRuleEditing(null); }}>取消</button>
            <button className="btn btn-primary" onClick={handleRuleSubmit} disabled={submitting}>{submitting ? '保存中...' : '保存规则'}</button>
          </>
        }
      >
        <div className="flex-col gap-12">
          <div className="form-group">
            <label>适用范围</label>
            <select className="form-control" value={ruleForm.scope} onChange={(event) => setRuleForm((prev) => ({ ...prev, scope: event.target.value }))}>
              <option value="all_maps">全部地图默认阈值</option>
              <option value="single_map">单独指定地图</option>
            </select>
            <div className="form-hint mt-4">单地图规则的优先级高于全部地图默认规则。</div>
          </div>
          {ruleForm.scope === 'single_map' ? (
            <div className="form-group">
              <label>地图名称</label>
              <input className="form-control" value={ruleForm.map_name} onChange={(event) => setRuleForm((prev) => ({ ...prev, map_name: event.target.value }))} placeholder="kz_cargo" />
            </div>
          ) : (
            <div className="alert alert-info" style={{ marginBottom: 0 }}>保存后，没有单独规则的地图都会使用此阈值。</div>
          )}
          <div style={{ display: 'grid', gridTemplateColumns: 'repeat(3, minmax(0, 1fr))', gap: 12 }}>
            <div className="form-group">
              <label>Course</label>
              <input className="form-control" type="number" min="0" value={ruleForm.course} onChange={(event) => setRuleForm((prev) => ({ ...prev, course: event.target.value }))} />
            </div>
            <div className="form-group">
              <label>模式</label>
              <select className="form-control" value={ruleForm.mode} onChange={(event) => setRuleForm((prev) => ({ ...prev, mode: event.target.value }))}>
                {MODE_OPTIONS.map((opt) => <option key={opt.value || 'all'} value={opt.value}>{opt.label}</option>)}
              </select>
            </div>
            <div className="form-group">
              <label>类型</label>
              <select className="form-control" value={ruleForm.time_type} onChange={(event) => setRuleForm((prev) => ({ ...prev, time_type: event.target.value }))}>
                {TIME_TYPE_OPTIONS.map((opt) => <option key={opt.value || 'all'} value={opt.value}>{opt.label}</option>)}
              </select>
            </div>
          </div>
          <div className="form-group">
            <label>异常阈值时间</label>
            <div style={{ display: 'grid', gridTemplateColumns: 'minmax(0, 1fr) minmax(0, 1fr)', gap: 12 }}>
              <div>
                <input className="form-control" type="number" min="0" max="1440" step="1" value={ruleForm.threshold_minutes} onChange={(event) => setRuleForm((prev) => ({ ...prev, threshold_minutes: event.target.value }))} />
                <div className="form-hint mt-4">分钟</div>
              </div>
              <div>
                <input className="form-control" type="number" min="0" max="59.99" step="0.01" value={ruleForm.threshold_seconds} onChange={(event) => setRuleForm((prev) => ({ ...prev, threshold_seconds: event.target.value }))} />
                <div className="form-hint mt-4">秒（可填写小数）</div>
              </div>
            </div>
          </div>
          <div className="form-group">
            <label style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
              <input type="checkbox" checked={ruleForm.enabled} onChange={(event) => setRuleForm((prev) => ({ ...prev, enabled: event.target.checked }))} />
              启用规则
            </label>
          </div>
          <div className="form-group" style={{ marginBottom: 0 }}>
            <label>备注</label>
            <textarea className="form-control" rows={3} value={ruleForm.note} onChange={(event) => setRuleForm((prev) => ({ ...prev, note: event.target.value }))} />
          </div>
        </div>
      </Modal>

      {dialog}
    </div>
  );
}

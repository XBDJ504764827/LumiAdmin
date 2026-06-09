import React, { useEffect, useMemo, useState } from 'react';
import { api } from '../../lib/api.js';
import { useAuth } from '../../state/auth.jsx';
import { useToast, ToastContainer } from '../../shared/Toast.jsx';
import { StatusPill } from '../../shared/StatusPill.jsx';
import { FileItem, fileActionLabel } from '../../shared/FilePreview.jsx';
import { formatChinaDateTime } from '../../shared/time.js';

const CATEGORY_FILTERS = [
  { value: 'all', label: '全部' },
  { value: 'ban', label: '封禁' },
  { value: 'whitelist', label: '白名单' },
  { value: 'appeal', label: '申诉' },
  { value: 'report', label: '举报' },
  { value: 'online', label: '在线' },
  { value: 'admin', label: '操作' },
  { value: 'audit', label: '审计' },
  { value: 'evidence', label: '证据' },
];

const CATEGORY_LABELS = {
  ban: '封禁',
  whitelist: '白名单',
  appeal: '申诉',
  report: '举报',
  online: '在线',
  admin: '操作',
  audit: '审计',
  evidence: '证据',
};

const STATUS_LABELS = {
  active: '生效中',
  inactive: '已失效',
  pending: '待审核',
  approved: '已通过',
  rejected: '已驳回',
  revoked: '已撤销',
  online: '在线',
  success: '成功',
  failed: '失败',
  image: '图片',
  video: '录像',
  audio: '录音',
  other: '文件',
};

const SOURCE_LABELS = {
  public: '公开提交',
  manual: '手动',
  web: '网站',
  game_plugin: '游戏插件',
  offline_sync: '离线同步',
  server_online_players: '在线上报',
  admin_logs: '管理日志',
  ban: '封禁',
  appeal: '申诉',
  report: '举报',
};

function statusKind(status, category) {
  if (status === 'active' || category === 'ban') return status === 'inactive' ? 'offline' : 'danger';
  if (status === 'online' || status === 'approved' || status === 'success') return 'success';
  if (status === 'pending') return 'warning';
  if (status === 'failed' || status === 'rejected') return 'danger';
  if (status === 'revoked' || status === 'inactive') return 'offline';
  if (category === 'audit') return status === 'failed' ? 'danger' : 'success';
  return 'default';
}

function statusLabel(status) {
  return STATUS_LABELS[status] || status || '-';
}

function sourceLabel(source) {
  return SOURCE_LABELS[source] || source || '-';
}

function tagsToText(tags = []) {
  return tags.join(', ');
}

function textToTags(value) {
  return value
    .split(/[,，\n]/)
    .map((item) => item.trim().replace(/^#/, ''))
    .filter(Boolean)
    .filter((item, index, arr) => arr.findIndex((next) => next.toLowerCase() === item.toLowerCase()) === index);
}

function durationLabel(minutes) {
  const value = Number(minutes);
  if (!Number.isFinite(value)) return '-';
  if (value === 0) return '永久';
  if (value < 60) return `${value} 分钟`;
  if (value % 1440 === 0) return `${value / 1440} 天`;
  if (value % 60 === 0) return `${value / 60} 小时`;
  return `${value} 分钟`;
}

function displayName(detail) {
  return detail?.profile?.display_name || detail?.profile?.steamid64 || '未选择玩家';
}

function shortDescription(text, max = 90) {
  if (!text) return '';
  return text.length > max ? `${text.slice(0, max)}...` : text;
}

function Metric({ label, value, sub }) {
  return (
    <div className="player-metric">
      <div className="player-metric-label">{label}</div>
      <div className="player-metric-value">{value}</div>
      {sub ? <div className="player-metric-sub">{sub}</div> : null}
    </div>
  );
}

function EmptyBlock({ children }) {
  return <div className="player-detail-empty">{children}</div>;
}

function Timeline({ items }) {
  if (items.length === 0) {
    return <EmptyBlock>暂无匹配时间线。</EmptyBlock>;
  }

  return (
    <div className="player-timeline">
      {items.map((item, index) => (
        <div className="player-timeline-item" key={`${item.event_type}-${item.related_id || index}-${item.occurred_at}`}>
          <div className={`player-timeline-dot player-timeline-dot-${item.category}`} />
          <div className="player-timeline-body">
            <div className="player-timeline-head">
              <div>
                <div className="player-timeline-title">{item.title}</div>
                <div className="player-timeline-time">{formatChinaDateTime(item.occurred_at, { seconds: false })}</div>
              </div>
              <div className="player-timeline-tags">
                <span className="player-category-tag">{CATEGORY_LABELS[item.category] || item.category}</span>
                {item.status ? (
                  <StatusPill kind={statusKind(item.status, item.category)}>{statusLabel(item.status)}</StatusPill>
                ) : null}
              </div>
            </div>
            {item.description ? <div className="player-timeline-desc">{item.description}</div> : null}
            <div className="player-timeline-meta">
              {item.actor ? <span>操作人: {item.actor}</span> : null}
              {item.source ? <span>来源: {sourceLabel(item.source)}</span> : null}
            </div>
          </div>
        </div>
      ))}
    </div>
  );
}

function WhitelistPanel({ records }) {
  const latest = records[0];
  if (!latest) return <EmptyBlock>暂无白名单记录。</EmptyBlock>;

  return (
    <div className="player-side-stack">
      <div className="player-side-row">
        <span>状态</span>
        <StatusPill kind={statusKind(latest.status, 'whitelist')}>{statusLabel(latest.status)}</StatusPill>
      </div>
      <div className="player-side-row"><span>昵称</span><strong>{latest.nickname}</strong></div>
      <div className="player-side-row"><span>申请时间</span><strong>{formatChinaDateTime(latest.applied_at, { seconds: false })}</strong></div>
      <div className="player-side-row"><span>审核人</span><strong>{latest.approved_by || latest.rejected_by || latest.revoked_by || '-'}</strong></div>
      {(latest.approval_reason || latest.rejection_reason) ? (
        <div className="player-detail-note">{latest.approval_reason || latest.rejection_reason}</div>
      ) : null}
    </div>
  );
}

function OnlinePanel({ records }) {
  if (records.length === 0) return <EmptyBlock>暂无当前在线上报。</EmptyBlock>;

  return (
    <div className="player-online-list">
      {records.map((item) => (
        <div className="player-online-item" key={`${item.server_id}-${item.reported_at}`}>
          <div className="player-online-head">
            <strong>{item.server_name}</strong>
            <StatusPill kind="success">在线</StatusPill>
          </div>
          <div className="player-online-meta">
            <span>{item.current_map || '-'}</span>
            <span>Ping {item.ping}</span>
            <span>{formatChinaDateTime(item.reported_at, { seconds: false })}</span>
          </div>
          <div className="player-online-meta">
            <span>{item.name}</span>
            <span>{item.ip}</span>
          </div>
        </div>
      ))}
    </div>
  );
}

function InternalProfilePanel({ profile, canEdit, saving, onSave }) {
  const [note, setNote] = useState('');
  const [tagsText, setTagsText] = useState('');

  useEffect(() => {
    React.startTransition(() => {
      setNote(profile?.note ?? '');
      setTagsText(tagsToText(profile?.tags ?? []));
    });
  }, [profile]);

  if (!canEdit) {
    if (!profile || (!profile.note && (!profile.tags || profile.tags.length === 0))) {
      return <EmptyBlock>暂无内部备注。</EmptyBlock>;
    }
    return (
      <div className="player-side-stack">
        {profile.tags?.length ? (
          <div className="player-tag-list">
            {profile.tags.map((tag) => <span className="player-tag" key={tag}>{tag}</span>)}
          </div>
        ) : null}
        {profile.note ? <div className="player-detail-note">{profile.note}</div> : null}
        <div className="player-detail-muted">
          {profile.updated_by ? `${profile.updated_by} · ` : ''}{formatChinaDateTime(profile.updated_at, { seconds: false })}
        </div>
      </div>
    );
  }

  return (
    <div className="player-internal-editor">
      <textarea
        className="form-control player-internal-note"
        value={note}
        onChange={(event) => setNote(event.target.value)}
        placeholder="内部备注"
        rows={4}
      />
      <input
        className="form-control"
        value={tagsText}
        onChange={(event) => setTagsText(event.target.value)}
        placeholder="标签，用逗号分隔"
      />
      <button
        className="btn btn-primary"
        type="button"
        disabled={saving}
        onClick={() => onSave({ note, tags: textToTags(tagsText) })}
      >
        {saving ? '保存中...' : '保存备注'}
      </button>
    </div>
  );
}

function EvidenceEditor({ file, onSave, saving }) {
  const [editing, setEditing] = useState(false);
  const [note, setNote] = useState(file.note || '');
  const [tagsText, setTagsText] = useState(tagsToText(file.tags ?? []));

  useEffect(() => {
    React.startTransition(() => {
      setNote(file.note || '');
      setTagsText(tagsToText(file.tags ?? []));
    });
  }, [file]);

  if (!editing) {
    return (
      <button className="action-btn" type="button" onClick={() => setEditing(true)}>
        编辑
      </button>
    );
  }

  return (
    <div className="player-evidence-editor">
      <input
        className="form-control"
        value={tagsText}
        onChange={(event) => setTagsText(event.target.value)}
        placeholder="标签，用逗号分隔"
      />
      <textarea
        className="form-control"
        value={note}
        onChange={(event) => setNote(event.target.value)}
        placeholder="证据备注"
        rows={2}
      />
      <div className="player-evidence-editor-actions">
        <button
          className="action-btn"
          type="button"
          disabled={saving}
          onClick={async () => {
            const saved = await onSave(file, { note, tags: textToTags(tagsText) });
            if (saved !== false) setEditing(false);
          }}
        >
          {saving ? '保存中...' : '保存'}
        </button>
        <button className="action-btn" type="button" disabled={saving} onClick={() => setEditing(false)}>
          取消
        </button>
      </div>
    </div>
  );
}

function EvidencePanel({ files, typeFilter, tagFilter, onTypeFilterChange, onTagFilterChange, onSave, savingFileId, canEdit }) {
  const allTags = useMemo(() => {
    const tags = new Set();
    files.forEach((file) => (file.tags ?? []).forEach((tag) => tags.add(tag)));
    return Array.from(tags).sort((a, b) => a.localeCompare(b));
  }, [files]);

  const filteredFiles = useMemo(() => files.filter((file) => {
    if (typeFilter !== 'all' && file.category !== typeFilter && file.source_type !== typeFilter) return false;
    if (tagFilter !== 'all' && !(file.tags ?? []).includes(tagFilter)) return false;
    return true;
  }), [files, typeFilter, tagFilter]);

  if (files.length === 0) return <EmptyBlock>暂无附件证据。</EmptyBlock>;

  return (
    <div className="player-evidence-list">
      <div className="player-evidence-filters">
        <select className="filter-select" value={typeFilter} onChange={(event) => onTypeFilterChange(event.target.value)}>
          <option value="all">全部证据</option>
          <option value="video">录像</option>
          <option value="image">图片</option>
          <option value="audio">录音</option>
          <option value="ban">封禁证据</option>
          <option value="appeal">申诉证据</option>
          <option value="report">举报证据</option>
        </select>
        <select className="filter-select" value={tagFilter} onChange={(event) => onTagFilterChange(event.target.value)}>
          <option value="all">全部标签</option>
          {allTags.map((tag) => <option value={tag} key={tag}>{tag}</option>)}
        </select>
      </div>
      {filteredFiles.slice(0, 10).map((file) => (
        <FileItem key={`${file.source_type}-${file.id}`} file={file}>
          <div className="player-evidence-side">
            <div className="player-evidence-actions">
              {file.url ? (
                <a className="action-btn" href={file.url} target="_blank" rel="noopener noreferrer">
                  {fileActionLabel(file.category)}
                </a>
              ) : (
                <span className="ban-file-unavailable">文件服务未配置</span>
              )}
              {canEdit ? (
                <EvidenceEditor
                  file={file}
                  onSave={onSave}
                  saving={savingFileId === file.id}
                />
              ) : null}
            </div>
            {(file.tags?.length || file.note) ? (
              <div className="player-evidence-meta">
                {file.tags?.length ? (
                  <div className="player-tag-list">
                    {file.tags.map((tag) => <span className="player-tag" key={tag}>{tag}</span>)}
                  </div>
                ) : null}
                {file.note ? <div className="player-evidence-note">{file.note}</div> : null}
              </div>
            ) : null}
          </div>
        </FileItem>
      ))}
      {filteredFiles.length === 0 ? <EmptyBlock>没有符合筛选条件的证据。</EmptyBlock> : null}
      {filteredFiles.length > 10 ? <div className="player-detail-muted">还有 {filteredFiles.length - 10} 个附件未显示。</div> : null}
    </div>
  );
}

function BansTable({ items }) {
  if (items.length === 0) return <EmptyBlock>暂无封禁历史。</EmptyBlock>;
  return (
    <div className="table-responsive">
      <table className="data-table player-record-table">
        <thead>
          <tr>
            <th>状态</th>
            <th>原因</th>
            <th>时长</th>
            <th>服务器</th>
            <th>操作人</th>
            <th>创建时间</th>
            <th>解封时间</th>
          </tr>
        </thead>
        <tbody>
          {items.map((item) => (
            <tr key={item.id}>
              <td><StatusPill kind={statusKind(item.status, 'ban')}>{statusLabel(item.status)}</StatusPill></td>
              <td className="text-ellipsis text-ellipsis-280" title={item.reason}>{item.reason}</td>
              <td>{durationLabel(item.duration_minutes)}</td>
              <td>{item.server_name || '-'}</td>
              <td>{item.operator_name || '-'}</td>
              <td>{formatChinaDateTime(item.created_at, { seconds: false })}</td>
              <td>{item.removed_at ? formatChinaDateTime(item.removed_at, { seconds: false }) : '-'}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function AppealsTable({ items }) {
  if (items.length === 0) return <EmptyBlock>暂无申诉记录。</EmptyBlock>;
  return (
    <div className="table-responsive">
      <table className="data-table player-record-table">
        <thead>
          <tr>
            <th>状态</th>
            <th>申诉理由</th>
            <th>关联封禁</th>
            <th>审核人</th>
            <th>提交时间</th>
            <th>处理时间</th>
          </tr>
        </thead>
        <tbody>
          {items.map((item) => (
            <tr key={item.id}>
              <td><StatusPill kind={statusKind(item.status, 'appeal')}>{statusLabel(item.status)}</StatusPill></td>
              <td className="text-ellipsis text-ellipsis-260" title={item.appeal_reason}>{item.appeal_reason}</td>
              <td className="text-ellipsis" style={{ maxWidth: 220 }} title={item.ban_reason || ''}>{item.ban_reason || '-'}</td>
              <td>{item.reviewed_by || '-'}</td>
              <td>{formatChinaDateTime(item.created_at, { seconds: false })}</td>
              <td>{item.reviewed_at ? formatChinaDateTime(item.reviewed_at, { seconds: false }) : '-'}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function ReportsTable({ items }) {
  if (items.length === 0) return <EmptyBlock>暂无举报记录。</EmptyBlock>;
  return (
    <div className="table-responsive">
      <table className="data-table player-record-table">
        <thead>
          <tr>
            <th>状态</th>
            <th>举报理由</th>
            <th>联系方式</th>
            <th>审核人</th>
            <th>提交时间</th>
            <th>处理时间</th>
          </tr>
        </thead>
        <tbody>
          {items.map((item) => (
            <tr key={item.id}>
              <td><StatusPill kind={statusKind(item.status, 'report')}>{statusLabel(item.status)}</StatusPill></td>
              <td className="text-ellipsis" style={{ maxWidth: 300 }} title={item.report_reason}>{item.report_reason}</td>
              <td className="text-ellipsis" style={{ maxWidth: 180 }} title={item.reporter_contact || ''}>{item.reporter_contact || '-'}</td>
              <td>{item.reviewed_by || '-'}</td>
              <td>{formatChinaDateTime(item.created_at, { seconds: false })}</td>
              <td>{item.reviewed_at ? formatChinaDateTime(item.reviewed_at, { seconds: false }) : '-'}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function AdminActionsTable({ adminActions, auditLogs }) {
  const rows = [
    ...adminActions.map((item) => ({
      key: `admin-${item.module}-${item.action}-${item.created_at}`,
      type: '管理日志',
      title: `${item.module} / ${item.action}`,
      detail: item.target_detail,
      actor: item.operator_name,
      source: 'admin_logs',
      status: '',
      time: item.created_at,
    })),
    ...auditLogs.map((item) => ({
      key: `audit-${item.id}`,
      type: '审计日志',
      title: item.operation,
      detail: item.message || item.reason || item.player_name || item.target,
      actor: item.operator_name,
      source: item.source,
      status: item.success ? 'success' : 'failed',
      time: item.created_at,
    })),
  ].sort((a, b) => new Date(b.time).getTime() - new Date(a.time).getTime());

  if (rows.length === 0) return <EmptyBlock>暂无管理员操作历史。</EmptyBlock>;

  return (
    <div className="table-responsive">
      <table className="data-table player-record-table">
        <thead>
          <tr>
            <th>类型</th>
            <th>操作</th>
            <th>详情</th>
            <th>操作人</th>
            <th>来源</th>
            <th>时间</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((item) => (
            <tr key={item.key}>
              <td>{item.type}</td>
              <td>{item.title}</td>
              <td className="text-ellipsis" style={{ maxWidth: 320 }} title={item.detail}>{shortDescription(item.detail)}</td>
              <td>{item.actor || '-'}</td>
              <td>
                {item.status ? (
                  <StatusPill kind={statusKind(item.status, 'audit')}>{sourceLabel(item.source)}</StatusPill>
                ) : sourceLabel(item.source)}
              </td>
              <td>{formatChinaDateTime(item.time, { seconds: false })}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

export function PlayerDetailPage() {
  const { session } = useAuth();
  const { toast, toasts, dismiss: dismissToast } = useToast();
  const token = session?.token ?? null;
  const [steamInput, setSteamInput] = useState('');
  const [lastQuery, setLastQuery] = useState('');
  const [detail, setDetail] = useState(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState('');
  const [category, setCategory] = useState('all');
  const [internalSaving, setInternalSaving] = useState(false);
  const [evidenceTypeFilter, setEvidenceTypeFilter] = useState('all');
  const [evidenceTagFilter, setEvidenceTagFilter] = useState('all');
  const [savingEvidenceId, setSavingEvidenceId] = useState(null);
  const canManageInternal = session?.role === 'developer' || session?.role === 'admin';

  async function loadDetail(input) {
    const query = input.trim();
    if (!query) {
      setError('请输入 SteamID。');
      return;
    }
    try {
      setLoading(true);
      setError('');
      const result = await api.playerDetail(token, query);
      setDetail(result.data);
      setLastQuery(query);
      setCategory('all');
      setEvidenceTypeFilter('all');
      setEvidenceTagFilter('all');
    } catch (requestError) {
      setDetail(null);
      setError(requestError.message || '加载玩家详情失败');
    } finally {
      setLoading(false);
    }
  }

  async function handleSaveInternalProfile(body) {
    if (!detail) return;
    try {
      setInternalSaving(true);
      setError('');
      const result = await api.updatePlayerInternalProfile(token, detail.profile.steamid64, body);
      setDetail((prev) => prev ? { ...prev, internal_profile: result.item } : prev);
      toast({ title: '保存成功', message: '内部备注已更新。' });
    } catch (requestError) {
      toast({ title: '保存失败', message: requestError.message || '保存内部备注失败', tone: 'danger' });
    } finally {
      setInternalSaving(false);
    }
  }

  async function handleSaveEvidence(file, body) {
    try {
      setSavingEvidenceId(file.id);
      setError('');
      const result = await api.updateEvidenceMetadata(token, file.source_type, file.id, body);
      setDetail((prev) => {
        if (!prev) return prev;
        const evidenceFiles = prev.evidence_files.map((item) => (
          item.id === file.id && item.source_type === file.source_type
            ? { ...item, tags: result.item.tags ?? [], note: result.item.note ?? '' }
            : item
        ));
        return { ...prev, evidence_files: evidenceFiles };
      });
      return true;
    } catch (requestError) {
      setError(requestError.message || '保存证据元数据失败');
      return false;
    } finally {
      setSavingEvidenceId(null);
    }
  }

  function handleSubmit(event) {
    event.preventDefault();
    loadDetail(steamInput);
  }

  const filteredTimeline = useMemo(() => {
    const items = detail?.timeline ?? [];
    if (category === 'all') return items;
    return items.filter((item) => item.category === category);
  }, [detail, category]);

  const activeBanText = detail ? `${detail.summary.active_ban_count}/${detail.summary.ban_count}` : '-';
  const playerName = displayName(detail);

  return (
    <div id="player-detail" className="content-section active">
      <div className="breadcrumb"><span>核心管理</span><span className="sep">›</span><span className="current">玩家详情</span></div>
      <div className="page-header">
        <div>
          <div className="page-title">玩家详情</div>
          <div className="page-sub">按 SteamID 聚合封禁、白名单、申诉、举报、在线上报、审计和附件证据。</div>
        </div>
      </div>

      <div className="card player-search-card">
        <form className="player-detail-search" onSubmit={handleSubmit}>
          <input
            className="form-control"
            value={steamInput}
            onChange={(event) => setSteamInput(event.target.value)}
            placeholder="SteamID64 / STEAM_0 / [U:1] / Steam 主页链接"
          />
          <button className="btn btn-primary" type="submit" disabled={loading}>
            {loading ? '查询中...' : '查询'}
          </button>
          {lastQuery ? (
            <button className="btn btn-outline" type="button" disabled={loading} onClick={() => loadDetail(lastQuery)}>
              刷新
            </button>
          ) : null}
        </form>
        {error ? <div className="player-detail-error">{error}</div> : null}
      </div>

      {!detail && !loading ? (
        <div className="player-welcome-card">
          <div className="player-welcome-inner">
            <div className="player-welcome-icon">
              <svg viewBox="0 0 24 24" fill="none" stroke="#fff" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" width="28" height="28">
                <circle cx="9" cy="7" r="4" />
                <path d="M2 21v-2a4 4 0 0 1 4-4h6a4 4 0 0 1 4 4v2" />
                <path d="M16 3.13a4 4 0 0 1 0 7.75" />
                <path d="M21 21v-2a4 4 0 0 0-3-3.87" />
              </svg>
            </div>
            <div className="player-welcome-title">查询玩家信息</div>
            <div className="player-welcome-desc">在上方输入 SteamID，即可查看该玩家的封禁记录、白名单状态、申诉举报、在线记录和附件证据等完整信息。</div>
            <div className="player-welcome-formats">
              <div className="player-welcome-format-label">支持的输入格式</div>
              <div className="player-welcome-format-list">
                <span className="player-welcome-format-tag">
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" width="13" height="13"><polyline points="16 18 22 12 16 6" /><polyline points="8 6 2 12 8 18" /></svg>
                  SteamID64
                </span>
                <span className="player-welcome-format-tag">
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" width="13" height="13"><polyline points="16 18 22 12 16 6" /><polyline points="8 6 2 12 8 18" /></svg>
                  STEAM_0:0:xxxxx
                </span>
                <span className="player-welcome-format-tag">
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" width="13" height="13"><polyline points="16 18 22 12 16 6" /><polyline points="8 6 2 12 8 18" /></svg>
                  [U:1:xxxxx]
                </span>
                <span className="player-welcome-format-tag">
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" width="13" height="13"><polyline points="16 18 22 12 16 6" /><polyline points="8 6 2 12 8 18" /></svg>
                  Steam 个人主页链接
                </span>
              </div>
            </div>
            <div className="player-welcome-features">
              <div className="player-welcome-feature">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" width="18" height="18"><circle cx="12" cy="12" r="10" /><path d="M4.93 4.93l14.14 14.14" /></svg>
                <div className="player-welcome-feature-text">
                  <strong>封禁查询</strong>
                  <span>活跃/历史封禁记录</span>
                </div>
              </div>
              <div className="player-welcome-feature">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" width="18" height="18"><path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z" /></svg>
                <div className="player-welcome-feature-text">
                  <strong>白名单状态</strong>
                  <span>申请与审核状态</span>
                </div>
              </div>
              <div className="player-welcome-feature">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" width="18" height="18"><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" /><polyline points="14 2 14 8 20 8" /><line x1="16" y1="13" x2="8" y2="13" /><line x1="16" y1="17" x2="8" y2="17" /></svg>
                <div className="player-welcome-feature-text">
                  <strong>申诉举报</strong>
                  <span>完整的申诉与举报历史</span>
                </div>
              </div>
              <div className="player-welcome-feature">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" width="18" height="18"><rect x="2" y="3" width="20" height="14" rx="2" ry="2" /><line x1="8" y1="21" x2="16" y2="21" /><line x1="12" y1="17" x2="12" y2="21" /></svg>
                <div className="player-welcome-feature-text">
                  <strong>在线记录</strong>
                  <span>当前/最近在线上报</span>
                </div>
              </div>
            </div>
          </div>
        </div>
      ) : null}

      {detail ? (
        <>
          <section className="player-profile-panel">
            <div className="player-profile-main">
              <div className={`player-profile-avatar ${detail.summary.active_ban_count > 0 ? 'is-banned' : ''}`}>
                {playerName.slice(0, 2).toUpperCase()}
              </div>
              <div className="player-profile-info">
                <div className="player-profile-name">{playerName}</div>
                <div className="player-profile-ids">
                  <code>{detail.profile.steamid64}</code>
                  {detail.profile.steamid ? <code>{detail.profile.steamid}</code> : null}
                  {detail.profile.steamid3 ? <code>{detail.profile.steamid3}</code> : null}
                </div>
              </div>
            </div>
            <div className="player-profile-actions">
              {detail.profile.profile_url ? (
                <a className="btn btn-outline" href={detail.profile.profile_url} target="_blank" rel="noopener noreferrer">Steam 主页</a>
              ) : null}
              <StatusPill kind={detail.summary.active_ban_count > 0 ? 'danger' : 'success'}>
                {detail.summary.active_ban_count > 0 ? '存在活跃封禁' : '无活跃封禁'}
              </StatusPill>
            </div>
          </section>

          <div className="player-detail-metrics">
            <Metric label="封禁" value={activeBanText} sub="活跃 / 全部" />
            <Metric label="白名单" value={statusLabel(detail.summary.whitelist_status)} />
            <Metric label="申诉" value={detail.summary.appeal_count} />
            <Metric label="举报" value={detail.summary.report_count} />
            <Metric label="在线记录" value={detail.summary.online_server_count} sub={detail.summary.last_seen_at ? formatChinaDateTime(detail.summary.last_seen_at, { seconds: false }) : '无上报'} />
            <Metric label="附件证据" value={detail.summary.evidence_file_count} />
            <Metric label="操作日志" value={detail.summary.admin_action_count} />
          </div>

          <div className="player-detail-layout">
            <section className="card player-timeline-card">
              <div className="card-header">
                <div>
                  <div className="card-title">事件时间线</div>
                  <div className="card-sub">{filteredTimeline.length} 条记录</div>
                </div>
              </div>
              <div className="card-body">
                <div className="tabs player-detail-tabs">
                  {CATEGORY_FILTERS.map((filter) => (
                    <button
                      key={filter.value}
                      className={`tab ${category === filter.value ? 'active' : ''}`}
                      onClick={() => setCategory(filter.value)}
                      type="button"
                    >
                      {filter.label}
                    </button>
                  ))}
                </div>
                <Timeline items={filteredTimeline} />
              </div>
            </section>

            <aside className="player-detail-side">
              <div className="card">
                <div className="card-header">
                  <div>
                    <div className="card-title">内部备注</div>
                    <div className="card-sub">后台可见</div>
                  </div>
                </div>
                <div className="card-body">
                  <InternalProfilePanel
                    profile={detail.internal_profile}
                    canEdit={canManageInternal}
                    saving={internalSaving}
                    onSave={handleSaveInternalProfile}
                  />
                </div>
              </div>
              <div className="card">
                <div className="card-header">
                  <div>
                    <div className="card-title">白名单状态</div>
                    <div className="card-sub">{detail.whitelist.length} 条记录</div>
                  </div>
                </div>
                <div className="card-body"><WhitelistPanel records={detail.whitelist} /></div>
              </div>
              <div className="card">
                <div className="card-header">
                  <div>
                    <div className="card-title">当前在线</div>
                    <div className="card-sub">当前/最近上报</div>
                  </div>
                </div>
                <div className="card-body"><OnlinePanel records={detail.online_records} /></div>
              </div>
              <div className="card">
                <div className="card-header">
                  <div>
                    <div className="card-title">附件证据</div>
                    <div className="card-sub">{detail.evidence_files.length} 个文件</div>
                  </div>
                </div>
                <div className="card-body">
                  <EvidencePanel
                    files={detail.evidence_files}
                    typeFilter={evidenceTypeFilter}
                    tagFilter={evidenceTagFilter}
                    onTypeFilterChange={setEvidenceTypeFilter}
                    onTagFilterChange={setEvidenceTagFilter}
                    onSave={handleSaveEvidence}
                    savingFileId={savingEvidenceId}
                    canEdit={canManageInternal}
                  />
                </div>
              </div>
            </aside>
          </div>

          <div className="player-records-grid">
            <section className="card">
              <div className="card-header">
                <div>
                  <div className="card-title">封禁历史</div>
                  <div className="card-sub">{detail.bans.length} 条记录</div>
                </div>
              </div>
              <div className="card-body p-0"><BansTable items={detail.bans} /></div>
            </section>
            <section className="card">
              <div className="card-header">
                <div>
                  <div className="card-title">申诉记录</div>
                  <div className="card-sub">{detail.appeals.length} 条记录</div>
                </div>
              </div>
              <div className="card-body p-0"><AppealsTable items={detail.appeals} /></div>
            </section>
            <section className="card">
              <div className="card-header">
                <div>
                  <div className="card-title">举报记录</div>
                  <div className="card-sub">{detail.reports.length} 条记录</div>
                </div>
              </div>
              <div className="card-body p-0"><ReportsTable items={detail.reports} /></div>
            </section>
            <section className="card">
              <div className="card-header">
                <div>
                  <div className="card-title">管理员操作历史</div>
                  <div className="card-sub">{detail.admin_actions.length + detail.audit_logs.length} 条记录</div>
                </div>
              </div>
              <div className="card-body p-0"><AdminActionsTable adminActions={detail.admin_actions} auditLogs={detail.audit_logs} /></div>
            </section>
          </div>
        </>
      ) : null}
      <ToastContainer toasts={toasts} onDismiss={dismissToast} />
    </div>
  );
}

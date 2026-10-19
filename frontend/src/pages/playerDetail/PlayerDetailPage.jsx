import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { api } from '../../lib/api.js';
import { useAuth } from '../../state/store.js';
import { useToast } from '../../shared/Toast.jsx';
import { useConfirmDialog } from '../../shared/ConfirmModal.jsx';
import { BanFormModal } from '../ban/BanFormModal.jsx';
import { StatusPill } from '../../shared/StatusPill.jsx';
import { Modal } from '../../shared/Modal.jsx';
import { formatChinaDate, formatChinaDateTime } from '../../shared/time.js';
import { QqBindingPanel } from '../../shared/QqBindingPanel.jsx';
import {
  AXIS_LANE_ROW_HEIGHT,
  AXIS_RANGES,
  buildBounds,
  buildDomain,
  buildLanes,
  buildSingleLane,
  buildTicks,
  formatTick,
} from './timelineAxis.js';
import {
  actorFieldLabel,
  candidateMeta,
  categoryIconLabel,
  categoryKind,
  categoryLabel,
  categoryTone,
  count,
  countActiveGlobalBans,
  durLabel,
  eventSourceLabel,
  failureLabel,
  latestWhitelistContact,
  methodLabel,
  riskLabel,
  riskTone,
  sessionDurationLabel,
  sessionEndLabel,
  sessionReasonKind,
  sessionReasonLabel,
  stKind,
  stLabel,
  tagsToText,
  textToTags,
} from './playerDetailFormat.js';

function Empty({ children }) { return <div className="player-detail-empty">{children}</div>; }

const WEEKDAY_FORMATTER = new Intl.DateTimeFormat('zh-CN', {
  timeZone: 'Asia/Shanghai',
  weekday: 'short',
});

const TIME_FORMATTER = new Intl.DateTimeFormat('zh-CN', {
  timeZone: 'Asia/Shanghai',
  hour12: false,
  hour: '2-digit',
  minute: '2-digit',
  second: '2-digit',
});

function chinaWeekday(value) {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return '';
  return WEEKDAY_FORMATTER.format(date);
}

/** 相对时间：刚刚 / N 分钟前 / N 小时前 / N 天前 / 日期 */
function relativeTime(value) {
  if (!value) return '-';
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return String(value);
  const diff = Date.now() - date.getTime();
  if (diff < 0) return formatChinaDateTime(value, { seconds: false });
  const minutes = Math.floor(diff / 60000);
  if (minutes < 1) return '刚刚';
  if (minutes < 60) return `${minutes} 分钟前`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours} 小时前`;
  const days = Math.floor(hours / 24);
  if (days < 30) return `${days} 天前`;
  return formatChinaDateTime(value, { seconds: false });
}

function minutesLabel(minutes) {
  const value = Number(minutes) || 0;
  if (value <= 0) return '0 分钟';
  if (value < 60) return `${value} 分钟`;
  const hours = value / 60;
  if (hours < 24) return `${hours.toFixed(hours >= 10 ? 0 : 1)} 小时`;
  const days = Math.floor(hours / 24);
  const rest = Math.round(hours % 24);
  return rest ? `${days} 天 ${rest} 小时` : `${days} 天`;
}

/** 复制文本：优先 Clipboard API，非安全上下文（局域网 http）回退到 execCommand。 */
async function copyText(value) {
  const text = String(value ?? '');
  if (!text) return false;
  try {
    if (navigator.clipboard && window.isSecureContext) {
      await navigator.clipboard.writeText(text);
      return true;
    }
  } catch { /* 回退 */ }
  try {
    const area = document.createElement('textarea');
    area.value = text;
    area.style.position = 'fixed';
    area.style.opacity = '0';
    document.body.appendChild(area);
    area.select();
    const ok = document.execCommand('copy');
    document.body.removeChild(area);
    return ok;
  } catch {
    return false;
  }
}

function CopyId({ label, value }) {
  const { toast } = useToast();
  if (!value) {
    return (
      <div className="pd-id-cell">
        <span className="pd-id-label">{label}</span>
        <code className="pd-id-value is-empty">未获取</code>
      </div>
    );
  }
  return (
    <div className="pd-id-cell">
      <span className="pd-id-label">{label}</span>
      <div className="pd-id-value-row">
        <code className="pd-id-value">{value}</code>
        <button
          type="button"
          className="pd-copy-btn"
          title={`复制 ${label}`}
          aria-label={`复制 ${label}`}
          onClick={async () => {
            const ok = await copyText(value);
            toast(ok
              ? { title: `${label} 已复制` }
              : { title: '复制失败', message: '请手动选择文本复制', tone: 'warning' });
          }}
        >
          <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
            <rect x="9" y="9" width="12" height="12" rx="2" />
            <path d="M5 15V5a2 2 0 0 1 2-2h10" />
          </svg>
        </button>
      </div>
    </div>
  );
}

// ── Ban Detail Popup ──
function BanDetailPopup({ item, onClose, onAction, token }) {
  const { toast } = useToast();
  const { confirm, dialog } = useConfirmDialog();
  const [acting, setActing] = useState(false);
  if (!item) return null;
  const isActive = item.status === 'active';

  async function doUnban() { if (acting) return; setActing(true); try { await api.unban(token, item.id); toast({ title: '解封成功' }); onAction(); onClose(); } catch (e) { toast({ title: '操作失败', message: e.message, tone: 'danger' }); } finally { setActing(false); } }
  async function doDelete() { if (acting) return; const ok = await confirm({ title: '删除封禁记录', message: '确定删除这条封禁记录吗？此操作不可恢复。', tone: 'danger', confirmText: '确认删除' }); if (!ok) return; setActing(true); try { await api.deleteBan(token, item.id); toast({ title: '删除成功' }); onAction(); onClose(); } catch (e) { toast({ title: '操作失败', message: e.message, tone: 'danger' }); } finally { setActing(false); } }

  return (<>
    <Modal open={true} title="封禁详情" onClose={onClose} footer={<><button className="btn btn-outline" onClick={onClose}>关闭</button>{isActive && <><button className="action-btn action-btn-success" onClick={doUnban} disabled={acting}>解封</button><button className="action-btn action-btn-danger" onClick={doDelete} disabled={acting}>删除</button></>}</>}>
      <div className="detail-grid">
        <span className="detail-label">状态</span><span className="detail-value"><StatusPill kind={stKind(item.status, 'ban')}>{stLabel(item.status)}</StatusPill></span>
        <span className="detail-label">玩家</span><span className="detail-value fw-600">{item.player || '-'}</span>
        <span className="detail-label">SteamID64</span><span className="detail-value mono">{item.steam_id}</span>
        {item.ip_address && <><span className="detail-label">IP</span><span className="detail-value mono">{item.ip_address}</span></>}
        {item.server_name && <><span className="detail-label">服务器</span><span className="detail-value">{item.server_name}</span></>}
        <span className="detail-label">封禁类型</span><span className="detail-value">{item.ban_type === 'ip' ? 'IP 封禁' : 'Steam 账号封禁'}</span>
        <span className="detail-label">时长</span><span className="detail-value">{durLabel(item.duration_minutes)}</span>
        <span className="detail-label">理由</span><span className="detail-value pre">{item.reason}</span>
        <span className="detail-label">操作人</span><span className="detail-value">{item.operator_name || '-'}</span>
        <span className="detail-label">封禁时间</span><span className="detail-value">{formatChinaDateTime(item.created_at)}</span>
        {item.removed_at && <><span className="detail-label">解封时间</span><span className="detail-value">{formatChinaDateTime(item.removed_at)} {item.removed_by ? `(${item.removed_by})` : ''}</span></>}
      </div>
    </Modal>
    {dialog}
  </>);
}

// ── Whitelist Detail Popup ──
function WhitelistDetailPopup({ item, onClose, onAction, token }) {
  const { toast } = useToast();
  const [acting, setActing] = useState(false);
  const [reason, setReason] = useState('');
  if (!item) return null;
  const isPending = item.status === 'pending';
  const isApproved = item.status === 'approved';
  const isRejected = item.status === 'rejected' || item.status === 'revoked';

  async function doAction(action) {
    if (acting) return; setActing(true);
    try {
      if (action === 'reject') {
        if (!reason.trim()) { toast({ title: '请填写拒绝理由', tone: 'warning' }); setActing(false); return; }
        await api.rejectWhitelist(token, item.id, { reason: reason.trim() });
      } else if (action === 'approve') {
        await api.approveWhitelist(token, item.id);
      } else if (action === 'revoke') {
        await api.revokeWhitelist(token, item.id);
      } else if (action === 'restore') {
        await api.restoreWhitelist(token, item.id);
      }
      toast({ title: '操作成功' }); onAction(); onClose();
    } catch (e) { toast({ title: '操作失败', message: e.message, tone: 'danger' }); } finally { setActing(false); }
  }

  return (<Modal open={true} title="白名单详情" onClose={onClose} footer={<><button className="btn btn-outline" onClick={onClose}>关闭</button>
    {isPending && <><button className="action-btn action-btn-success" onClick={() => doAction('approve')} disabled={acting}>通过</button><button className="action-btn action-btn-danger" onClick={() => doAction('reject')} disabled={acting || !reason.trim()}>拒绝</button></>}
    {isApproved && <button className="action-btn action-btn-danger" onClick={() => doAction('revoke')} disabled={acting}>撤销</button>}
    {isRejected && <button className="action-btn action-btn-success" onClick={() => doAction('restore')} disabled={acting}>恢复</button>}
  </>}>
    {isPending && <div className="form-group" style={{ marginBottom: 12 }}><label>拒绝理由（必填）</label><textarea className="form-control" rows={2} value={reason} onChange={e => setReason(e.target.value)} placeholder="请输入拒绝理由" style={{ resize: 'vertical', minHeight: 50 }} /></div>}
    <div className="detail-grid">
      <span className="detail-label">状态</span><span className="detail-value"><StatusPill kind={stKind(item.status, 'whitelist')}>{stLabel(item.status)}</StatusPill></span>
      <span className="detail-label">昵称</span><span className="detail-value fw-600">{item.nickname}</span>
      {item.contact && <><span className="detail-label">联系方式</span><span className="detail-value pre">{item.contact}</span></>}
      {item.reason && <><span className="detail-label">申请理由</span><span className="detail-value pre">{item.reason}</span></>}
      <span className="detail-label">SteamID64</span><span className="detail-value mono">{item.steamid64}</span>
      {item.steam_persona_name && <><span className="detail-label">Steam 名称</span><span className="detail-value">{item.steam_persona_name}</span></>}
      <span className="detail-label">申请时间</span><span className="detail-value">{formatChinaDateTime(item.applied_at)}</span>
      {item.approved_at && <><span className="detail-label">通过时间</span><span className="detail-value">{formatChinaDateTime(item.approved_at)} ({item.approved_by || '-'})</span></>}
      {item.rejected_at && <><span className="detail-label">拒绝时间</span><span className="detail-value">{formatChinaDateTime(item.rejected_at)} ({item.rejected_by || '-'})<br />{item.rejection_reason || ''}</span></>}
      {item.revoked_at && <><span className="detail-label">撤销时间</span><span className="detail-value">{formatChinaDateTime(item.revoked_at)} ({item.revoked_by || '-'})</span></>}
      {item.approval_reason && <><span className="detail-label">通过理由</span><span className="detail-value pre">{item.approval_reason}</span></>}
    </div>
  </Modal>);
}

// ── Global Ban Popup ──
function GlobalBanPopup({ items, onClose }) {
  if (!items || items.length === 0) return null;
  function banTypeLabel(t) { const m = { bhop_hack: '连跳作弊', cheat: '作弊', tool_assist: '辅助工具', other: '其他' }; return m[t] || t || '-'; }
  function isPerma(e) { return e && e.startsWith('9999'); }
  function expLabel(e) { if (!e) return '永久'; if (isPerma(e)) return '永久'; return formatChinaDateTime(e, { seconds: false }); }
  return (<Modal open={true} title={`全球封禁详情 (${items.length} 条)`} onClose={onClose} wide footer={<button className="btn btn-outline" onClick={onClose}>关闭</button>}>
    {/* 堆叠卡片而非表格：弹窗宽度有限，表格在移动端会严重溢出 */}
    <div className="global-ban-list">
      {items.map(item => {
        const ban = item.ban; return (<div key={ban.id} className="global-ban-item">
          <div className="global-ban-item-header">
            <span className="global-ban-type">{banTypeLabel(ban.ban_type)}</span>
            {isPerma(ban.expires_on) ? <span className="global-ban-permanent">永久</span> : <span className="global-ban-temporary">{expLabel(ban.expires_on)}</span>}
          </div>
          <div className="global-ban-item-body">
            <div className="global-ban-field"><span className="global-ban-label">封禁时间</span><span className="global-ban-value">{formatChinaDateTime(ban.created_on, { seconds: false })}</span></div>
            {ban.notes && <div className="global-ban-field"><span className="global-ban-label">备注</span><span className="global-ban-value">{ban.notes}</span></div>}
            <div className="global-ban-field"><span className="global-ban-label">本地状态</span><span className="global-ban-value">{item.manual_unbanned ? '已解封' : item.local_ban_id ? '已封禁' : '未封禁'}</span></div>
          </div>
        </div>);
      })}
    </div>
  </Modal>);
}

const GOKZ_MODES = [
  { key: 'kzt', label: 'KZT', color: '#f59e0b' },
  { key: 'skz', label: 'SKZ', color: '#3b82f6' },
  { key: 'vnl', label: 'VNL', color: '#10b981' },
  { key: 'ovr', label: 'OVR', color: '#ec4899' },
];

/** GOKZ 战绩面板：有缓存数据才渲染，无数据时显示灰色提示行。 */
function GokzPanel({ gokz }) {
  if (!gokz) return <div className="player-gokz-empty">暂无 GOKZ 战绩缓存（玩家未在 KZ 服务器出没或缓存过期）。</div>;
  const modes = GOKZ_MODES.map(m => ({ ...m, data: gokz[m.key] })).filter(m => m.data);
  if (modes.length === 0) return <div className="player-gokz-empty">GOKZ 缓存存在但无各模式数据。</div>;
  return <div className="player-gokz-panel">
    {modes.map(({ key, label, color, data }) => <div className="player-gokz-mode" key={key} style={{ borderTopColor: color }}>
      <div className="player-gokz-mode-label" style={{ color }}>{label}</div>
      {data.rating != null ? <>
        <div className="player-gokz-rating">{Number(data.rating).toFixed(1)}</div>
        <div className="player-gokz-sub">{data.rank != null ? `排名 #${data.rank}` : '无排名'}{data.points != null ? ` · ${Number(data.points).toFixed(0)} 分` : ''}</div>
        {data.unique_map_finishes != null && <div className="player-gokz-sub">完图 {data.unique_map_finishes} 张</div>}
      </> : <div className="player-gokz-sub">暂无 Rating</div>}
    </div>)}
  </div>;
}

// ── 时间线（坐标轴 + 列表） ─────────────────────────────────────────────────

const TIMELINE_CATEGORY_ORDER = [
  'session', 'access', 'whitelist', 'ban', 'global_ban', 'abnormal',
  'qq', 'internal', 'online', 'admin', 'audit', 'evidence',
];

/** 无事件时的兜底时间参考（模块加载时取一次，避免渲染期调用非纯函数）。 */
const AXIS_FALLBACK_NOW = Date.now();

/** 事件详情弹窗：点击坐标轴上的点后展示完整信息。 */
function TimelineEventModal({ event, onClose }) {
  if (!event) return null;
  return <Modal open title="事件详情" onClose={onClose} wide footer={<button className="btn btn-outline" type="button" onClick={onClose}>关闭</button>}>
    <div className="pd-event-detail">
      <div className="pd-event-detail-head">
        <div className="pd-event-detail-title">{event.title}</div>
        <div className="pd-event-pills">
          <StatusPill kind={categoryKind(event.category, event.status)}>{categoryLabel(event.category)}</StatusPill>
          {event.status && <StatusPill kind={stKind(event.status)}>{stLabel(event.status)}</StatusPill>}
        </div>
      </div>
      <div className="pd-event-detail-time">{formatChinaDateTime(event.occurred_at)}（{relativeTime(event.occurred_at)}）</div>
      {event.description && <div className="pd-event-detail-desc">{event.description}</div>}
      {(event.meta?.length > 0 || event.server_name) && <div className="pd-event-meta">
        {event.server_name && <span className="pd-event-meta-item pd-event-server">{event.server_name}</span>}
        {(event.meta || []).map((meta, index) => <span className="pd-event-meta-item" key={`${meta.label}-${index}`}>
          <b>{meta.label}</b>{meta.value}
        </span>)}
      </div>}
      <div className="pd-event-detail-footer">
        <span>{actorFieldLabel(event.category)}：{event.actor || '系统 / 玩家本人'}</span>
        <span>来源：{eventSourceLabel(event.source)}</span>
        {event.related_id && <span className="mono">关联 ID：{event.related_id}</span>}
        <span>事件类型：<code className="mono">{event.event_type}</code></span>
      </div>
    </div>
  </Modal>;
}

/** 总览条：展示全量时间范围与事件分布。
 *  - 拖动窗口 = 平移
 *  - 在窗口外拖拽 = 框选时间范围（放大）
 *  - 单击 = 以点击位置为中心聚焦
 */
function TimelineOverview({ items, fullBounds, domain, onPanTo }) {
  const ref = useRef(null);
  const [drag, setDrag] = useState(null);
  const fullSpan = Math.max(fullBounds.max - fullBounds.min, 1);
  const windowStart = (domain.start - fullBounds.min) / fullSpan;
  const windowWidth = Math.max((domain.end - domain.start) / fullSpan, 0.008);

  useEffect(() => {
    if (!drag) return undefined;
    function handleMove(nativeEvent) {
      const node = ref.current;
      if (!node) return;
      const rect = node.getBoundingClientRect();
      const ratio = Math.min(Math.max((nativeEvent.clientX - rect.left) / Math.max(rect.width, 1), 0), 1);
      const time = fullBounds.min + ratio * fullSpan;
      if (drag.mode === 'pan') {
        const deltaRatio = (nativeEvent.clientX - drag.startX) / Math.max(rect.width, 1);
        const deltaTime = deltaRatio * fullSpan;
        onPanTo({ start: drag.domain.start + deltaTime, end: drag.domain.end + deltaTime });
        return;
      }
      // 框选：向两侧扩展，最小宽度约为总量程的 1%
      const minSpan = Math.max(fullSpan * 0.01, 10 * 60 * 1000);
      let start = Math.min(drag.startTime, time);
      let end = Math.max(drag.startTime, time);
      if (end - start < minSpan) end = start + minSpan;
      if (end > fullBounds.max) { end = fullBounds.max; start = Math.max(fullBounds.min, end - minSpan); }
      onPanTo({ start, end });
    }
    function handleUp(nativeEvent) {
      setDrag(null);
      if (drag.mode !== 'brush') return;
      const node = ref.current;
      if (!node) return;
      const rect = node.getBoundingClientRect();
      const movedPx = Math.abs(nativeEvent.clientX - drag.startX);
      if (movedPx > 6) return;
      // 单击：以点击位置为中心，聚焦到当前跨度的 35%（至少 10 分钟）
      const ratio = Math.min(Math.max((nativeEvent.clientX - rect.left) / Math.max(rect.width, 1), 0), 1);
      const time = fullBounds.min + ratio * fullSpan;
      const currentSpan = drag.domain.end - drag.domain.start;
      const span = Math.max(Math.min(currentSpan * 0.35, fullSpan), Math.max(fullSpan * 0.01, 10 * 60 * 1000));
      const center = Math.min(Math.max(time, fullBounds.min + span / 2), fullBounds.max - span / 2);
      onPanTo({ start: center - span / 2, end: center + span / 2 });
    }
    window.addEventListener('mousemove', handleMove);
    window.addEventListener('mouseup', handleUp);
    return () => {
      window.removeEventListener('mousemove', handleMove);
      window.removeEventListener('mouseup', handleUp);
    };
  }, [drag, fullBounds, fullSpan, onPanTo]);

  function beginDrag(nativeEvent) {
    const node = ref.current;
    if (!node) return;
    const rect = node.getBoundingClientRect();
    const ratio = Math.min(Math.max((nativeEvent.clientX - rect.left) / Math.max(rect.width, 1), 0), 1);
    const time = fullBounds.min + ratio * fullSpan;
    const inside = time >= domain.start && time <= domain.end && (domain.end - domain.start) < fullSpan * 0.98;
    setDrag({
      mode: inside ? 'pan' : 'brush',
      startX: nativeEvent.clientX,
      startTime: time,
      domain,
    });
    nativeEvent.preventDefault();
  }

  return <div
    className="pd-overview"
    ref={ref}
    onMouseDown={beginDrag}
    role="presentation"
    title="拖动窗口平移；在空白处拖拽框选范围放大；单击聚焦"
  >
    <div className="pd-overview-ticks">
      {items.map((item, index) => <span
        key={`${item.occurred_at}-${index}`}
        className={`pd-overview-tick tone-${categoryTone(item.category)}`}
        style={{ left: `${((new Date(item.occurred_at).getTime() - fullBounds.min) / fullSpan) * 100}%` }}
      />)}
      <span className="pd-overview-window" style={{ left: `${windowStart * 100}%`, width: `${windowWidth * 100}%` }} />
    </div>
    <div className="pd-overview-labels">
      <span>{formatChinaDate(fullBounds.min)}</span>
      <span>拖动窗口平移 · 空白处拖拽框选放大 · 单击聚焦</span>
      <span>{formatChinaDate(fullBounds.max)}</span>
    </div>
  </div>;
}

/** 坐标轴时间线：横轴为时间，事件在发生时间处画点。
 *  mode='lanes' 按分类分泳道；mode='single' 所有事件共用一条轴（自动分层避让）。 */
function TimelineAxis({ events, overviewItems, onSelect, domain, mode = 'lanes', fullBounds, onPanTo, onResetView }) {
  const scrollRef = useRef(null);
  const containerRef = useRef(null);
  const [viewportWidth, setViewportWidth] = useState(960);
  const [zoomExp, setZoomExp] = useState(0);
  const [tooltip, setTooltip] = useState(null);
  const { start, end } = domain;

  // 监听容器宽度（窗口变化 / 侧边栏收折）
  useEffect(() => {
    const node = containerRef.current;
    if (!node || typeof ResizeObserver === 'undefined') return;
    const observer = new ResizeObserver(entries => {
      const width = entries[0]?.contentRect?.width;
      if (width) setViewportWidth(Math.max(320, width));
    });
    observer.observe(node);
    return () => observer.disconnect();
  }, []);

  const domainMs = Math.max(end - start, 1);
  // 单轴模式只需一个窄标签列，给时间轴留出更多空间
  const labelWidth = mode === 'single' ? 88 : 104;
  const trackWidth = Math.max(viewportWidth - labelWidth, 240);
  const fitPxPerMs = trackWidth / domainMs;
  const pxPerMs = fitPxPerMs * (2 ** zoomExp);
  const innerWidth = domainMs * pxPerMs;
  const scrollable = innerWidth > trackWidth + 1;
  // 坐标轴两端留白：避免最左/最右的事件点被分类标签或边框遮挡
  const edgePad = 18;
  const plotWidth = Math.max(innerWidth - edgePad * 2, 1);
  const plotPxPerMs = plotWidth / domainMs;

  const lanes = useMemo(() => {
    if (mode === 'single') {
      const lane = buildSingleLane(events, { start, pxPerMs: plotPxPerMs, insetX: edgePad });
      return [{ category: 'all', rows: lane.rows, items: lane.items }];
    }
    return buildLanes(events, { start, pxPerMs: plotPxPerMs, insetX: edgePad, order: TIMELINE_CATEGORY_ORDER });
  }, [events, plotPxPerMs, edgePad, start, mode]);

  const ticks = useMemo(() => buildTicks(start, end, plotPxPerMs), [plotPxPerMs, start, end]);

  // 缩放/切换范围后把视口停在最新端
  useEffect(() => {
    const node = scrollRef.current;
    if (!node) return;
    node.scrollLeft = node.scrollWidth;
  }, [innerWidth, zoomExp, start, end]);

  // 单轴模式下抬高画布，避免整条轴过窄不便点击
  const laneHeight = lane => (mode === 'single'
    ? Math.max(lane.rows * AXIS_LANE_ROW_HEIGHT + 28, 132)
    : lane.rows * AXIS_LANE_ROW_HEIGHT + 18);
  const laneTop = lane => (mode === 'single'
    ? Math.round((laneHeight(lane) - lane.rows * AXIS_LANE_ROW_HEIGHT) / 2)
    : 9);
  const axisHeight = lanes.reduce((sum, lane) => sum + laneHeight(lane), 0);

  function showTooltip(event, nativeEvent) {
    // 键盘聚焦时 FocusEvent 没有坐标，退化为元素位置
    if (nativeEvent.clientX || nativeEvent.clientY) {
      setTooltip({ event, x: nativeEvent.clientX, y: nativeEvent.clientY });
      return;
    }
    const rect = nativeEvent.currentTarget?.getBoundingClientRect?.();
    setTooltip({
      event,
      x: rect ? rect.right : 0,
      y: rect ? rect.top : 0,
    });
  }

  function panBy(factor) {
    const span = end - start;
    onPanTo({ start: start + span * factor, end: end + span * factor });
  }

  return <div className={`pd-axis-wrap ${mode === 'single' ? 'pd-axis-single' : ''}`} ref={containerRef}>
    <TimelineOverview items={overviewItems ?? events} fullBounds={fullBounds} domain={domain} onPanTo={onPanTo} />

    <div className="pd-axis-controls">
      <div className="pd-axis-zoom">
        <button type="button" className="btn btn-outline btn-sm" disabled={zoomExp === 0} onClick={() => setZoomExp(value => Math.max(0, value - 1))} title="缩小">－</button>
        <span className="pd-axis-zoom-label">{zoomExp === 0 ? '自适应' : `${2 ** zoomExp}×`}</span>
        <button type="button" className="btn btn-outline btn-sm" disabled={zoomExp >= 7} onClick={() => setZoomExp(value => Math.min(7, value + 1))} title="放大">＋</button>
      </div>
      <div className="pd-axis-pan">
        <button type="button" className="btn btn-outline btn-sm" onClick={() => panBy(-0.5)} title="向更早时间平移">← 更早</button>
        <button type="button" className="btn btn-outline btn-sm" onClick={() => panBy(0.5)} title="向更晚时间平移">更晚 →</button>
      </div>
      <button type="button" className="btn btn-outline btn-sm" onClick={onResetView}>重置视野</button>
      <span className="pd-axis-hint">{scrollable ? '点上方总览条或拖动窗口快速定位；放大后可左右滚动' : '点上方总览条快速定位，放大后查看细节'}</span>
    </div>

    {events.length === 0 && <div className="pd-axis-empty">当前视野内没有事件，可拖动上方总览条、在空白处框选，或点击「重置视野」。</div>}

    <div className="pd-axis-frame">
      {/* 左侧固定标签列：独立于横向滚动区域，滚动时不会遮挡事件点 */}
      <div className="pd-axis-labels" style={{ width: labelWidth }}>
        <div className="pd-axis-corner" style={{ height: 36 }}>分类 / 时间</div>
        {lanes.map(lane => <div className="pd-axis-lane-label" key={lane.category} style={{ height: laneHeight(lane) }}>
          {lane.category === 'all' ? <>
            <i className="tone-accent">全</i>
            <span>全部事件</span>
          </> : <>
            <i className={`tone-${categoryTone(lane.category)}`}>{categoryIconLabel(lane.category)}</i>
            <span>{categoryLabel(lane.category)}</span>
          </>}
          <em>{lane.items.length}</em>
        </div>)}
      </div>

      <div className="pd-axis-scroll" ref={scrollRef}>
        <div className="pd-axis-track" style={{ width: innerWidth }}>
          {/* 标尺 */}
          <div className="pd-axis-ruler" style={{ width: innerWidth }}>
            <div className="pd-axis-ruler-track" style={{ width: innerWidth }}>
              {ticks.items.map(value => {
                const left = edgePad + (value - start) * plotPxPerMs;
                return <span
                  className={`pd-axis-tick${left < 52 ? ' align-left' : left > innerWidth - 52 ? ' align-right' : ''}`}
                  key={value}
                  style={{ left }}
                >
                  <i />
                  <b>{formatTick(value, ticks.step)}</b>
                </span>;
              })}
            </div>
          </div>

          {/* 事件点 */}
          <div className="pd-axis-body" style={{ height: axisHeight }}>
            <div className="pd-axis-gridtrack" style={{ width: innerWidth }}>
              {ticks.items.map(value => <span className="pd-axis-gridline" key={value} style={{ left: edgePad + (value - start) * plotPxPerMs }} />)}
            </div>
            {lanes.map(lane => <div className="pd-axis-lane" key={lane.category} style={{ height: laneHeight(lane) }}>
              <div className="pd-axis-lane-track" style={{ width: innerWidth }}>
                {lane.items.map(({ event, x, row }, index) => <button
                  type="button"
                  key={`${event.event_type}-${event.related_id || 'x'}-${event.occurred_at}-${index}`}
                  className={`pd-axis-dot tone-${categoryTone(event.category)} ${event.status === 'failed' || event.status === 'rejected' ? 'is-alert' : ''}`}
                  style={{ left: x, top: laneTop(lane) + row * AXIS_LANE_ROW_HEIGHT + (AXIS_LANE_ROW_HEIGHT - 16) / 2 }}
                  onMouseEnter={nativeEvent => showTooltip(event, nativeEvent)}
                  onMouseMove={nativeEvent => showTooltip(event, nativeEvent)}
                  onMouseLeave={() => setTooltip(null)}
                  onFocus={nativeEvent => showTooltip(event, nativeEvent)}
                  onBlur={() => setTooltip(null)}
                  onClick={() => onSelect(event)}
                  aria-label={`${formatChinaDateTime(event.occurred_at)} ${event.title}`}
                />)}
              </div>
            </div>)}
          </div>
        </div>
      </div>
    </div>

    {tooltip && <div className="pd-axis-tooltip" style={{ left: Math.min(tooltip.x + 14, window.innerWidth - 280), top: Math.min(tooltip.y + 14, window.innerHeight - 120) }}>
      <div className="pd-axis-tooltip-time">{formatChinaDateTime(tooltip.event.occurred_at)}</div>
      <div className="pd-axis-tooltip-title">{tooltip.event.title}</div>
      {tooltip.event.description && <div className="pd-axis-tooltip-desc">{tooltip.event.description}</div>}
      <div className="pd-axis-tooltip-foot">{categoryLabel(tooltip.event.category)}{tooltip.event.server_name ? ` · ${tooltip.event.server_name}` : ''} · 点击查看详情</div>
    </div>}
  </div>;
}

/** 列表时间线：按天分组的完整事件卡片。 */
function TimelineList({ events }) {
  const [order, setOrder] = useState('desc');
  const [limit, setLimit] = useState(80);
  const ordered = useMemo(() => order === 'asc' ? [...events].reverse() : events, [events, order]);
  const groups = useMemo(() => {
    const result = [];
    for (const event of ordered.slice(0, limit)) {
      const key = formatChinaDate(event.occurred_at);
      const last = result[result.length - 1];
      if (last && last.key === key) last.items.push(event);
      else result.push({ key, weekday: chinaWeekday(event.occurred_at), items: [event] });
    }
    return result;
  }, [ordered, limit]);

  return <>
    <div className="pd-timeline-filters">
      <button type="button" className="btn btn-outline btn-sm" onClick={() => setOrder(order === 'desc' ? 'asc' : 'desc')}>
        {order === 'desc' ? '最新在前' : '最早在前'}
      </button>
    </div>
    <div className="pd-timeline">
      {groups.map(group => <section className="pd-day" key={group.key}>
        <div className="pd-day-head">
          <span className="pd-day-date">{group.key}</span>
          <span className="pd-day-weekday">{group.weekday}</span>
          <span className="pd-day-count">{group.items.length} 条</span>
          <span className="pd-day-line" />
        </div>
        <div className="pd-day-events">
          {group.items.map((event, index) => <TimelineEventCard
            event={event}
            key={`${event.event_type}-${event.related_id || 'x'}-${event.occurred_at}-${index}`}
          />)}
        </div>
      </section>)}
      {ordered.length > limit && <div className="pd-timeline-more">
        <button type="button" className="btn btn-outline" onClick={() => setLimit(limit + 80)}>
          加载更多（剩余 {ordered.length - limit} 条）
        </button>
      </div>}
    </div>
  </>;
}

/** 事件卡片（列表视图）。 */
function TimelineEventCard({ event }) {
  const tone = categoryTone(event.category);
  return <div className={`pd-event tone-${tone}`}>
    <div className="pd-event-time" title={formatChinaDateTime(event.occurred_at)}>
      {TIME_FORMATTER.format(new Date(event.occurred_at))}
    </div>
    <div className="pd-event-rail">
      <span className="pd-event-dot" aria-hidden="true">{categoryIconLabel(event.category)}</span>
    </div>
    <div className="pd-event-card">
      <div className="pd-event-head">
        <div className="pd-event-title">{event.title}</div>
        <div className="pd-event-pills">
          <StatusPill kind={categoryKind(event.category, event.status)}>{categoryLabel(event.category)}</StatusPill>
          {event.status && <StatusPill kind={stKind(event.status)}>{stLabel(event.status)}</StatusPill>}
        </div>
      </div>
      {event.description && <div className="pd-event-desc">{event.description}</div>}
      {(event.meta?.length > 0 || event.server_name) && <div className="pd-event-meta">
        {event.server_name && <span className="pd-event-meta-item pd-event-server">
          <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><rect x="2" y="2" width="20" height="8" rx="2" /><rect x="2" y="14" width="20" height="8" rx="2" /></svg>
          {event.server_name}
        </span>}
        {(event.meta || []).map((meta, index) => <span className="pd-event-meta-item" key={`${meta.label}-${index}`}>
          <b>{meta.label}</b>{meta.value}
        </span>)}
      </div>}
      <div className="pd-event-footer">
        <span>{actorFieldLabel(event.category)}：{event.actor || '系统 / 玩家本人'}</span>
        <span>来源：{eventSourceLabel(event.source)}</span>
        {event.related_id && <span className="mono">ID：{event.related_id}</span>}
      </div>
    </div>
  </div>;
}

function TimelineTab({ detail }) {
  const events = useMemo(() => detail.timeline || [], [detail.timeline]);
  const [selectedCategories, setSelectedCategories] = useState([]);
  const [keyword, setKeyword] = useState('');
  const [rangeKey, setRangeKey] = useState('all');
  const [customDomain, setCustomDomain] = useState(null);
  const [view, setView] = useState('single');
  const [selectedEvent, setSelectedEvent] = useState(null);

  const fullBounds = useMemo(() => buildBounds(events, AXIS_FALLBACK_NOW), [events]);
  const domain = useMemo(
    () => customDomain ?? buildDomain(events, rangeKey, AXIS_FALLBACK_NOW),
    [events, rangeKey, customDomain],
  );

  // 总览条/平移按钮调整视野时，钳制在全量时间范围内
  const panTo = useCallback(({ start, end }) => {
    const maxSpan = Math.max(fullBounds.max - fullBounds.min, 1000);
    const span = Math.min(Math.max(end - start, 1000), maxSpan);
    let nextStart = start;
    let nextEnd = end;
    if (nextStart < fullBounds.min) { nextStart = fullBounds.min; nextEnd = nextStart + span; }
    if (nextEnd > fullBounds.max) { nextEnd = fullBounds.max; nextStart = nextEnd - span; }
    setCustomDomain({ start: nextStart, end: nextEnd });
  }, [fullBounds.min, fullBounds.max]);

  function resetView() {
    setCustomDomain(null);
    setRangeKey('all');
  }

  const counts = useMemo(() => {
    const map = new Map();
    for (const event of events) map.set(event.category, (map.get(event.category) || 0) + 1);
    return map;
  }, [events]);

  const categories = useMemo(() => {
    const known = TIMELINE_CATEGORY_ORDER.filter(category => counts.has(category));
    const extra = [...counts.keys()].filter(category => !TIMELINE_CATEGORY_ORDER.includes(category));
    return [...known, ...extra];
  }, [counts]);

  // 分类 / 关键字筛选（不含时间范围），用于总览条与列表
  const matched = useMemo(() => {
    const needle = keyword.trim().toLowerCase();
    return events.filter(event => {
      if (selectedCategories.length > 0 && !selectedCategories.includes(event.category)) return false;
      if (needle) {
        const haystack = [
          event.title,
          event.description,
          event.actor,
          event.server_name,
          event.source,
          ...(event.meta || []).flatMap(meta => [meta.label, meta.value]),
        ].filter(Boolean).join(' ').toLowerCase();
        if (!haystack.includes(needle)) return false;
      }
      return true;
    });
  }, [events, selectedCategories, keyword]);

  // 当前坐标轴视野内的事件
  const visible = useMemo(() => matched.filter(event => {
    const time = new Date(event.occurred_at).getTime();
    return Number.isFinite(time) && time >= domain.start && time <= domain.end;
  }), [matched, domain.start, domain.end]);

  const dayCount = useMemo(() => new Set(matched.map(event => formatChinaDate(event.occurred_at))).size, [matched]);

  function toggleCategory(category) {
    setSelectedCategories(prev => prev.includes(category)
      ? prev.filter(item => item !== category)
      : [...prev, category]);
  }

  function resetFilters() {
    setSelectedCategories([]);
    setKeyword('');
    setCustomDomain(null);
    setRangeKey('all');
  }

  const hasFilters = selectedCategories.length > 0 || keyword.trim() !== '' || rangeKey !== 'all' || customDomain != null;

  return <div className="card pd-timeline-card">
    <div className="card-header">
      <div>
        <div className="card-title">玩家事件时间线</div>
        <div className="card-sub">
          横轴为时间，所有事件在发生时间处打点：服务器进出、进服判定、白名单、封禁、全球封禁、异常成绩、QQ、内部备注及管理员操作。悬停看摘要，点击看详情。
        </div>
      </div>
      <div className="pd-timeline-summary">
        <strong>{events.length}</strong>
        <span>条事件</span>
        {detail.timeline_truncated && detail.timeline_total > events.length && (
          <span className="pd-timeline-warn" title="事件过多，仅展示最近 500 条，请用筛选缩小范围">
            已截断（共 {detail.timeline_total} 条）
          </span>
        )}
      </div>
    </div>

    <div className="card-body pd-timeline-body">
      <div className="pd-timeline-toolbar">
        <div className="pd-timeline-chips" role="group" aria-label="事件分类筛选">
          <button type="button" className={`pd-chip ${selectedCategories.length === 0 ? 'active' : ''}`} onClick={() => setSelectedCategories([])}>
            全部<span className="pd-chip-count">{events.length}</span>
          </button>
          {categories.map(category => (
            <button
              type="button"
              key={category}
              className={`pd-chip tone-${categoryTone(category)} ${selectedCategories.includes(category) ? 'active' : ''}`}
              onClick={() => toggleCategory(category)}
              aria-pressed={selectedCategories.includes(category)}
            >
              <i className="pd-chip-dot" aria-hidden="true" />
              {categoryLabel(category)}<span className="pd-chip-count">{counts.get(category) || 0}</span>
            </button>
          ))}
        </div>
        <div className="pd-timeline-filters">
          <div className="pd-timeline-search">
            <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><circle cx="11" cy="11" r="8" /><path d="M21 21l-4.35-4.35" /></svg>
            <input
              type="search"
              value={keyword}
              placeholder="搜索事件标题 / 描述 / 服务器 / 操作人"
              onChange={event => setKeyword(event.target.value)}
            />
          </div>
          <div className="pd-range-group" role="group" aria-label="时间范围">
            {AXIS_RANGES.map(range => <button
              type="button"
              key={range.key}
              className={`pd-range-btn ${rangeKey === range.key ? 'active' : ''}`}
              onClick={() => { setRangeKey(range.key); setCustomDomain(null); }}
            >{range.label}</button>)}
          </div>
          <div className="pd-view-switch" role="group" aria-label="视图切换">
            <button type="button" className={view === 'single' ? 'active' : ''} onClick={() => setView('single')} title="所有事件共用一条时间轴">单轴</button>
            <button type="button" className={view === 'lanes' ? 'active' : ''} onClick={() => setView('lanes')} title="按事件分类分泳道">分泳道</button>
            <button type="button" className={view === 'list' ? 'active' : ''} onClick={() => setView('list')}>列表</button>
          </div>
          {hasFilters && <button type="button" className="btn btn-outline btn-sm" onClick={resetFilters}>重置筛选</button>}
        </div>
        <div className="pd-timeline-meta">
          匹配 <strong>{matched.length}</strong> 条 · 当前视野 <strong>{visible.length}</strong> 条 · 跨 <strong>{dayCount}</strong> 天 · 范围 <strong>{formatChinaDate(domain.start)}</strong> ~ <strong>{formatChinaDate(domain.end)}</strong>
        </div>
      </div>

      {matched.length === 0
        ? <Empty>{events.length === 0 ? '暂无事件记录。' : '没有匹配的事件，请调整筛选条件。'}</Empty>
        : (view === 'list'
          ? <TimelineList events={matched} />
          : <TimelineAxis
            events={visible}
            overviewItems={matched}
            domain={domain}
            fullBounds={fullBounds}
            onPanTo={panTo}
            onResetView={resetView}
            onSelect={setSelectedEvent}
            mode={view}
          />)}
    </div>

    {selectedEvent && <TimelineEventModal event={selectedEvent} onClose={() => setSelectedEvent(null)} />}
  </div>;
}

// ── 进服记录 ──────────────────────────────────────────────────────────────

function SessionTable({ sessions = [], emptyText = '暂无服务器会话记录。' }) {
  return sessions.length === 0 ? <Empty>{emptyText}</Empty> : <div className="table-responsive"><table className="data-table player-record-table mobile-card-table"><thead><tr><th>服务器</th><th>进入时间</th><th>退出时间</th><th>退出原因</th><th>时长</th><th>IP</th><th>Ping</th><th>地图</th><th>玩家名</th></tr></thead><tbody>
    {sessions.map(item => <tr key={item.id}>
      <td className="fw-600 mobile-card-primary" data-label="服务器">{item.server_name}:{item.server_port}<br /><span style={{ color: 'var(--text3)', fontSize: '11.5px' }}>{item.community_name || '-'}</span></td>
      <td data-label="进入时间" style={{ fontFamily: 'var(--mono)', fontSize: '12px' }}>{formatChinaDateTime(item.first_seen_at, { seconds: false })}</td>
      <td data-label="退出状态"><StatusPill kind={item.left_at ? 'offline' : 'success'}>{item.left_at ? '已退出' : '在线'}</StatusPill><div className="player-table-sub">{sessionEndLabel(item)}</div></td>
      <td data-label="退出原因"><StatusPill kind={sessionReasonKind(item.end_reason)}>{item.left_at ? sessionReasonLabel(item.end_reason) : '仍在线'}</StatusPill>{item.end_detail ? <div className="player-table-sub text-break">{item.end_detail}</div> : null}</td>
      <td data-label="时长">{sessionDurationLabel(item.duration_seconds)}</td>
      <td data-label="IP" className="steam-id">{item.ip || '-'}</td>
      <td data-label="Ping">{item.last_ping ?? '-'}</td>
      <td data-label="地图">{item.last_map || '-'}</td>
      <td data-label="玩家名">{item.player_name || '-'}</td>
    </tr>)}
  </tbody></table></div>;
}

/** 进服行为统计条：时长聚合 + 活跃时段分布 + Top 服务器/地图。 */
function ActivityStatsBar({ stats }) {
  if (!stats) return null;
  const fmtMinutes = (m) => m >= 1440 ? `${Math.floor(m / 1440)} 天 ${(m % 1440 / 60).toFixed(0)} 小时` : m >= 60 ? `${(m / 60).toFixed(1)} 小时` : `${m} 分钟`;
  const nightBuckets = [16, 17, 18, 19, 20, 21, 22, 23]; // UTC 16-23 ≈ 本地 0-7 点
  const nightCount = nightBuckets.reduce((sum, h) => sum + (stats.hourly_distribution?.[h] || 0), 0);
  const totalCount = (stats.hourly_distribution || []).reduce((a, b) => a + b, 0);
  const nightRatio = totalCount > 0 ? nightCount / totalCount : 0;
  const maxHour = Math.max(...(stats.hourly_distribution || [0]));
  const cards = [
    { label: '累计时长', value: fmtMinutes(stats.total_play_minutes || 0) },
    { label: '平均单次', value: fmtMinutes(stats.avg_play_minutes || 0) },
    { label: '最长单次', value: fmtMinutes(stats.max_play_minutes || 0) },
    { label: '深夜活跃占比', value: `${(nightRatio * 100).toFixed(0)}%`, warn: nightRatio > 0.4 },
  ];
  return <div className="card"><div className="card-header"><div><div className="card-title">行为统计</div><div className="card-sub">基于会话记录聚合。深夜时段（本地 0-7 点）高占比是代练/工作室的常见特征。</div></div></div><div className="card-body">
    <div className="player-activity-cards">
      {cards.map(c => <div className={`player-activity-card ${c.warn ? 'warn' : ''}`} key={c.label}>
        <div className="player-activity-value">{c.value}</div>
        <div className="player-activity-label">{c.label}</div>
      </div>)}
    </div>
    <div className="player-activity-section">
      <div className="player-activity-title">活跃时段（本地时间，共 {totalCount} 次会话）</div>
      <div className="player-activity-hours">
        {Array.from({ length: 24 }, (_, h) => {
          const localH = (h + 8) % 24;
          const v = stats.hourly_distribution?.[h] || 0;
          const pct = maxHour > 0 ? (v / maxHour) * 100 : 0;
          return <div className="player-activity-hour" key={h} title={`${localH}:00-${localH + 1}:00 · ${v} 次`}>
            <div className="player-activity-bar" style={{ height: `${Math.max(pct, v > 0 ? 8 : 2)}%` }} />
            <span>{localH}</span>
          </div>;
        })}
      </div>
    </div>
    {(stats.top_servers?.length > 0 || stats.top_maps?.length > 0) && <div className="player-activity-topgrid">
      {stats.top_servers?.length > 0 && <div>
        <div className="player-activity-title">最常出没服务器</div>
        {stats.top_servers.map(([name, n]) => <div className="player-activity-row" key={name}><span className="text-ellipsis" style={{ maxWidth: 180 }}>{name}</span><strong>{n} 次</strong></div>)}
      </div>}
      {stats.top_maps?.length > 0 && <div>
        <div className="player-activity-title">最常玩地图</div>
        {stats.top_maps.map(([name, n]) => <div className="player-activity-row" key={name}><span className="mono" style={{ maxWidth: 180, overflow: 'hidden', textOverflow: 'ellipsis' }}>{name}</span><strong>{n} 次</strong></div>)}
      </div>}
    </div>}
  </div></div>;
}

function AccessTab({ detail }) {
  const accessLogs = detail.access_logs || [];
  const currentOnline = detail.online_records || [];
  const sessions = detail.player_sessions || [];
  return <>
    <ActivityStatsBar stats={detail.activity_stats} />
    <div className="card"><div className="card-header"><div><div className="card-title">当前在线服务器</div><div className="card-sub">来自服务器实时上报快照。</div></div></div><div className="card-body p-0">
      {currentOnline.length === 0 ? <Empty>当前没有在已上报服务器中。</Empty> : <div className="table-responsive"><table className="data-table player-record-table mobile-card-table"><thead><tr><th>服务器</th><th>社区</th><th>上报时间</th><th>IP</th><th>Ping</th><th>地图</th></tr></thead><tbody>
        {currentOnline.map((item, i) => <tr key={`${item.server_id}-${item.reported_at}-${i}`}>
          <td data-label="服务器" className="fw-600 mobile-card-primary">{item.server_name}:{item.server_port}</td>
          <td data-label="社区">{item.community_name || '-'}</td>
          <td data-label="上报时间" style={{ fontFamily: 'var(--mono)', fontSize: '12px' }}>{formatChinaDateTime(item.reported_at, { seconds: false })}</td>
          <td data-label="IP" className="steam-id">{item.ip}</td>
          <td data-label="Ping">{item.ping}</td>
          <td data-label="地图">{item.current_map || '-'}</td>
        </tr>)}
      </tbody></table></div>}
    </div></div>

    <div className="card"><div className="card-header"><div><div className="card-title">服务器会话历史</div><div className="card-sub">由在线快照生成，包含进入服务器和退出服务器时间。</div></div></div><div className="card-body p-0">
      <SessionTable sessions={sessions} />
    </div></div>

    <div className="card"><div className="card-header"><div><div className="card-title">进服成功 / 失败明细</div><div className="card-sub">展示进入服务器的判定方式、失败原因、IP、Rating 和 Steam 等级。</div></div></div><div className="card-body p-0">
      {accessLogs.length === 0 ? <Empty>暂无进服尝试日志。</Empty> : <div className="table-responsive"><table className="data-table player-record-table mobile-card-table"><thead><tr><th>时间</th><th>结果</th><th>服务器</th><th>IP</th><th>方式 / 原因</th><th>Rating</th><th>Steam等级</th><th>玩家名</th></tr></thead><tbody>
        {accessLogs.map(item => <tr key={item.id} className={!item.allowed ? 'row-access-denied' : undefined}>
          <td data-label="时间" style={{ fontFamily: 'var(--mono)', fontSize: '12px' }}>{formatChinaDateTime(item.created_at, { seconds: false })}</td>
          <td data-label="结果"><StatusPill kind={item.allowed ? 'success' : 'danger'}>{item.allowed ? '成功' : '失败'}</StatusPill></td>
          <td data-label="服务器" className="fw-600 mobile-card-primary">{item.server_name}:{item.server_port}<br /><span style={{ color: 'var(--text3)', fontSize: '11.5px' }}>{item.community_name || '-'}</span></td>
          <td data-label="IP" className="steam-id">{item.ip_address || '-'}</td>
          <td data-label="方式 / 原因"><StatusPill kind={item.allowed ? 'success' : 'danger'}>{methodLabel(item.access_method)}</StatusPill><div className="player-table-sub">{item.allowed ? '允许原因' : failureLabel(item)}</div></td>
          <td data-label="Rating">{item.rating ?? '-'}</td>
          <td data-label="Steam等级">{item.steam_level ?? '-'}</td>
          <td data-label="玩家名">{item.player_name || '-'}</td>
        </tr>)}
      </tbody></table></div>}
    </div></div>
  </>;
}

// ── 封禁 / 白名单 ─────────────────────────────────────────────────────────

function StatusTab({ detail, token, onRefresh }) {
  const [popup, setPopup] = useState(null);
  return <>
    <div className="card"><div className="card-header"><div><div className="card-title">历史封禁履历表</div><div className="card-sub">点击行查看详情并操作。</div></div></div><div className="card-body p-0">
      {detail.bans.length === 0 ? <Empty>暂无封禁记录。</Empty> : <div className="table-responsive"><table className="data-table player-record-table mobile-card-table"><thead><tr><th>执行时间</th><th>封禁维度</th><th>时长</th><th>封禁理由</th><th>操作人</th><th>状态</th></tr></thead><tbody>
        {detail.bans.map(item => <tr key={item.id} style={{ cursor: 'pointer' }} onClick={() => setPopup({ type: 'ban', item })}>
          <td data-label="执行时间" style={{ fontFamily: 'var(--mono)', fontSize: '12px' }}>{formatChinaDateTime(item.created_at, { seconds: false })}</td>
          <td data-label="封禁维度">{item.ban_type === 'ip' ? '账号+IP' : '账号'}</td>
          <td data-label="时长" style={{ color: item.status === 'active' ? 'var(--danger-text)' : 'var(--text2)' }}>{durLabel(item.duration_minutes)}</td>
          <td data-label="封禁理由" className="mobile-card-full" title={item.reason}>{item.reason}</td>
          <td data-label="操作人">{item.operator_name || '-'}</td>
          <td data-label="状态"><StatusPill kind={stKind(item.status, 'ban')}>{stLabel(item.status)}</StatusPill></td>
        </tr>)}
      </tbody></table></div>}
    </div></div>
    <div className="card"><div className="card-header"><div><div className="card-title">社区白名单记录</div><div className="card-sub">点击行查看详情并审核。</div></div></div><div className="card-body p-0">
      {detail.whitelist.length === 0 ? <Empty>暂无白名单记录。</Empty> : <div className="table-responsive"><table className="data-table player-record-table mobile-card-table"><thead><tr><th>提交时间</th><th>昵称</th><th>联系方式</th><th>审核人</th><th>审核意见</th><th>状态</th></tr></thead><tbody>
        {detail.whitelist.map(item => <tr key={item.id} style={{ cursor: 'pointer' }} onClick={() => setPopup({ type: 'whitelist', item })}>
          <td data-label="提交时间" style={{ fontFamily: 'var(--mono)', fontSize: '12px' }}>{formatChinaDateTime(item.applied_at, { seconds: false })}</td>
          <td data-label="昵称" className="fw-600 mobile-card-primary">{item.nickname}</td>
          <td data-label="联系方式" className="steam-id">{item.contact || '-'}</td>
          <td data-label="审核人">{item.approved_by || item.rejected_by || item.revoked_by || '-'}</td>
          <td data-label="审核意见" className="mobile-card-full">{item.approval_reason || item.rejection_reason || '-'}</td>
          <td data-label="状态"><StatusPill kind={stKind(item.status, 'whitelist')}>{stLabel(item.status)}</StatusPill></td>
        </tr>)}
      </tbody></table></div>}
    </div></div>
    {popup?.type === 'ban' && <BanDetailPopup item={popup.item} onClose={() => setPopup(null)} onAction={onRefresh} token={token} />}
    {popup?.type === 'whitelist' && <WhitelistDetailPopup item={popup.item} onClose={() => setPopup(null)} onAction={onRefresh} token={token} />}
  </>;
}

// ── IP / QQ 关联 ──────────────────────────────────────────────────────────

function NetworkTab({ detail, token }) {
  const ipHistory = detail.ip_history || [];
  const sessions = detail.player_sessions || [];
  const { toast } = useToast();
  const [selected, setSelected] = useState([]);
  const [tag, setTag] = useState('');
  const [acting, setActing] = useState(false);
  const toggle = (id) => setSelected(prev => prev.includes(id) ? prev.filter(item => item !== id) : [...prev, id]);
  async function batch(action) {
    if (!tag.trim() || selected.length === 0 || acting) return;
    setActing(true);
    try {
      await api.linkedAccountsBatch(token, detail.profile.steamid64, { steamids64: selected, action, tag: tag.trim() });
      toast({ title: action === 'add_tag' ? '关联账号已批量标记' : '关联账号标签已移除' });
      setSelected([]);
    } catch (error) { toast({ title: '批量操作失败', message: error.message, tone: 'danger' }); }
    finally { setActing(false); }
  }
  return <>
    <div className="card"><div className="card-header"><div><div className="card-title">QQ 绑定与私聊</div><div className="card-sub">展示绑定的 QQ 与同 QQ 账号风险，管理员可直接私聊玩家或解绑。</div></div></div><div className="card-body">
      <QqBindingPanel steamid64={detail.profile.steamid64} />
    </div></div>
    <div className="card"><div className="card-header"><div><div className="card-title">同 QQ 关联账号</div><div className="card-sub">同一 QQ openid 下绑定的其他 Steam 账号。任一账号存在封禁/白名单拒绝记录时，该 QQ 即为高风险 QQ。</div></div></div><div className="card-body">
      {(() => {
        const riskProfile = detail.risk_profile;
        const qqAccounts = riskProfile?.qq_linked_accounts || [];
        if (!riskProfile?.qq_openid) return <Empty>该账号尚未绑定 QQ，暂无同 QQ 关联。</Empty>;
        if (qqAccounts.length === 0) return <Empty>已绑定 QQ，同 QQ 下暂无其他 Steam 账号。</Empty>;
        const qqLb = qqAccounts.filter((a) => a.has_active_local_ban).length;
        const qqGb = qqAccounts.filter((a) => a.has_active_global_ban).length;
        const qqRejected = qqAccounts.reduce((sum, a) => sum + (a.rejected_whitelist_count || 0), 0);
        return <>
          <div className="player-table-sub" style={{ marginBottom: 8 }}>
            已绑定 QQ · 同 QQ 账号 {qqAccounts.length} 个
            {(qqLb + qqGb + qqRejected) > 0 ? <span style={{ color: 'var(--danger-text)', fontWeight: 600 }}> · ⚠ 高风险 QQ（本地封禁 {qqLb} · 全球封禁 {qqGb} · 白名单被拒 {qqRejected} 次）</span> : <span> · 暂无封禁/拒绝记录</span>}
          </div>
          <div className="table-responsive"><table className="data-table player-record-table mobile-card-table"><thead><tr><th>账号</th><th>关联方式</th><th>风险标记</th><th>最近出现</th></tr></thead><tbody>
            {qqAccounts.map((account) => (
              <tr key={account.steamid64} className={account.has_active_local_ban || account.has_active_global_ban ? 'row-access-denied' : undefined}>
                <td data-label="账号" className="mobile-card-primary"><span style={{ fontWeight: 600 }}>{account.player_name || '(未知玩家)'}</span> <code className="steam-id" style={{ fontSize: '11px' }}>{account.steamid64}</code></td>
                <td data-label="关联方式"><StatusPill kind="default">同QQ绑定</StatusPill></td>
                <td data-label="风险标记"><div className="linked-account-pills">
                  {account.has_active_local_ban && <StatusPill kind="danger">本地封禁</StatusPill>}
                  {account.has_active_global_ban && <StatusPill kind="danger">全球封禁</StatusPill>}
                  {(account.rejected_whitelist_count || 0) > 0 && <StatusPill kind="warning">白名单被拒 {account.rejected_whitelist_count} 次</StatusPill>}
                  {!account.has_active_local_ban && !account.has_active_global_ban && !(account.rejected_whitelist_count > 0) && <StatusPill kind="default">无风险标记</StatusPill>}
                </div></td>
                <td data-label="最近出现" style={{ fontFamily: 'var(--mono)', fontSize: '12px' }}>{account.last_seen_at ? formatChinaDateTime(account.last_seen_at, { seconds: false }) : '-'}</td>
              </tr>
            ))}
          </tbody></table></div>
        </>;
      })()}
    </div></div>
    <div className="card"><div className="card-header"><div><div className="card-title">账号 / IP 关系图</div><div className="card-sub">节点表示账号，连线标签表示共享 IP。勾选关联账号后可批量标记。</div></div></div>
      <div className="player-relation-graph">
        <div className="player-relation-center">
          <div className="player-relation-center-avatar">{(detail.profile.display_name || detail.profile.steamid64 || '?').slice(0, 2).toUpperCase()}</div>
          <strong>{detail.profile.display_name || '当前玩家'}</strong>
          <code>{detail.profile.steamid64}</code>
          <div className="player-relation-center-tags">
            {riskLabel(detail?.risk_profile?.action) && <StatusPill kind={riskTone(detail?.risk_profile?.action)}>{riskLabel(detail?.risk_profile?.action)}</StatusPill>}
          </div>
          <div className="player-table-sub">中心账号 · {ipHistory.length} 个关联 IP</div>
        </div>
        <div className="player-relation-list">
          {ipHistory.length === 0 ? <Empty>暂无可绘制的 IP 关系。</Empty> : ipHistory.map(entry => {
            const lb = (entry.linked_accounts || []).filter(a => a.has_local_ban).length;
            const gb = (entry.linked_accounts || []).filter(a => a.has_global_ban).length;
            const banned = lb + gb;
            return <div className="player-relation-ip" key={entry.ip}>
              <div className="player-relation-ip-head">
                <span className="player-relation-ip-badge"><svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><rect x="2" y="9" width="20" height="6" rx="2" /><path d="M6 9V6a3 3 0 0 1 3-3h6a3 3 0 0 1 3 3v3" /></svg><code>{entry.ip}</code></span>
                <span className="player-table-sub">{entry.servers?.map(s => `${s.server_name}${s.server_port ? `:${s.server_port}` : ''}`).slice(0, 2).join(' · ') || '服务器未知'}</span>
                {(entry.linked_accounts?.length > 0) && <span className={`player-relation-ip-count ${banned > 0 ? 'danger' : ''}`}>{entry.linked_accounts.length} 个关联{banned > 0 ? ` · ${lb} 本地 / ${gb} 全球` : ''}</span>}
              </div>
              <div className="player-relation-accounts">
                {(entry.linked_accounts || []).map(account => {
                  const isSelected = selected.includes(account.steam_id64);
                  return <label className={`player-relation-account ${isSelected ? 'selected' : ''} ${account.has_local_ban || account.has_global_ban ? 'flagged' : ''}`} key={entry.ip + '-' + account.steam_id64}>
                    <span className="player-relation-account-check"><input type="checkbox" checked={isSelected} onChange={() => toggle(account.steam_id64)} /></span>
                    <span className="player-relation-account-avatar">{(account.player_name || '?').slice(0, 1).toUpperCase()}</span>
                    <span className="player-relation-account-body">
                      <span className="player-relation-account-name">{account.player_name || '未知玩家'}</span>
                      <code className="player-relation-account-id">{account.steam_id64}</code>
                      <span className="player-relation-account-meta">
                        <span>访问 {account.access_count || 0} 次</span>
                        {(account.has_local_ban || account.has_global_ban) && <span className="player-relation-account-ban">⚑ 封禁</span>}
                      </span>
                    </span>
                  </label>;
                })}
                {(!entry.linked_accounts || entry.linked_accounts.length === 0) && <div className="player-relation-empty">该 IP 暂无其他关联账号</div>}
              </div>
            </div>;
          })}
        </div>
      </div>
      {selected.length > 0 && <div className="card-body player-relation-actions"><strong>已选 {selected.length} 个账号</strong><input className="form-control" style={{ maxWidth: 220, flex: 1, minWidth: 140 }} value={tag} onChange={e => setTag(e.target.value)} placeholder="标签名称" /><button className="btn btn-primary btn-sm" disabled={acting || !tag.trim()} onClick={() => batch('add_tag')}>批量添加标签</button><button className="btn btn-outline btn-sm" disabled={acting || !tag.trim()} onClick={() => batch('remove_tag')}>批量移除标签</button></div>}
    </div>
    <div className="card"><div className="card-header"><div><div className="card-title">深度 IP 交叉与设备追踪表</div><div className="card-sub"><strong style={{ color: 'var(--accent)' }}>逆向检索同 IP 的关联 Steam 账号</strong>，包含关联账号白名单和封禁状态。</div></div></div><div className="card-body p-0">
      {ipHistory.length === 0 ? <Empty>暂无 IP 登录记录。</Empty> : <div className="table-responsive"><table className="data-table tree-table mobile-card-table"><thead><tr><th>IP</th><th>首次/最后活跃</th><th>服务器</th><th>关联账号 / 白名单</th></tr></thead><tbody>
        {ipHistory.map(entry => {
          const lb = entry.linked_accounts?.filter(a => a.has_local_ban).length || 0;
          const gb = entry.linked_accounts?.filter(a => a.has_global_ban).length || 0;
          const banned = lb + gb;
          return <React.Fragment key={entry.ip}>
            <tr><td data-label="IP"><code className="steam-id" style={{ fontWeight: 700, fontSize: '13px' }}>{entry.ip}</code></td>
              <td data-label="首次/最后活跃" className="mobile-card-full" style={{ fontFamily: 'var(--mono)', color: 'var(--text2)', fontSize: '12px' }}>首:{entry.first_seen ? formatChinaDateTime(entry.first_seen, { seconds: false }) : '-'}<br />末:{entry.last_seen ? formatChinaDateTime(entry.last_seen, { seconds: false }) : '-'}</td>
              <td data-label="服务器" style={{ color: 'var(--text2)' }}>{entry.servers?.map(s => `${s.server_name}${s.server_port ? `:${s.server_port}` : ''}`).join(', ') || '-'}</td>
              <td data-label="关联账号" className="mobile-card-primary">{entry.linked_accounts?.length > 0 ? <span style={{ color: banned > 0 ? 'var(--danger-text)' : 'var(--text2)', fontWeight: 600 }}>⚠ {entry.linked_accounts.length}个关联{banned > 0 ? `（本地 ${lb} · 全球 ${gb}）` : ''}</span> : <span style={{ color: 'var(--text3)' }}>无</span>}</td></tr>
            {entry.linked_accounts?.map(acc => <tr key={acc.steam_id64}><td data-label="关联账号" className="nested mobile-card-primary">└─关联</td><td data-label="访问次数" style={{ fontSize: '11px', color: 'var(--text3)' }}>访问 {acc.access_count || 0}次</td><td data-label="服务器">{acc.servers?.join(', ') || '-'}</td>
              <td data-label="状态" className="mobile-card-full">
                <div className="linked-account-cell">
                  <span style={{ fontWeight: 600 }}>{acc.player_name || '(未知)'}</span> <code className="steam-id" style={{ fontSize: '11px' }}>{acc.steam_id64}</code>
                  <div className="linked-account-pills">
                    {acc.has_local_ban && <StatusPill kind="danger">本地封禁</StatusPill>}
                    {acc.has_global_ban && <StatusPill kind="danger">全球封禁</StatusPill>}
                    {acc.whitelist_count > 0 ? <StatusPill kind={stKind(acc.whitelist_status, 'whitelist')}>白名单 {stLabel(acc.whitelist_status)} · {acc.whitelist_count} 次</StatusPill> : <StatusPill kind="default">无白名单</StatusPill>}
                  </div>
                  {acc.whitelist_applied_at && <div className="player-table-sub">最近申请: {formatChinaDateTime(acc.whitelist_applied_at, { seconds: false })}{acc.whitelist_reviewer ? ` · 审核人: ${acc.whitelist_reviewer}` : ''}</div>}
                </div>
              </td></tr>)}
          </React.Fragment>;
        })}
      </tbody></table></div>}
    </div></div>
    <div className="card"><div className="card-header"><div><div className="card-title">服务器会话与在线记录</div></div></div><div className="card-body p-0">
      <SessionTable sessions={sessions} emptyText="暂无服务器会话记录。" />
    </div></div>
  </>;
}

// ── 证据 ──────────────────────────────────────────────────────────────────

function EvidenceTab({ detail, token }) {
  const { toast } = useToast();
  return <div className="card"><div className="card-header"><div><div className="card-title">物理证据文件与媒体库</div></div></div><div className="card-body">
    {detail.evidence_files.length === 0 ? <Empty>暂无附件证据。</Empty> : <div className="table-responsive"><table className="data-table mobile-card-table"><thead><tr><th>文件名称</th><th>归属</th><th>尺寸</th><th>上传时间</th><th>操作</th></tr></thead><tbody>
      {detail.evidence_files.map(file => <tr key={`${file.source_type}-${file.id}`}>
        <td data-label="文件名称" className="mobile-card-primary" style={{ fontFamily: 'var(--mono)', fontWeight: 600, fontSize: '12px', overflowWrap: 'anywhere' }}>{file.file_name}</td><td data-label="归属">{file.source_label}</td>
        <td data-label="尺寸" style={{ fontFamily: 'var(--mono)' }}>{(file.file_size / 1024 / 1024).toFixed(1)} MB</td>
        <td data-label="上传时间" style={{ fontFamily: 'var(--mono)', fontSize: '12px' }}>{formatChinaDateTime(file.uploaded_at, { seconds: false })}</td>
        <td data-label="操作"><button type="button" className="action-btn" onClick={async () => { try { const result = await api.downloadPlayerEvidence(token, detail.profile.steamid64, file.source_type, file.id); if (result.url) window.open(result.url, '_blank', 'noopener,noreferrer'); } catch (error) { toast({ title: '下载失败', message: error.message, tone: 'danger' }); } }}>下载</button></td>
      </tr>)}
    </tbody></table></div>}
  </div></div>;
}

// ── 审计 ──────────────────────────────────────────────────────────────────

function AuditTab({ detail }) {
  const aa = detail.admin_actions || [];
  const al = detail.audit_logs || [];
  return <div className="lower-grid">
    <div className="card"><div className="card-header"><div><div className="card-title">玩家 Web 端操作日志</div></div></div><div className="card-body">
      {aa.length === 0 ? <Empty>暂无操作日志。</Empty> : <div className="log-timeline">{aa.slice(0, 20).map((item, i) => <div className="log-item" key={`admin-${i}`}><div className="log-meta"><span>{formatChinaDateTime(item.created_at, { seconds: false })}</span><span>IP: {item.ip_address || '-'}</span></div><div className="log-content">{item.module}/{item.action}: {item.target_detail}</div></div>)}</div>}
    </div></div>
    <div className="card"><div className="card-header"><div><div className="card-title">管理员审计记录</div></div></div><div className="card-body">
      {al.length === 0 ? <Empty>暂无审计记录。</Empty> : <div className="log-timeline">{al.slice(0, 20).map((item, i) => <div className={`log-item ${!item.success ? 'critical' : ''}`} key={`audit-${item.id || i}`}><div className="log-meta"><span>{formatChinaDateTime(item.created_at, { seconds: false })}</span><span style={{ color: 'var(--text)' }}>操作人: {item.operator_name || '-'}</span></div><div className="log-content" style={!item.success ? { borderLeft: '3px solid var(--danger-dot)' } : undefined}><strong>{item.operation}</strong>{item.message ? `: ${item.message}` : ''}</div></div>)}</div>}
    </div></div>
  </div>;
}

// ── 风险与战绩 ────────────────────────────────────────────────────────────

function RiskTab({ detail, globalBans }) {
  const summary = detail.summary || {};
  const riskProfile = detail.risk_profile;
  const currentOnline = detail.online_records || [];
  const internalTags = detail.internal_profile?.tags || [];
  const linkedLocalBans = count(summary.linked_banned_account_count);
  const linkedGlobalBans = count(summary.linked_global_banned_account_count);
  const linkedRiskBans = linkedLocalBans + linkedGlobalBans;
  const qqLinkedCount = count(riskProfile?.qq_linked_account_count);
  const qqLocalBans = count(riskProfile?.qq_linked_banned_account_count);
  const qqGlobalBans = count(riskProfile?.qq_linked_global_banned_account_count);
  const qqRiskBans = qqLocalBans + qqGlobalBans;
  const activeGlobalCount = countActiveGlobalBans(globalBans || []);

  return <>
    <div className="player-dossier-overview">
      <div className="card">
        <div className="card-header"><div><div className="card-title">关键结论</div><div className="card-sub">优先展示会影响处理判断的信息。</div></div></div>
        <div className="card-body">
          <div className="player-insight-list">
            <div className="player-insight-item">
              <span className={`player-insight-dot ${count(summary.active_ban_count) + activeGlobalCount > 0 ? 'danger' : 'success'}`}></span>
              <div><strong>封禁状态</strong><p>{count(summary.active_ban_count) > 0 ? `存在 ${summary.active_ban_count} 条本地有效封禁。` : '未发现本地有效封禁。'}{activeGlobalCount > 0 ? `另有 ${activeGlobalCount} 条有效全球封禁。` : ''}</p></div>
            </div>
            <div className="player-insight-item">
              <span className={`player-insight-dot ${currentOnline.length > 0 ? 'success' : 'default'}`}></span>
              <div><strong>服务器状态</strong><p>{currentOnline.length > 0 ? `正在 ${currentOnline.map(i => `${i.server_name}:${i.server_port}`).join('、')} 中。` : '当前在线快照中未发现该玩家。'}</p></div>
            </div>
            <div className="player-insight-item">
              <span className={`player-insight-dot ${linkedRiskBans > 0 ? 'danger' : 'default'}`}></span>
              <div><strong>IP 关联</strong><p>{summary.linked_account_count > 0 ? `共发现 ${summary.linked_account_count} 个同 IP 账号，其中本地封禁 ${linkedLocalBans} 个、全球封禁 ${linkedGlobalBans} 个。` : '暂无同 IP 关联账号。'}</p></div>
            </div>
            <div className="player-insight-item">
              <span className={`player-insight-dot ${qqRiskBans > 0 ? 'danger' : 'default'}`}></span>
              <div><strong>QQ 关联</strong><p>{riskProfile?.qq_openid ? `已绑定 QQ，同 QQ 账号 ${qqLinkedCount} 个，其中本地封禁 ${qqLocalBans} 个、全球封禁 ${qqGlobalBans} 个。` : '该账号尚未绑定 QQ。'}</p></div>
            </div>
            <div className="player-insight-item">
              <span className={`player-insight-dot ${riskProfile?.action === 'deny' || riskProfile?.action === 'require_force' ? 'danger' : riskProfile?.action === 'warn' ? 'warning' : 'success'}`}></span>
              <div><strong>白名单建议</strong><p>{riskProfile?.recommendation || '可以按正常流程审核。'}{riskProfile?.summary ? ` ${riskProfile.summary}` : ''}</p></div>
            </div>
            <div className="player-insight-item">
              <span className={`player-insight-dot ${internalTags.length > 0 ? 'warning' : 'default'}`}></span>
              <div><strong>内部标记</strong><p>{internalTags.length > 0 ? internalTags.map(t => `#${t}`).join(' ') : (detail.internal_profile?.note || '暂无内部备注或标签。')}</p></div>
            </div>
          </div>
          <GokzPanel gokz={detail.gokz} />
        </div>
      </div>
      <div className="card">
        <div className="card-header"><div><div className="card-title">风险评分解释</div><div className="card-sub">每个结论都显示对应证据，便于复核和申诉。</div></div></div>
        <div className="card-body">
          {!riskProfile?.reasons?.length ? <Empty>暂无风险信号。</Empty> : <div className="player-risk-reasons">
            {riskProfile.reasons.map((reason, index) => <div className={`player-risk-reason ${reason.severity}`} key={`${reason.code}-${index}`}>
              <strong>{reason.message}</strong>
              {(reason.ip || reason.steamid64 || reason.count) && <div className="player-table-sub">{reason.ip ? `IP: ${reason.ip} ` : ''}{reason.steamid64 ? `账号: ${reason.steamid64} ` : ''}{reason.count ? `次数: ${reason.count}` : ''}</div>}
            </div>)}
          </div>}
        </div>
      </div>
    </div>
  </>;
}

// ── 身份 / 联系方式 / 内部备注 ─────────────────────────────────────────────

function InternalNoteModal({ detail, saving, tagCatalog, onClose, onSave }) {
  const [note, setNote] = useState(() => detail?.internal_profile?.note ?? '');
  const [tagText, setTagText] = useState(() => tagsToText(detail?.internal_profile?.tags ?? []));

  const tags = textToTags(tagText);

  return <Modal
    open
    title="编辑内部备注"
    subtitle="仅后台管理员可见，用于记录处理依据、沟通结论与风险标记。"
    onClose={onClose}
    wide
    footer={<>
      <button className="btn btn-outline" type="button" onClick={onClose}>取消</button>
      <button className="btn btn-primary" type="button" disabled={saving} onClick={() => onSave({ note, tags })}>
        {saving ? '保存中…' : '保存备注'}
      </button>
    </>}
  >
    <div className="form-group">
      <label>备注内容</label>
      <textarea
        className="form-control"
        rows={6}
        maxLength={2000}
        value={note}
        placeholder="例如：2026-10-16 已确认其与 xxx 为同一人；沟通后承诺不再使用违规配置…"
        onChange={event => setNote(event.target.value)}
      />
      <div className="pd-field-hint">{note.length} / 2000</div>
    </div>
    <div className="form-group">
      <label>标签</label>
      <input
        className="form-control"
        value={tagText}
        placeholder="多个标签用逗号分隔，如：重点关注, 代练嫌疑"
        onChange={event => setTagText(event.target.value)}
      />
      {tags.length > 0 && <div className="pd-tag-preview">
        {tags.map(tag => <span className="pd-tag" key={tag}>#{tag}</span>)}
      </div>}
      {tagCatalog.length > 0 && <div className="pd-tag-catalog">
        <span className="pd-field-hint">常用标签：</span>
        {tagCatalog.map(tag => <button
          type="button"
          key={tag.id}
          className={`pd-tag pd-tag-button ${tags.some(item => item.toLowerCase() === tag.name.toLowerCase()) ? 'active' : ''}`}
          style={tag.color ? { borderColor: tag.color, color: tag.color } : undefined}
          onClick={() => setTagText(current => textToTags(`${current},${tag.name}`).join(', '))}
        >#{tag.name}</button>)}
      </div>}
    </div>
  </Modal>;
}

/** 顶部身份卡：Steam 名称 / 三个 SteamID / QQ 绑定与联系方式 / 内部备注。 */
function IdentityHero({ detail, globalBans, canEdit, onEditNote, internalSaving, onShowGlobalBans }) {
  const profile = detail.profile || {};
  const summary = detail.summary || {};
  const currentOnline = detail.online_records || [];
  const risk = detail.risk_profile;
  const activeGlobalCount = countActiveGlobalBans(globalBans || []);
  const activeBanCount = count(summary.active_ban_count);
  const tags = detail.internal_profile?.tags || [];
  const note = detail.internal_profile?.note || '';
  const contact = latestWhitelistContact(detail);
  const binding = detail.qq_binding;
  const onlineText = currentOnline.length > 0
    ? currentOnline.map(item => `${item.server_name}${item.server_port ? `:${item.server_port}` : ''}`).filter(Boolean).join(' / ')
    : '未在线';

  return <section className="pd-hero">
    <div className="pd-hero-identity">
      <div className={`pd-hero-avatar ${activeBanCount + activeGlobalCount > 0 ? 'danger' : ''}`}>
        {(profile.display_name || profile.steamid64 || '?').slice(0, 2).toUpperCase()}
      </div>
      <div className="pd-hero-name-block">
        <div className="pd-hero-name" title={profile.display_name || profile.steamid64}>
          {profile.display_name || profile.steamid64 || '未知玩家'}
        </div>
        <div className="pd-hero-sub">
          {summary.last_seen_at ? `最后活跃 ${relativeTime(summary.last_seen_at)}` : '暂无活跃记录'}
          <span className="pd-dot-sep">·</span>
          首次记录 {detail.timeline?.length ? relativeTime(detail.timeline[detail.timeline.length - 1].occurred_at) : '-'}
        </div>
        <div className="pd-hero-pills">
          <StatusPill kind={currentOnline.length > 0 ? 'success' : 'default'}>{onlineText}</StatusPill>
          <StatusPill kind={activeBanCount + activeGlobalCount > 0 ? 'danger' : 'success'}>
            {activeBanCount + activeGlobalCount > 0 ? `有效封禁 ${activeBanCount + activeGlobalCount}` : '无有效封禁'}
          </StatusPill>
          <StatusPill kind={summary.whitelist_status === 'approved' ? 'success' : summary.whitelist_status === 'pending' ? 'warning' : summary.whitelist_status ? 'danger' : 'default'}>
            白名单 {stLabel(summary.whitelist_status)}
          </StatusPill>
          <StatusPill kind={riskTone(risk?.action)}>风险 {riskLabel(risk?.action)}</StatusPill>
          {profile.profile_url && <a className="pd-hero-link" href={profile.profile_url} target="_blank" rel="noreferrer noopener">
            Steam 主页
            <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6" /><polyline points="15 3 21 3 21 9" /><line x1="10" y1="14" x2="21" y2="3" /></svg>
          </a>}
        </div>
      </div>
      {(activeGlobalCount > 0 || (globalBans || []).length > 0) && <button type="button" className={`pd-hero-alert ${activeGlobalCount > 0 ? 'danger' : ''}`} onClick={onShowGlobalBans}>
        <strong>{activeGlobalCount > 0 ? '存在活跃全球封禁' : '存在历史全球封禁'}</strong>
        <span>{globalBans.length} 条记录 · 点击查看</span>
      </button>}
    </div>

    <div className="pd-hero-grid">
      <div className="pd-hero-col">
        <div className="pd-hero-col-title">Steam 标识</div>
        <div className="pd-id-grid">
          <CopyId label="SteamID64" value={profile.steamid64} />
          <CopyId label="SteamID2" value={profile.steamid} />
          <CopyId label="SteamID3" value={profile.steamid3} />
        </div>
      </div>

      <div className="pd-hero-col">
        <div className="pd-hero-col-title">QQ 绑定 / 联系方式</div>
        <div className="pd-contact-list">
          <div className="pd-contact-row">
            <span>白名单联系方式</span>
            <strong>{contact || <em>未填写</em>}</strong>
          </div>
          <div className="pd-contact-row">
            <span>QQ 绑定</span>
            <strong>{binding
              ? <>{binding.qq_username || '未知昵称'}<code className="pd-inline-code">{binding.qq_openid}</code></>
              : <em>未绑定</em>}</strong>
          </div>
          <div className="pd-contact-row">
            <span>绑定时间</span>
            <strong>{binding ? formatChinaDateTime(binding.verified_at, { seconds: false }) : '-'}</strong>
          </div>
          <div className="pd-contact-row">
            <span>同 QQ 账号</span>
            <strong>
              {count(risk?.qq_linked_account_count)}
              {count(risk?.qq_linked_banned_account_count) + count(risk?.qq_linked_global_banned_account_count) > 0
                ? <span className="pd-contact-danger"> · 含封禁 {count(risk?.qq_linked_banned_account_count) + count(risk?.qq_linked_global_banned_account_count)} 个</span>
                : null}
            </strong>
          </div>
        </div>
        <div className="pd-hero-col-note">完整 QQ 绑定与私聊记录见「IP / QQ 关联」页。</div>
      </div>

      <div className="pd-hero-col pd-hero-col-note-block">
        <div className="pd-hero-col-title">
          内部备注
          {canEdit && <button type="button" className="pd-note-edit" onClick={onEditNote} disabled={internalSaving}>
            {internalSaving ? '保存中…' : '编辑'}
          </button>}
        </div>
        {note || tags.length > 0 ? <>
          {note && <p className="pd-note-text">{note}</p>}
          {tags.length > 0 && <div className="pd-note-tags">{tags.map(tag => <span className="pd-tag" key={tag}>#{tag}</span>)}</div>}
        </> : <div className="pd-note-empty">暂无备注。{canEdit ? '点击「编辑」记录处理结论、沟通情况或风险标记。' : ''}</div>}
        {detail.internal_note_history?.length > 0 && <details className="pd-note-history">
          <summary>备注变更历史（{detail.internal_note_history.length}）</summary>
          <div>
            {detail.internal_note_history.slice(0, 8).map(item => <div className="pd-note-history-item" key={item.id}>
              <div className="pd-note-history-head">
                <strong>{item.changed_by_name}</strong>
                <span>{formatChinaDateTime(item.changed_at, { seconds: false })}</span>
              </div>
              <div className="pd-note-history-body">
                {item.note || '（清空备注）'}
                {item.tags?.length ? <span className="pd-note-history-tags">{item.tags.map(tag => `#${tag}`).join(' ')}</span> : null}
              </div>
            </div>)}
          </div>
        </details>}
      </div>
    </div>
  </section>;
}

/** 顶部统计条：关键计数与时长。 */
function StatStrip({ detail, globalBans }) {
  const summary = detail.summary || {};
  const stats = detail.activity_stats;
  const risk = detail.risk_profile;
  const activeGlobal = countActiveGlobalBans(globalBans || []);
  const items = [
    { label: '最后活跃', value: summary.last_seen_at ? relativeTime(summary.last_seen_at) : '-', tone: 'default' },
    { label: '累计时长', value: minutesLabel(stats?.total_play_minutes), tone: 'default' },
    { label: '服务器会话', value: (detail.player_sessions || []).length, tone: 'default' },
    { label: '进服成功', value: count(summary.access_success_count), tone: 'success' },
    { label: '进服失败', value: count(summary.access_failure_count), tone: count(summary.access_failure_count) > 0 ? 'danger' : 'default' },
    { label: '本地有效封禁', value: count(summary.active_ban_count), tone: count(summary.active_ban_count) > 0 ? 'danger' : 'success' },
    { label: '全球封禁', value: activeGlobal, tone: activeGlobal > 0 ? 'danger' : 'default' },
    { label: '同 IP 账号', value: count(summary.linked_account_count), tone: count(summary.linked_banned_account_count) + count(summary.linked_global_banned_account_count) > 0 ? 'warning' : 'default' },
    { label: '同 QQ 账号', value: count(risk?.qq_linked_account_count), tone: count(risk?.qq_linked_banned_account_count) + count(risk?.qq_linked_global_banned_account_count) > 0 ? 'danger' : 'default' },
    { label: '时间线事件', value: count(detail.timeline_total ?? (detail.timeline || []).length), tone: 'default' },
  ];
  return <div className="pd-stat-strip">
    {items.map(item => <div className={`pd-stat tone-${item.tone}`} key={item.label}>
      <span>{item.label}</span>
      <strong>{item.value}</strong>
    </div>)}
  </div>;
}

const TABS = [
  { key: 'timeline', label: '时间线' },
  { key: 'access', label: '进服与在线' },
  { key: 'status', label: '封禁 / 白名单' },
  { key: 'network', label: 'IP / QQ 关联' },
  { key: 'risk', label: '风险与战绩' },
  { key: 'evidence', label: '证据' },
  { key: 'audit', label: '审计' },
];

export function PlayerDetailPage() {
  const { session } = useAuth();
  const { toast } = useToast();
  const queryClient = useQueryClient();
  const token = session?.token ?? null;
  const canEdit = session?.permissions?.includes('player_internal.manage') || session?.role === 'developer' || session?.role === 'admin';
  const [steamInput, setSteamInput] = useState('');
  const [lastQuery, setLastQuery] = useState('');
  const lastQueryRef = useRef('');
  const [detail, setDetail] = useState(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState('');
  const [tab, setTab] = useState('timeline');
  const [internalSaving, setInternalSaving] = useState(false);
  const [noteModalOpen, setNoteModalOpen] = useState(false);
  const [tagCatalog, setTagCatalog] = useState([]);
  const [globalBans, setGlobalBans] = useState(null);
  const [showGlobalBans, setShowGlobalBans] = useState(false);
  const [candidates, setCandidates] = useState([]);
  const [candidatesOpen, setCandidatesOpen] = useState(false);
  const [candidatesLoading, setCandidatesLoading] = useState(false);
  const [candidateError, setCandidateError] = useState('');
  const [activeCandidateIndex, setActiveCandidateIndex] = useState(-1);
  const [banFormOpen, setBanFormOpen] = useState(false);
  const searchWrapRef = useRef(null);
  const suppressCandidateSearchRef = useRef(false);
  // 与封禁管理页一致：ban.create 权限或 developer/admin 角色可发起封禁
  const canCreateBan = session?.permissions?.includes('ban.create') || session?.permissions?.includes('ban.manage') || session?.role === 'developer' || session?.role === 'admin';

  useEffect(() => {
    if (!token) return;
    api.playerTags(token).then(result => setTagCatalog(result.items || [])).catch(() => setTagCatalog([]));
  }, [token]);

  // 支持 /player-detail?steamid=xxx 直接查询（如从在线玩家卡片跳转）
  useEffect(() => {
    if (!token) return;
    const urlSteamid = new URLSearchParams(window.location.search).get('steamid');
    if (urlSteamid && urlSteamid.trim()) loadDetail(urlSteamid.trim());
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [token]);

  // 加载玩家详情
  async function loadDetail(input, resetTab = true) {
    const q = input.trim(); if (!q) { setError('请输入 SteamID。'); return; }
    setCandidates([]); setCandidateError(''); setCandidatesLoading(false); setCandidatesOpen(false); setActiveCandidateIndex(-1);
    try { setLoading(true); setError(''); const r = await api.playerDetail(token, q); setDetail(r.data); const nextInput = r.data?.profile?.display_name || r.data?.profile?.steamid64 || q; if (nextInput !== steamInput) { suppressCandidateSearchRef.current = true; setSteamInput(nextInput); } else { suppressCandidateSearchRef.current = false; } setLastQuery(q); lastQueryRef.current = q; if (resetTab) setTab('timeline'); } catch (e) { setDetail(null); setError(e.message || '加载失败'); } finally { setLoading(false); }
  }
  function selectCandidate(candidate) { if (!candidate) return; loadDetail(candidate.steamid64); }
  function submitSearch() {
    if (candidatesLoading) { setCandidatesOpen(true); return; }
    const picked = activeCandidateIndex >= 0 ? candidates[activeCandidateIndex] : candidates.length === 1 ? candidates[0] : null;
    if (picked) { selectCandidate(picked); return; }
    if (candidates.length > 1) { setCandidatesOpen(true); setError('找到多个匹配玩家，请在下拉框中选择。'); return; }
    loadDetail(steamInput);
  }
  function handleSearchKeyDown(e) {
    if (!candidatesOpen || candidates.length === 0) { if (e.key === 'Escape') setCandidatesOpen(false); return; }
    if (e.key === 'ArrowDown') { e.preventDefault(); setActiveCandidateIndex(i => (i + 1) % candidates.length); }
    else if (e.key === 'ArrowUp') { e.preventDefault(); setActiveCandidateIndex(i => (i <= 0 ? candidates.length - 1 : i - 1)); }
    else if (e.key === 'Enter' && activeCandidateIndex >= 0) { e.preventDefault(); selectCandidate(candidates[activeCandidateIndex]); }
    else if (e.key === 'Escape') { e.preventDefault(); setCandidatesOpen(false); setActiveCandidateIndex(-1); }
  }
  function refreshDetail() { const q = lastQueryRef.current || lastQuery; if (q) loadDetail(q, false); queryClient.invalidateQueries({ queryKey: ['whitelist'] }); queryClient.invalidateQueries({ queryKey: ['bans'] }); }
  async function exportReport() {
    if (!detail?.profile?.steamid64) return;
    try {
      const report = await api.playerReport(token, detail.profile.steamid64);
      const blob = new Blob([JSON.stringify(report, null, 2)], { type: 'application/json;charset=utf-8' });
      const url = URL.createObjectURL(blob);
      const link = document.createElement('a');
      link.href = url;
      link.download = `player-investigation-${detail.profile.steamid64}.json`;
      link.click();
      URL.revokeObjectURL(url);
      toast({ title: '调查报告已导出' });
    } catch (error) { toast({ title: '导出失败', message: error.message, tone: 'danger' }); }
  }
  // 内部备注
  async function handleSaveInternal(body) {
    if (!detail) return;
    try {
      setInternalSaving(true);
      const r = await api.updatePlayerInternalProfile(token, detail.profile.steamid64, body);
      const history = await api.playerInternalNoteHistory(token, detail.profile.steamid64).catch(() => null);
      setDetail(p => p ? { ...p, internal_profile: r.item, internal_note_history: history?.items || p.internal_note_history } : p);
      setNoteModalOpen(false);
      toast({ title: '保存成功' });
    } catch (e) { toast({ title: '保存失败', message: e.message, tone: 'danger' }); } finally { setInternalSaving(false); }
  }

  // 查询全球封禁
  useEffect(() => {
    if (!detail?.profile?.steamid64) { React.startTransition(() => setGlobalBans(null)); return; }
    let c = false; (async () => { try { const r = await api.searchGlobalBans(token, { steam_input: detail.profile.steamid64 }); if (!c) setGlobalBans(r.items || []); } catch { if (!c) setGlobalBans(null); } })();
    return () => { c = true; };
  }, [detail?.profile?.steamid64, token]);

  // 强制刷新进服资料（rating / steam 等级）：误拒玩家的秒级修复入口
  const [profileRefreshing, setProfileRefreshing] = useState(false);
  async function refreshAccessProfile() {
    const steamid64 = detail?.profile?.steamid64;
    if (!steamid64 || profileRefreshing) return;
    setProfileRefreshing(true);
    try {
      const r = await api.refreshPlayerAccessProfile(token, steamid64);
      if (r.refreshed) {
        toast({ title: `进服资料已刷新：Rating ${r.rating ?? '-'}，Steam 等级 ${r.steam_level ?? '-'}` });
        refreshDetail();
      } else {
        toast({ title: '资料刷新失败', message: '外部数据暂时不可用，请稍后重试', tone: 'warning' });
      }
    } catch (e) {
      toast({ title: '资料刷新失败', message: e.message, tone: 'danger' });
    } finally {
      setProfileRefreshing(false);
    }
  }

  useEffect(() => {
    function closeOnOutsideClick(event) {
      if (searchWrapRef.current && !searchWrapRef.current.contains(event.target)) {
        setCandidatesOpen(false);
        setActiveCandidateIndex(-1);
      }
    }
    document.addEventListener('mousedown', closeOnOutsideClick);
    return () => document.removeEventListener('mousedown', closeOnOutsideClick);
  }, []);

  useEffect(() => {
    const q = steamInput.trim();
    if (!token || q.length < 2) { suppressCandidateSearchRef.current = false; React.startTransition(() => { setCandidates([]); setCandidateError(''); setCandidatesLoading(false); setActiveCandidateIndex(-1); }); return; }
    if (suppressCandidateSearchRef.current) { suppressCandidateSearchRef.current = false; React.startTransition(() => { setCandidates([]); setCandidateError(''); setCandidatesOpen(false); setCandidatesLoading(false); setActiveCandidateIndex(-1); }); return; }
    let cancelled = false;
    React.startTransition(() => setCandidatesLoading(true));
    const timer = window.setTimeout(async () => {
      try {
        const r = await api.playerDetailCandidates(token, q);
        if (cancelled) return;
        const items = r.items || [];
        setCandidates(items);
        setCandidateError('');
        setCandidatesOpen(Boolean(document.activeElement && searchWrapRef.current?.contains(document.activeElement)));
        setActiveCandidateIndex(items.length === 1 ? 0 : -1);
      } catch (e) {
        if (cancelled) return;
        setCandidates([]);
        setCandidateError(e.message || '候选查询失败');
        setCandidatesOpen(true);
        setActiveCandidateIndex(-1);
      } finally {
        if (!cancelled) setCandidatesLoading(false);
      }
    }, 260);
    return () => { cancelled = true; window.clearTimeout(timer); };
  }, [steamInput, token]);

  return (<div id="player-detail" className="content-section active">
    <div className="breadcrumb"><span>核心管理</span><span className="sep">›</span><span className="current">玩家全息档案</span></div>
    <div className="page-header"><div><div className="page-title">玩家详情</div><div className="page-sub">以时间线还原玩家行为轨迹：服务器进出、进服判定、白名单、封禁、管理员操作一目了然。</div></div><div style={{ display: 'flex', gap: 8, flexWrap: 'wrap' }}>
      {detail && canCreateBan && <button type="button" className="btn btn-danger" onClick={() => setBanFormOpen(true)}>发起封禁</button>}
      {detail && <button type="button" className="btn btn-outline" onClick={refreshAccessProfile} disabled={profileRefreshing}>{profileRefreshing ? '刷新资料中...' : '刷新进服资料'}</button>}
      {detail && <button type="button" className="btn btn-outline" onClick={exportReport}>导出调查报告</button>}
    </div></div>

    <div className="card player-search-card">
      <form className="player-detail-search" onSubmit={e => { e.preventDefault(); submitSearch(); }}>
        <div className="player-search-combobox" ref={searchWrapRef}>
          <div className="search-bar-box">
            <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" width="15" height="15" style={{ flexShrink: 0, color: 'var(--text3)' }}><circle cx="7" cy="7" r="4.5" /><path d="M10.5 10.5L14 14" /></svg>
            <input
              type="text"
              placeholder="输入玩家名称 / SteamID64 / SteamID2 / SteamID3 / IP / 联系方式..."
              value={steamInput}
              autoComplete="off"
              role="combobox"
              aria-expanded={candidatesOpen}
              aria-controls="player-search-candidates"
              aria-autocomplete="list"
              aria-activedescendant={activeCandidateIndex >= 0 ? `player-candidate-${activeCandidateIndex}` : undefined}
              onFocus={() => { if (steamInput.trim().length >= 2 && (candidates.length > 0 || candidateError || candidatesLoading)) setCandidatesOpen(true); }}
              onChange={e => { setSteamInput(e.target.value); setError(''); setCandidatesOpen(true); }}
              onKeyDown={handleSearchKeyDown}
            />
            {candidatesLoading && <span className="player-search-spinner" aria-hidden="true" />}
          </div>
          {candidatesOpen && steamInput.trim().length >= 2 && (
            <div className="player-candidate-menu" id="player-search-candidates" role="listbox">
              {candidateError ? <div className="player-candidate-state">{candidateError}</div> : null}
              {!candidateError && candidatesLoading && candidates.length === 0 ? <div className="player-candidate-state">正在匹配玩家...</div> : null}
              {!candidateError && !candidatesLoading && candidates.length === 0 ? <div className="player-candidate-state">没有找到匹配玩家</div> : null}
              {!candidateError && candidates.map((candidate, index) => {
                const meta = candidateMeta(candidate);
                return <button
                  key={candidate.steamid64}
                  id={`player-candidate-${index}`}
                  className={`player-candidate-item ${index === activeCandidateIndex ? 'active' : ''}`}
                  type="button"
                  role="option"
                  aria-selected={index === activeCandidateIndex}
                  onMouseEnter={() => setActiveCandidateIndex(index)}
                  onClick={() => selectCandidate(candidate)}
                >
                  <span className="player-candidate-main">
                    <span className="player-candidate-name">{candidate.display_name || candidate.steamid64}</span>
                    <span className="player-candidate-id">{candidate.steamid64}</span>
                  </span>
                  <span className="player-candidate-secondary">
                    {candidate.steamid && <span>{candidate.steamid}</span>}
                    {candidate.steamid3 && <span>{candidate.steamid3}</span>}
                  </span>
                  <span className="player-candidate-meta">
                    {candidate.sources.slice(0, 4).map(source => <span className="player-candidate-source" key={source}>{source}</span>)}
                    {meta.map(text => <span key={text}>{text}</span>)}
                  </span>
                </button>;
              })}
            </div>
          )}
        </div>
        {lastQuery && <button className="btn btn-outline" type="button" disabled={loading} onClick={refreshDetail}>刷新</button>}
      </form>
      {error && <div className="player-detail-error">{error}</div>}
    </div>

    {!detail && !loading ? (<div className="player-welcome-card"><div className="player-welcome-inner"><div className="player-welcome-icon"><svg viewBox="0 0 24 24" fill="none" stroke="#fff" strokeWidth="2" width="28" height="28"><circle cx="9" cy="7" r="4" /><path d="M2 21v-2a4 4 0 0 1 4-4h6a4 4 0 0 1 4 4v2" /></svg></div><div className="player-welcome-title">查询玩家全息档案</div><div className="player-welcome-desc">输入 SteamID、玩家名称或联系方式，查看身份信息、QQ 绑定、内部备注与完整行为时间线。</div></div></div>) : null}

    {detail ? <>
      <IdentityHero
        detail={detail}
        globalBans={globalBans}
        canEdit={canEdit}
        internalSaving={internalSaving}
        onEditNote={() => setNoteModalOpen(true)}
        onShowGlobalBans={() => setShowGlobalBans(true)}
      />

      <StatStrip detail={detail} globalBans={globalBans} />

      <div className="profile-tabs player-detail-tabs">
        {TABS.map(t => <button key={t.key} className={`profile-tab ${tab === t.key ? 'active' : ''}`} onClick={() => setTab(t.key)} type="button">
          {t.label}
          {t.key === 'timeline' && detail.timeline?.length > 0 && <span className="profile-tab-badge">{detail.timeline_total ?? detail.timeline.length}</span>}
        </button>)}
      </div>
      <div className="profile-tab-pane active">
        {tab === 'timeline' && <TimelineTab detail={detail} />}
        {tab === 'access' && <AccessTab detail={detail} />}
        {tab === 'status' && <StatusTab detail={detail} token={token} onRefresh={refreshDetail} />}
        {tab === 'network' && <NetworkTab detail={detail} token={token} />}
        {tab === 'risk' && <RiskTab detail={detail} globalBans={globalBans} />}
        {tab === 'evidence' && <EvidenceTab detail={detail} token={token} />}
        {tab === 'audit' && <AuditTab detail={detail} />}
      </div>
    </> : null}

    {showGlobalBans && <GlobalBanPopup items={globalBans || []} onClose={() => setShowGlobalBans(false)} />}

    {detail && noteModalOpen && <InternalNoteModal
      detail={detail}
      saving={internalSaving}
      tagCatalog={tagCatalog}
      onClose={() => setNoteModalOpen(false)}
      onSave={handleSaveInternal}
    />}

    {/* 快捷封禁：预填当前玩家信息，成功后刷新详情 */}
    {detail && <BanFormModal
      open={banFormOpen}
      mode="create"
      prefillForm={{
        player: detail.profile?.display_name || '',
        steam_id: detail.profile?.steamid64 || '',
      }}
      onClose={() => setBanFormOpen(false)}
      onSuccess={() => { setBanFormOpen(false); refreshDetail(); }}
      token={token}
    />}
  </div>);
}

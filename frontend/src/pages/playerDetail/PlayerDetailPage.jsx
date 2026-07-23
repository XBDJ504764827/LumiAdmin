import React, { useEffect, useRef, useState } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { api } from '../../lib/api.js';
import { useAuth } from '../../state/store.js';
import { useToast } from '../../shared/Toast.jsx';
import { useConfirmDialog } from '../../shared/ConfirmModal.jsx';
import { StatusPill } from '../../shared/StatusPill.jsx';
import { Modal } from '../../shared/Modal.jsx';
import { formatChinaDateTime } from '../../shared/time.js';
import {
  candidateMeta,
  categoryKind,
  categoryLabel,
  count,
  countActiveGlobalBans,
  durLabel,
  eventSourceLabel,
  failureLabel,
  feedbackTypeLabel,
  latestItems,
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

function Empty({children}){return<div className="player-detail-empty">{children}</div>}

// ── Ban Detail Popup ──
function BanDetailPopup({item, onClose, onAction, token}) {
  const { toast } = useToast();
  const { confirm } = useConfirmDialog();
  const [acting, setActing] = useState(false);
  if (!item) return null;
  const isActive = item.status === 'active';

  async function doUnban() { if(acting)return;setActing(true);try{await api.unban(token,item.id);toast({title:'解封成功'});onAction();onClose();}catch(e){toast({title:'操作失败',message:e.message,tone:'danger'})}finally{setActing(false)} }
  async function doDelete() { if(acting)return; const ok = await confirm({ title: '删除封禁记录', message: '确定删除这条封禁记录吗？此操作不可恢复。', tone: 'danger', confirmText: '确认删除' }); if(!ok)return;setActing(true);try{await api.deleteBan(token,item.id);toast({title:'删除成功'});onAction();onClose();}catch(e){toast({title:'操作失败',message:e.message,tone:'danger'})}finally{setActing(false)} }

  return (<>
    <Modal open={true} title="封禁详情" onClose={onClose} footer={<><button className="btn btn-outline" onClick={onClose}>关闭</button>{isActive&&<><button className="action-btn action-btn-success" onClick={doUnban} disabled={acting}>解封</button><button className="action-btn action-btn-danger" onClick={doDelete} disabled={acting}>删除</button></>}</>} >
    <div className="detail-grid">
      <span className="detail-label">状态</span><span className="detail-value"><StatusPill kind={stKind(item.status,'ban')}>{stLabel(item.status)}</StatusPill></span>
      <span className="detail-label">玩家</span><span className="detail-value fw-600">{item.player||'-'}</span>
      <span className="detail-label">SteamID64</span><span className="detail-value mono">{item.steam_id}</span>
      {item.ip_address&&<><span className="detail-label">IP</span><span className="detail-value mono">{item.ip_address}</span></>}
      {item.server_name&&<><span className="detail-label">服务器</span><span className="detail-value">{item.server_name}</span></>}
      <span className="detail-label">封禁类型</span><span className="detail-value">{item.ban_type==='ip'?'IP 封禁':'Steam 账号封禁'}</span>
      <span className="detail-label">时长</span><span className="detail-value">{durLabel(item.duration_minutes)}</span>
      <span className="detail-label">理由</span><span className="detail-value pre">{item.reason}</span>
      <span className="detail-label">操作人</span><span className="detail-value">{item.operator_name||'-'}</span>
      <span className="detail-label">封禁时间</span><span className="detail-value">{formatChinaDateTime(item.created_at)}</span>
      {item.removed_at&&<><span className="detail-label">解封时间</span><span className="detail-value">{formatChinaDateTime(item.removed_at)} {item.removed_by?`(${item.removed_by})`:''}</span></>}
    </div>
  </Modal>
  {dialog}
  </>);
}

// ── Whitelist Detail Popup ──
function WhitelistDetailPopup({item, onClose, onAction, token}) {
  const { toast } = useToast();
  const [acting, setActing] = useState(false);
  const [reason, setReason] = useState('');
  if (!item) return null;
  const isPending = item.status === 'pending';
  const isApproved = item.status === 'approved';
  const isRejected = item.status === 'rejected' || item.status === 'revoked';

  async function doAction(action) { if(acting)return;setActing(true);
    try{
      if(action==='reject'){
        if(!reason.trim()){toast({title:'请填写拒绝理由',tone:'warning'});setActing(false);return;}
        await api.rejectWhitelist(token,item.id,{reason:reason.trim()});
      } else if(action==='approve'){
        await api.approveWhitelist(token,item.id);
      } else if(action==='revoke'){
        await api.revokeWhitelist(token,item.id);
      } else if(action==='restore'){
        await api.restoreWhitelist(token,item.id);
      }
      toast({title:'操作成功'});onAction();onClose();
    }catch(e){toast({title:'操作失败',message:e.message,tone:'danger'})}finally{setActing(false)} }

  return (<Modal open={true} title="白名单详情" onClose={onClose} footer={<><button className="btn btn-outline" onClick={onClose}>关闭</button>
    {isPending&&<><button className="action-btn action-btn-success" onClick={()=>doAction('approve')} disabled={acting}>通过</button><button className="action-btn action-btn-danger" onClick={()=>doAction('reject')} disabled={acting||!reason.trim()}>拒绝</button></>}
    {isApproved&&<button className="action-btn action-btn-danger" onClick={()=>doAction('revoke')} disabled={acting}>撤销</button>}
    {isRejected&&<button className="action-btn action-btn-success" onClick={()=>doAction('restore')} disabled={acting}>恢复</button>}
  </>} >
    {isPending&&<div className="form-group" style={{marginBottom:12}}><label>拒绝理由（必填）</label><textarea className="form-control" rows={2} value={reason} onChange={e=>setReason(e.target.value)} placeholder="请输入拒绝理由" style={{resize:'vertical',minHeight:50}}/></div>}
    <div className="detail-grid">
      <span className="detail-label">状态</span><span className="detail-value"><StatusPill kind={stKind(item.status,'whitelist')}>{stLabel(item.status)}</StatusPill></span>
      <span className="detail-label">昵称</span><span className="detail-value fw-600">{item.nickname}</span>
      {item.contact&&<><span className="detail-label">联系方式</span><span className="detail-value pre">{item.contact}</span></>}
      <span className="detail-label">SteamID64</span><span className="detail-value mono">{item.steamid64}</span>
      {item.steam_persona_name&&<><span className="detail-label">Steam 名称</span><span className="detail-value">{item.steam_persona_name}</span></>}
      <span className="detail-label">申请时间</span><span className="detail-value">{formatChinaDateTime(item.applied_at)}</span>
      {item.approved_at&&<><span className="detail-label">通过时间</span><span className="detail-value">{formatChinaDateTime(item.approved_at)} ({item.approved_by||'-'})</span></>}
      {item.rejected_at&&<><span className="detail-label">拒绝时间</span><span className="detail-value">{formatChinaDateTime(item.rejected_at)} ({item.rejected_by||'-'})<br/>{item.rejection_reason||''}</span></>}
      {item.revoked_at&&<><span className="detail-label">撤销时间</span><span className="detail-value">{formatChinaDateTime(item.revoked_at)} ({item.revoked_by||'-'})</span></>}
      {item.approval_reason&&<><span className="detail-label">通过理由</span><span className="detail-value pre">{item.approval_reason}</span></>}
    </div>
  </Modal>);
}

// ── Appeal Detail Popup ──
function AppealDetailPopup({item, onClose, onAction, token}) {
  const { toast } = useToast();
  const [acting, setActing] = useState(false);
  const [note, setNote] = useState('');
  if (!item) return null;
  const isPending = item.status === 'pending';

  async function doReview(status) { if(acting)return;setActing(true);
    try{await api.rejectBanAppeal(token,item.id,{status,review_note:note.trim()||null});toast({title:'审核成功'});onAction();onClose();}catch(e){toast({title:'操作失败',message:e.message,tone:'danger'})}finally{setActing(false)} }

  return (<Modal open={true} title="申诉详情" onClose={onClose} footer={<><button className="btn btn-outline" onClick={onClose}>关闭</button>
    {isPending&&<><button className="action-btn action-btn-success" onClick={()=>doReview('approved')} disabled={acting}>通过</button>
    <button className="action-btn action-btn-danger" onClick={()=>doReview('rejected')} disabled={acting}>驳回</button></>}
  </>} >
    {isPending&&<div className="form-group" style={{marginBottom:12}}><label>审核备注（选填）</label><textarea className="form-control" rows={2} value={note} onChange={e=>setNote(e.target.value)} placeholder="可选，向玩家说明处理结果" style={{resize:'vertical',minHeight:50}}/></div>}
    <div className="detail-grid">
      <span className="detail-label">状态</span><span className="detail-value"><StatusPill kind={stKind(item.status,'appeal')}>{stLabel(item.status)}</StatusPill></span>
      <span className="detail-label">玩家</span><span className="detail-value fw-600">{item.player_name}</span>
      <span className="detail-label">申诉理由</span><span className="detail-value pre">{item.appeal_reason}</span>
      {item.ban_reason&&<><span className="detail-label">关联封禁</span><span className="detail-value">{item.ban_reason}</span></>}
      <span className="detail-label">提交时间</span><span className="detail-value">{formatChinaDateTime(item.created_at)}</span>
      {item.reviewed_by&&<><span className="detail-label">审核人</span><span className="detail-value">{item.reviewed_by}</span></>}
      {item.review_note&&<><span className="detail-label">审核备注</span><span className="detail-value pre">{item.review_note}</span></>}
    </div>
  </Modal>);
}

// ── Report Detail Popup ──
function ReportDetailPopup({item, onClose, onAction, token}) {
  const { toast } = useToast();
  const [acting, setActing] = useState(false);
  const [note, setNote] = useState('');
  if (!item) return null;
  const isPending = item.status === 'pending';

  async function doReview(status) { if(acting)return;setActing(true);
    try{await api.reviewPlayerReport(token,item.id,{status,review_note:note.trim()||null});toast({title:'审核成功'});onAction();onClose();}catch(e){toast({title:'操作失败',message:e.message,tone:'danger'})}finally{setActing(false)} }

  return (<Modal open={true} title="举报详情" onClose={onClose} footer={<><button className="btn btn-outline" onClick={onClose}>关闭</button>
    {isPending&&<><button className="action-btn action-btn-success" onClick={()=>doReview('approved')} disabled={acting}>通过（封禁）</button><button className="action-btn action-btn-danger" onClick={()=>doReview('rejected')} disabled={acting}>驳回</button></>}
  </>} >
    {isPending&&<div className="form-group" style={{marginBottom:12}}><label>审核备注（选填）</label><textarea className="form-control" rows={2} value={note} onChange={e=>setNote(e.target.value)} placeholder="可选，审核备注" style={{resize:'vertical',minHeight:50}}/></div>}
    <div className="detail-grid">
      <span className="detail-label">状态</span><span className="detail-value"><StatusPill kind={stKind(item.status,'report')}>{stLabel(item.status)}</StatusPill></span>
      <span className="detail-label">被举报玩家</span><span className="detail-value fw-600">{item.target_player_name||'-'}</span>
      <span className="detail-label">举报理由</span><span className="detail-value pre">{item.report_reason}</span>
      {item.reporter_contact&&<><span className="detail-label">举报人联系方式</span><span className="detail-value">{item.reporter_contact}</span></>}
      <span className="detail-label">提交时间</span><span className="detail-value">{formatChinaDateTime(item.created_at)}</span>
      {item.reviewed_by&&<><span className="detail-label">审核人</span><span className="detail-value">{item.reviewed_by}</span></>}
      {item.review_note&&<><span className="detail-label">审核备注</span><span className="detail-value pre">{item.review_note}</span></>}
    </div>
  </Modal>);
}

// ── Map Feedback Detail Popup ──
function MapFeedbackDetailPopup({item, onClose, onAction, token}) {
  const { toast } = useToast();
  const [acting, setActing] = useState(false);
  const [note, setNote] = useState('');
  if (!item) return null;
  const isPending = item.status === 'pending';

  async function doReview(status) { if(acting)return;setActing(true);
    try{await api.reviewMapFeedback(token,item.id,{status,review_note:note.trim()||null});toast({title:'审核成功'});onAction();onClose();}catch(e){toast({title:'操作失败',message:e.message,tone:'danger'})}finally{setActing(false)} }

  return (<Modal open={true} title="地图反馈详情" onClose={onClose} footer={<><button className="btn btn-outline" onClick={onClose}>关闭</button>
    {isPending&&<><button className="action-btn action-btn-success" onClick={()=>doReview('resolved')} disabled={acting}>已解决</button><button className="action-btn action-btn-danger" onClick={()=>doReview('rejected')} disabled={acting}>驳回</button></>}
  </>} >
    {isPending&&<div className="form-group" style={{marginBottom:12}}><label>回复备注（选填）</label><textarea className="form-control" rows={2} value={note} onChange={e=>setNote(e.target.value)} placeholder="可选，向玩家说明处理结果" style={{resize:'vertical',minHeight:50}}/></div>}
    <div className="detail-grid">
      <span className="detail-label">状态</span><span className="detail-value"><StatusPill kind={stKind(item.status)}>{stLabel(item.status)}</StatusPill></span>
      <span className="detail-label">反馈类型</span><span className="detail-value">{feedbackTypeLabel(item.feedback_type)}</span>
      <span className="detail-label">详细内容</span><span className="detail-value pre">{item.detail}</span>
      {item.steam_persona_name&&<><span className="detail-label">玩家昵称</span><span className="detail-value fw-600">{item.steam_persona_name}</span></>}
      {item.contact&&<><span className="detail-label">联系方式</span><span className="detail-value">{item.contact}</span></>}
      <span className="detail-label">提交时间</span><span className="detail-value">{formatChinaDateTime(item.created_at)}</span>
      {item.reviewed_by&&<><span className="detail-label">审核人</span><span className="detail-value">{item.reviewed_by}</span></>}
      {item.review_note&&<><span className="detail-label">回复备注</span><span className="detail-value pre">{item.review_note}</span></>}
      {item.reviewed_at&&<><span className="detail-label">审核时间</span><span className="detail-value">{formatChinaDateTime(item.reviewed_at)}</span></>}
    </div>
  </Modal>);
}

// ── Global Ban Popup ──
function GlobalBanPopup({items, onClose}) {
  if (!items||items.length===0) return null;
  function banTypeLabel(t){const m={bhop_hack:'连跳作弊',cheat:'作弊',tool_assist:'辅助工具',other:'其他'};return m[t]||t||'-'}
  function isPerma(e){return e&&e.startsWith('9999')}
  function expLabel(e){if(!e)return'永久';if(isPerma(e))return'永久';return formatChinaDateTime(e,{seconds:false})}
  return (<Modal open={true} title={`全球封禁详情 (${items.length} 条)`} onClose={onClose} wide footer={<button className="btn btn-outline" onClick={onClose}>关闭</button>}>
    <div className="table-responsive"><table className="data-table"><thead><tr><th>类型</th><th>到期</th><th>备注</th><th>封禁时间</th><th>本地状态</th></tr></thead><tbody>
      {items.map(item=>{const ban=item.ban;return(<tr key={ban.id}>
        <td><StatusPill kind="danger">{banTypeLabel(ban.ban_type)}</StatusPill></td>
        <td style={{whiteSpace:'nowrap'}}>{isPerma(ban.expires_on)?<span className="permanent-ban">永久</span>:expLabel(ban.expires_on)}</td>
        <td className="text-ellipsis" style={{maxWidth:200}} title={ban.notes||''}>{ban.notes||'-'}</td>
        <td style={{whiteSpace:'nowrap'}}>{formatChinaDateTime(ban.created_on,{seconds:false})}</td>
        <td>{item.manual_unbanned?<StatusPill kind="default">已解封</StatusPill>:item.local_ban_id?<StatusPill kind="danger">已封禁</StatusPill>:<StatusPill kind="success">未封禁</StatusPill>}</td>
      </tr>)})}
    </tbody></table></div>
  </Modal>);
}

function OverviewTab({detail, globalBans}) {
  const summary = detail.summary || {};
  const riskProfile = detail.risk_profile;
  const timeline = latestItems(detail.timeline || [], 10);
  const currentOnline = detail.online_records || [];
  const internalTags = detail.internal_profile?.tags || [];
  const linkedLocalBans = count(summary.linked_banned_account_count);
  const linkedGlobalBans = count(summary.linked_global_banned_account_count);
  const linkedRiskBans = linkedLocalBans + linkedGlobalBans;
  const activeGlobalCount = countActiveGlobalBans(globalBans || []);

  return <>
    <div className="player-dossier-overview">
      <div className="card">
        <div className="card-header"><div><div className="card-title">关键结论</div><div className="card-sub">优先展示会影响处理判断的信息。</div></div></div>
        <div className="card-body">
          <div className="player-insight-list">
            <div className="player-insight-item">
              <span className={`player-insight-dot ${count(summary.active_ban_count)+activeGlobalCount>0?'danger':'success'}`}></span>
              <div><strong>封禁状态</strong><p>{count(summary.active_ban_count)>0?`存在 ${summary.active_ban_count} 条本地有效封禁。`:'未发现本地有效封禁。'}{activeGlobalCount>0?`另有 ${activeGlobalCount} 条有效全球封禁。`:''}</p></div>
            </div>
            <div className="player-insight-item">
              <span className={`player-insight-dot ${currentOnline.length>0?'success':'default'}`}></span>
              <div><strong>服务器状态</strong><p>{currentOnline.length>0?`正在 ${currentOnline.map(i=>`${i.server_name}:${i.server_port}`).join('、')} 中。`:'当前在线快照中未发现该玩家。'}</p></div>
            </div>
            <div className="player-insight-item">
              <span className={`player-insight-dot ${linkedRiskBans>0?'danger':'default'}`}></span>
              <div><strong>IP 关联</strong><p>{summary.linked_account_count>0?`共发现 ${summary.linked_account_count} 个同 IP 账号，其中本地封禁 ${linkedLocalBans} 个、全球封禁 ${linkedGlobalBans} 个。`:'暂无同 IP 关联账号。'}</p></div>
            </div>
            <div className="player-insight-item">
              <span className={`player-insight-dot ${riskProfile?.action==='deny'||riskProfile?.action==='require_force'?'danger':riskProfile?.action==='warn'?'warning':'success'}`}></span>
              <div><strong>白名单建议</strong><p>{riskProfile?.recommendation||'可以按正常流程审核。'}{riskProfile?.summary?` ${riskProfile.summary}`:''}</p></div>
            </div>
            <div className="player-insight-item">
              <span className={`player-insight-dot ${internalTags.length>0?'warning':'default'}`}></span>
              <div><strong>内部标记</strong><p>{internalTags.length>0?internalTags.map(t=>`#${t}`).join(' '):(detail.internal_profile?.note||'暂无内部备注或标签。')}</p></div>
            </div>
          </div>
        </div>
      </div>

      <div className="card">
        <div className="card-header"><div><div className="card-title">最近事件</div><div className="card-sub">合并白名单、封禁、进服、工单和审计记录。</div></div></div>
        <div className="card-body">
          {timeline.length===0?<Empty>暂无事件时间线。</Empty>:<div className="player-mini-timeline">
            {timeline.map((event,i)=><div className="player-mini-event" key={`${event.event_type}-${event.related_id||i}-${event.occurred_at}`}>
              <div className="player-mini-event-time">{formatChinaDateTime(event.occurred_at,{seconds:false})}</div>
              <div className="player-mini-event-main"><strong>{event.title}</strong><span>{event.description||categoryLabel(event.category)}</span></div>
              <StatusPill kind={categoryKind(event.category,event.status)}>{categoryLabel(event.category)}</StatusPill>
            </div>)}
          </div>}
        </div>
      </div>
    </div>
  </>;
}

function TimelineTab({detail}) {
  const [filter, setFilter] = useState('all');
  const timeline = detail.timeline || [];
  const categories = Array.from(new Set(timeline.map(item=>item.category))).filter(Boolean);
  const visible = filter==='all'?timeline:timeline.filter(item=>item.category===filter);

  return <div className="card">
    <div className="card-header">
      <div><div className="card-title">玩家完整事件时间线</div><div className="card-sub">按发生时间倒序合并该账号牵涉到的所有记录。</div></div>
    </div>
    <div className="card-body">
      <div className="player-timeline-filters">
        <button type="button" className={`profile-tab ${filter==='all'?'active':''}`} onClick={()=>setFilter('all')}>全部</button>
        {categories.map(category=><button type="button" key={category} className={`profile-tab ${filter===category?'active':''}`} onClick={()=>setFilter(category)}>{categoryLabel(category)}</button>)}
      </div>
      {visible.length===0?<Empty>暂无匹配事件。</Empty>:<div className="player-event-timeline">
        {visible.map((event,i)=><div className={`player-event-item ${event.status==='failed'?'danger':''}`} key={`${event.event_type}-${event.related_id||i}-${event.occurred_at}`}>
          <div className="player-event-marker"><span></span></div>
          <div className="player-event-card">
            <div className="player-event-top">
              <div>
                <div className="player-event-title">{event.title}</div>
                <div className="player-event-time">{formatChinaDateTime(event.occurred_at)}</div>
              </div>
              <div className="player-event-pills">
                <StatusPill kind={categoryKind(event.category,event.status)}>{categoryLabel(event.category)}</StatusPill>
                {event.status&&<StatusPill kind={stKind(event.status)}>{stLabel(event.status)}</StatusPill>}
              </div>
            </div>
            {event.description&&<div className="player-event-desc">{event.description}</div>}
            <div className="player-event-meta">
              <span>操作人: {event.actor||'-'}</span>
              <span>来源: {eventSourceLabel(event.source)}</span>
              {event.related_id&&<span className="mono">ID: {event.related_id}</span>}
            </div>
          </div>
        </div>)}
      </div>}
    </div>
  </div>;
}

function SessionTable({sessions=[], emptyText='暂无服务器会话记录。'}) {
  return sessions.length===0?<Empty>{emptyText}</Empty>:<div className="table-responsive"><table className="data-table player-record-table"><thead><tr><th>服务器</th><th>进入时间</th><th>退出时间</th><th>退出原因</th><th>时长</th><th>IP</th><th>Ping</th><th>地图</th><th>玩家名</th></tr></thead><tbody>
    {sessions.map(item=><tr key={item.id}>
      <td className="fw-600">{item.server_name}:{item.server_port}<br/><span style={{color:'var(--text3)',fontSize:'11.5px'}}>{item.community_name||'-'}</span></td>
      <td style={{fontFamily:'var(--mono)',fontSize:'12px',whiteSpace:'nowrap'}}>{formatChinaDateTime(item.first_seen_at,{seconds:false})}</td>
      <td style={{fontFamily:'var(--mono)',fontSize:'12px',whiteSpace:'nowrap'}}><StatusPill kind={item.left_at?'offline':'success'}>{item.left_at?'已退出':'在线'}</StatusPill><div className="player-table-sub">{sessionEndLabel(item)}</div></td>
      <td><StatusPill kind={sessionReasonKind(item.end_reason)}>{item.left_at?sessionReasonLabel(item.end_reason):'仍在线'}</StatusPill>{item.end_detail?<div className="player-table-sub text-break">{item.end_detail}</div>:null}</td>
      <td>{sessionDurationLabel(item.duration_seconds)}</td>
      <td className="steam-id">{item.ip||'-'}</td>
      <td>{item.last_ping??'-'}</td>
      <td>{item.last_map||'-'}</td>
      <td>{item.player_name||'-'}</td>
    </tr>)}
  </tbody></table></div>
}

function AccessTab({detail}) {
  const accessLogs = detail.access_logs || [];
  const currentOnline = detail.online_records || [];
  const sessions = detail.player_sessions || [];
  return <>
    <div className="card"><div className="card-header"><div><div className="card-title">进服成功 / 失败明细</div><div className="card-sub">展示进入服务器的判定方式、失败原因、IP、Rating 和 Steam 等级。</div></div></div><div className="card-body p-0">
      {accessLogs.length===0?<Empty>暂无进服尝试日志。</Empty>:<div className="table-responsive"><table className="data-table player-record-table"><thead><tr><th>时间</th><th>结果</th><th>服务器</th><th>IP</th><th>方式 / 原因</th><th>Rating</th><th>Steam等级</th><th>玩家名</th></tr></thead><tbody>
        {accessLogs.map(item=><tr key={item.id} className={!item.allowed?'row-access-denied':undefined}>
          <td style={{fontFamily:'var(--mono)',fontSize:'12px',whiteSpace:'nowrap'}}>{formatChinaDateTime(item.created_at,{seconds:false})}</td>
          <td><StatusPill kind={item.allowed?'success':'danger'}>{item.allowed?'成功':'失败'}</StatusPill></td>
          <td className="fw-600">{item.server_name}:{item.server_port}<br/><span style={{color:'var(--text3)',fontSize:'11.5px'}}>{item.community_name||'-'}</span></td>
          <td className="steam-id">{item.ip_address||'-'}</td>
          <td><StatusPill kind={item.allowed?'success':'danger'}>{methodLabel(item.access_method)}</StatusPill><div className="player-table-sub">{item.allowed?'允许原因':failureLabel(item)}</div></td>
          <td>{item.rating??'-'}</td>
          <td>{item.steam_level??'-'}</td>
          <td>{item.player_name||'-'}</td>
        </tr>)}
      </tbody></table></div>}
    </div></div>

    <div className="card"><div className="card-header"><div><div className="card-title">服务器会话历史</div><div className="card-sub">由在线快照生成，包含进入服务器和退出服务器时间。</div></div></div><div className="card-body p-0">
      <SessionTable sessions={sessions}/>
    </div></div>

    <div className="card"><div className="card-header"><div><div className="card-title">当前在线服务器</div><div className="card-sub">来自服务器实时上报快照。</div></div></div><div className="card-body p-0">
      {currentOnline.length===0?<Empty>当前没有在已上报服务器中。</Empty>:<div className="table-responsive"><table className="data-table player-record-table"><thead><tr><th>服务器</th><th>社区</th><th>上报时间</th><th>IP</th><th>Ping</th><th>地图</th></tr></thead><tbody>
        {currentOnline.map((item,i)=><tr key={`${item.server_id}-${item.reported_at}-${i}`}>
          <td className="fw-600">{item.server_name}:{item.server_port}</td>
          <td>{item.community_name||'-'}</td>
          <td style={{fontFamily:'var(--mono)',fontSize:'12px',whiteSpace:'nowrap'}}>{formatChinaDateTime(item.reported_at,{seconds:false})}</td>
          <td className="steam-id">{item.ip}</td>
          <td>{item.ping}</td>
          <td>{item.current_map||'-'}</td>
        </tr>)}
      </tbody></table></div>}
    </div></div>
  </>;
}

// ── Tabs ──
function StatusTab({detail, token, onRefresh}) {
  const [popup, setPopup] = useState(null);
  return <>
    <div className="card"><div className="card-header"><div><div className="card-title">历史封禁履历表</div><div className="card-sub">点击行查看详情并操作。</div></div></div><div className="card-body p-0">
      {detail.bans.length===0?<Empty>暂无封禁记录。</Empty>:<div className="table-responsive"><table className="data-table player-record-table"><thead><tr><th>执行时间</th><th>封禁维度</th><th>时长</th><th>封禁理由</th><th>操作人</th><th>状态</th></tr></thead><tbody>
        {detail.bans.map(item=><tr key={item.id} style={{cursor:'pointer'}} onClick={()=>setPopup({type:'ban',item})}>
          <td style={{fontFamily:'var(--mono)',fontSize:'12px',whiteSpace:'nowrap'}}>{formatChinaDateTime(item.created_at,{seconds:false})}</td>
          <td>{item.ban_type==='ip'?'账号+IP':'账号'}</td>
          <td style={{color:item.status==='active'?'var(--danger-text)':'var(--text2)'}}>{durLabel(item.duration_minutes)}</td>
          <td className="text-ellipsis" style={{maxWidth:240}} title={item.reason}>{item.reason}</td>
          <td>{item.operator_name||'-'}</td>
          <td><StatusPill kind={stKind(item.status,'ban')}>{stLabel(item.status)}</StatusPill></td>
        </tr>)}
      </tbody></table></div>}
    </div></div>
    <div className="card"><div className="card-header"><div><div className="card-title">社区白名单记录</div><div className="card-sub">点击行查看详情并审核。</div></div></div><div className="card-body p-0">
      {detail.whitelist.length===0?<Empty>暂无白名单记录。</Empty>:<div className="table-responsive"><table className="data-table player-record-table"><thead><tr><th>提交时间</th><th>昵称</th><th>审核人</th><th>审核意见</th><th>状态</th></tr></thead><tbody>
        {detail.whitelist.map(item=><tr key={item.id} style={{cursor:'pointer'}} onClick={()=>setPopup({type:'whitelist',item})}>
          <td style={{fontFamily:'var(--mono)',fontSize:'12px',whiteSpace:'nowrap'}}>{formatChinaDateTime(item.applied_at,{seconds:false})}</td>
          <td className="fw-600">{item.nickname}</td>
          <td>{item.approved_by||item.rejected_by||item.revoked_by||'-'}</td>
          <td className="text-ellipsis" style={{maxWidth:160}}>{item.approval_reason||item.rejection_reason||'-'}</td>
          <td><StatusPill kind={stKind(item.status,'whitelist')}>{stLabel(item.status)}</StatusPill></td>
        </tr>)}
      </tbody></table></div>}
    </div></div>
    {popup?.type==='ban'&&<BanDetailPopup item={popup.item} onClose={()=>setPopup(null)} onAction={onRefresh} token={token}/>}
    {popup?.type==='whitelist'&&<WhitelistDetailPopup item={popup.item} onClose={()=>setPopup(null)} onAction={onRefresh} token={token}/>}
  </>;
}

function OverviewStrip({detail, globalBans}) {
  const summary = detail.summary || {};
  const risk = detail.risk_profile;
  const linkedRiskBans = count(summary.linked_banned_account_count) + count(summary.linked_global_banned_account_count);
  const activeGlobal = countActiveGlobalBans(globalBans || []);
  const items = [
    { label:'进服成功', value:count(summary.access_success_count), tone:'success' },
    { label:'进服失败', value:count(summary.access_failure_count), tone:count(summary.access_failure_count)>0?'danger':'default' },
    { label:'本地封禁', value:count(summary.active_ban_count), tone:count(summary.active_ban_count)>0?'danger':'success' },
    { label:'全球封禁', value:activeGlobal, tone:activeGlobal>0?'danger':'default' },
    { label:'同 IP 账号', value:count(summary.linked_account_count), tone:linkedRiskBans>0?'warning':'default' },
    { label:'风险等级', value:risk?.action==='deny'||risk?.action==='require_force'?'高':risk?.action==='warn'?'中':'低', tone:risk?.action==='deny'||risk?.action==='require_force'?'danger':risk?.action==='warn'?'warning':'success' },
    { label:'证据文件', value:count(summary.evidence_file_count), tone:'default' },
  ];
  return <div className="player-overview-strip">
    {items.map(item=><div className={`player-overview-chip ${item.tone}`} key={item.label}>
      <span>{item.label}</span><strong>{item.value}</strong>
    </div>)}
  </div>;
}

function PlayerSummaryRail({detail, globalBans, canEdit, onSaveInternal, internalSaving, onShowGlobalBans}) {
  const profile = detail.profile || {};
  const summary = detail.summary || {};
  const currentOnline = detail.online_records || [];
  const risk = detail.risk_profile;
  const activeGlobalCount = countActiveGlobalBans(globalBans || []);
  const activeBanCount = count(summary.active_ban_count);
  const tags = detail.internal_profile?.tags || [];
  const note = detail.internal_profile?.note || '';
  const onlineText = currentOnline.length > 0 ? currentOnline.map(item=>item.server_name).filter(Boolean).join(' / ') : '未在线';
  const contact = latestWhitelistContact(detail);

  return <aside className="player-summary-card">
    <div className="player-summary-head">
      <div className={`player-summary-avatar ${activeBanCount+activeGlobalCount>0?'danger':''}`}>{(profile.display_name||profile.steamid64||'?').slice(0,2).toUpperCase()}</div>
      <div className="player-summary-title">
        <div className="player-summary-name">{profile.display_name||profile.steamid64}</div>
        <div className="player-summary-sub">{summary.last_seen_at?`最后活跃 ${formatChinaDateTime(summary.last_seen_at,{seconds:false})}`:'暂无活跃记录'}</div>
      </div>
    </div>

    <div className="player-summary-pills">
      <StatusPill kind={currentOnline.length>0?'success':'default'}>{onlineText}</StatusPill>
      <StatusPill kind={activeBanCount+activeGlobalCount>0?'danger':'success'}>{activeBanCount+activeGlobalCount>0?`有效封禁 ${activeBanCount+activeGlobalCount}`:'无有效封禁'}</StatusPill>
      <StatusPill kind={summary.whitelist_status==='approved'?'success':summary.whitelist_status==='pending'?'warning':summary.whitelist_status?'danger':'default'}>白名单 {stLabel(summary.whitelist_status)}</StatusPill>
    </div>

    {(activeGlobalCount>0 || ((globalBans||[]).length>0 && activeGlobalCount===0)) ? (
      <button type="button" className={`player-summary-alert ${activeGlobalCount>0?'danger':''}`} onClick={onShowGlobalBans}>
        <strong>{activeGlobalCount>0?'存在活跃全球封禁':'存在历史全球封禁'}</strong>
        <span>{globalBans.length} 条记录，点击查看详情</span>
      </button>
    ) : null}

    <div className={`player-summary-risk ${riskTone(risk?.action)}`}>
      <span>处理建议</span>
      <strong>{riskLabel(risk?.action)}</strong>
      <p>{risk?.recommendation || '可以按正常流程审核。'}{risk?.summary ? ` ${risk.summary}` : ''}</p>
    </div>

    <div className="player-summary-section">
      <div className="player-summary-section-title">身份信息</div>
      <div className="player-summary-id-row"><span>SteamID64</span><code>{profile.steamid64}</code></div>
      {profile.steamid&&<div className="player-summary-id-row"><span>SteamID2</span><code>{profile.steamid}</code></div>}
      {profile.steamid3&&<div className="player-summary-id-row"><span>SteamID3</span><code>{profile.steamid3}</code></div>}
      <div className="player-summary-id-row"><span>联系方式</span><code>{contact || ''}</code></div>
    </div>

    <div className="player-summary-section">
      <div className="player-summary-section-title">内部备注</div>
      {note||tags.length>0 ? (
        <div className="player-summary-note">
          {note?<p>{note}</p>:null}
          {tags.length>0?<div className="player-summary-tags">{tags.map(tag=><span key={tag}>#{tag}</span>)}</div>:null}
        </div>
      ) : <div className="player-summary-empty">暂无备注或标签。</div>}
      {canEdit&&<InternalBtn key={detail.profile.steamid64} detail={detail} onSave={onSaveInternal} saving={internalSaving}/>}
    </div>
  </aside>;
}

function NetworkTab({detail}) {
  const ipHistory = detail.ip_history || [];
  const sessions = detail.player_sessions || [];
  return <>
    <div className="card"><div className="card-header"><div><div className="card-title">深度 IP 交叉与设备追踪表</div><div className="card-sub"><strong style={{color:'var(--accent)'}}>逆向检索同 IP 的关联 Steam 账号</strong>，包含关联账号白名单和封禁状态。</div></div></div><div className="card-body p-0">
      {ipHistory.length===0?<Empty>暂无 IP 登录记录。</Empty>:<div className="table-responsive"><table className="data-table tree-table"><thead><tr><th>IP</th><th>首次/最后活跃</th><th>服务器</th><th>关联账号 / 白名单</th></tr></thead><tbody>
        {ipHistory.map(entry=>{const lb=entry.linked_accounts?.filter(a=>a.has_local_ban).length||0;const gb=entry.linked_accounts?.filter(a=>a.has_global_ban).length||0;const banned=lb+gb;return <React.Fragment key={entry.ip}>
          <tr><td><code className="steam-id" style={{fontWeight:700,fontSize:'13px'}}>{entry.ip}</code></td>
          <td style={{fontFamily:'var(--mono)',color:'var(--text2)',fontSize:'12px'}}>首:{entry.first_seen?formatChinaDateTime(entry.first_seen,{seconds:false}):'-'}<br/>末:{entry.last_seen?formatChinaDateTime(entry.last_seen,{seconds:false}):'-'}</td>
          <td style={{color:'var(--text2)'}}>{entry.servers?.map(s=>`${s.server_name}${s.server_port?`:${s.server_port}`:''}`).join(', ')||'-'}</td>
          <td>{entry.linked_accounts?.length>0?<span style={{color:banned>0?'var(--danger-text)':'var(--text2)',fontWeight:600}}>⚠ {entry.linked_accounts.length}个关联{banned>0?`（本地 ${lb} · 全球 ${gb}）`:''}</span>:<span style={{color:'var(--text3)'}}>无</span>}</td></tr>
          {entry.linked_accounts?.map(acc=><tr key={acc.steam_id64}><td className="nested">└─关联</td><td style={{fontSize:'11px',color:'var(--text3)'}}>访问 {acc.access_count||0}次</td><td>{acc.servers?.join(', ')||'-'}</td>
          <td>
            <div className="linked-account-cell">
              <span style={{fontWeight:600}}>{acc.player_name||'(未知)'}</span> <code className="steam-id" style={{fontSize:'11px'}}>{acc.steam_id64}</code>
              <div className="linked-account-pills">
                {acc.has_local_ban&&<StatusPill kind="danger">本地封禁</StatusPill>}
                {acc.has_global_ban&&<StatusPill kind="danger">全球封禁</StatusPill>}
                {acc.whitelist_count>0?<StatusPill kind={stKind(acc.whitelist_status,'whitelist')}>白名单 {stLabel(acc.whitelist_status)} · {acc.whitelist_count} 次</StatusPill>:<StatusPill kind="default">无白名单</StatusPill>}
              </div>
              {acc.whitelist_applied_at&&<div className="player-table-sub">最近申请: {formatChinaDateTime(acc.whitelist_applied_at,{seconds:false})}{acc.whitelist_reviewer?` · 审核人: ${acc.whitelist_reviewer}`:''}</div>}
            </div>
          </td></tr>)}
        </React.Fragment>})}
      </tbody></table></div>}
    </div></div>
    <div className="card"><div className="card-header"><div><div className="card-title">服务器会话与在线记录</div></div></div><div className="card-body p-0">
      <SessionTable sessions={sessions} emptyText="暂无服务器会话记录。"/>
    </div></div>
  </>;
}

function BehaviorTab({detail, token, onRefresh}) {
  const [popup, setPopup] = useState(null);
  const mf = detail.map_feedback || [];
  return <>
    <div className="lower-grid">
      <div className="card"><div className="card-header"><div><div className="card-title">玩家自主申诉记录</div><div className="card-sub">点击行查看详情并审核。</div></div></div><div className="card-body p-0">
        {detail.appeals.length===0?<Empty>暂无申诉记录。</Empty>:<div className="table-responsive"><table className="data-table player-record-table"><thead><tr><th>提交时间</th><th>申诉理由</th><th>关联封禁</th><th>审核人</th><th>状态</th></tr></thead><tbody>
          {detail.appeals.map(item=><tr key={item.id} style={{cursor:'pointer'}} onClick={()=>setPopup({type:'appeal',item})}>
            <td style={{fontFamily:'var(--mono)',fontSize:'12px',whiteSpace:'nowrap'}}>{formatChinaDateTime(item.created_at,{seconds:false})}</td>
            <td className="text-ellipsis" style={{maxWidth:180}} title={item.appeal_reason}>{item.appeal_reason}</td>
            <td className="text-ellipsis" style={{maxWidth:140}} title={item.ban_reason||''}>{item.ban_reason||'-'}</td>
            <td>{item.reviewed_by||'-'}</td>
            <td><StatusPill kind={stKind(item.status,'appeal')}>{stLabel(item.status)}</StatusPill></td>
          </tr>)}
        </tbody></table></div>}
      </div></div>
      <div className="card"><div className="card-header"><div><div className="card-title">玩家被举报记录</div><div className="card-sub">点击行查看详情并审核。</div></div></div><div className="card-body p-0">
        {detail.reports.length===0?<Empty>暂无举报记录。</Empty>:<table className="data-table"><thead><tr><th>举报时间</th><th>理由</th><th>举报联系方式</th><th>审核人</th><th>状态</th></tr></thead><tbody>
          {detail.reports.map(item=><tr key={item.id} style={{cursor:'pointer'}} onClick={()=>setPopup({type:'report',item})}>
            <td style={{fontFamily:'var(--mono)',fontSize:'12px',whiteSpace:'nowrap'}}>{formatChinaDateTime(item.created_at,{seconds:false})}</td>
            <td className="text-ellipsis" style={{maxWidth:180}}>{item.report_reason}</td>
            <td className="text-ellipsis" style={{maxWidth:140}} title={item.reporter_contact||''}>{item.reporter_contact||'-'}</td>
            <td>{item.reviewed_by||'-'}</td>
            <td><StatusPill kind={stKind(item.status,'report')}>{stLabel(item.status)}</StatusPill></td>
          </tr>)}
        </tbody></table>}
      </div></div>
    </div>
    <div className="card"><div className="card-header"><div><div className="card-title">地图反馈记录</div><div className="card-sub">点击行查看详情并快捷回复。</div></div></div><div className="card-body p-0">
      {mf.length===0?<Empty>暂无地图反馈记录。</Empty>:<div className="table-responsive"><table className="data-table player-record-table"><thead><tr><th>提交时间</th><th>反馈类型</th><th>详细内容</th><th>审核人</th><th>状态</th></tr></thead><tbody>
        {mf.map(item=><tr key={item.id} style={{cursor:'pointer'}} onClick={()=>setPopup({type:'feedback',item})}>
          <td style={{fontFamily:'var(--mono)',fontSize:'12px',whiteSpace:'nowrap'}}>{formatChinaDateTime(item.created_at,{seconds:false})}</td>
          <td><StatusPill kind={item.feedback_type==='broken'?'danger':item.feedback_type==='missing'?'warning':'default'}>{feedbackTypeLabel(item.feedback_type)}</StatusPill></td>
          <td className="text-ellipsis" style={{maxWidth:240}} title={item.detail}>{item.detail}</td>
          <td>{item.reviewed_by||'-'}</td>
          <td><StatusPill kind={stKind(item.status)}>{stLabel(item.status)}</StatusPill></td>
        </tr>)}
      </tbody></table></div>}
    </div></div>
    <div className="card"><div className="card-header"><div><div className="card-title">物理证据文件与媒体库</div></div></div><div className="card-body">
      {detail.evidence_files.length===0?<Empty>暂无附件证据。</Empty>:<div className="table-responsive"><table className="data-table"><thead><tr><th>文件名称</th><th>归属</th><th>尺寸</th><th>上传时间</th></tr></thead><tbody>
        {detail.evidence_files.map(file=><tr key={`${file.source_type}-${file.id}`}>
          <td style={{fontFamily:'var(--mono)',fontWeight:600,fontSize:'12px'}}>{file.file_name}</td><td>{file.source_label}</td>
          <td style={{fontFamily:'var(--mono)'}}>{(file.file_size/1024/1024).toFixed(1)} MB</td>
          <td style={{fontFamily:'var(--mono)',fontSize:'12px'}}>{formatChinaDateTime(file.uploaded_at,{seconds:false})}</td>
        </tr>)}
      </tbody></table></div>}
    </div></div>
    {popup?.type==='appeal'&&<AppealDetailPopup item={popup.item} onClose={()=>setPopup(null)} onAction={onRefresh} token={token}/>}
    {popup?.type==='report'&&<ReportDetailPopup item={popup.item} onClose={()=>setPopup(null)} onAction={onRefresh} token={token}/>}
    {popup?.type==='feedback'&&<MapFeedbackDetailPopup item={popup.item} onClose={()=>setPopup(null)} onAction={onRefresh} token={token}/>}
  </>;
}

function AuditTab({detail}) {
  const aa = detail.admin_actions || [];
  const al = detail.audit_logs || [];
  return <div className="lower-grid">
    <div className="card"><div className="card-header"><div><div className="card-title">玩家 Web 端操作日志</div></div></div><div className="card-body">
      {aa.length===0?<Empty>暂无操作日志。</Empty>:<div className="log-timeline">{aa.slice(0,20).map((item,i)=><div className="log-item" key={`admin-${i}`}><div className="log-meta"><span>{formatChinaDateTime(item.created_at,{seconds:false})}</span><span>IP: {item.ip_address||'-'}</span></div><div className="log-content">{item.module}/{item.action}: {item.target_detail}</div></div>)}</div>}
    </div></div>
    <div className="card"><div className="card-header"><div><div className="card-title">管理员审计记录</div></div></div><div className="card-body">
      {al.length===0?<Empty>暂无审计记录。</Empty>:<div className="log-timeline">{al.slice(0,20).map((item,i)=><div className={`log-item ${!item.success?'critical':''}`} key={`audit-${item.id||i}`}><div className="log-meta"><span>{formatChinaDateTime(item.created_at,{seconds:false})}</span><span style={{color:'var(--text)'}}>操作人: {item.operator_name||'-'}</span></div><div className="log-content" style={!item.success?{borderLeft:'3px solid var(--danger-dot)'}:undefined}><strong>{item.operation}</strong>{item.message?`: ${item.message}`:''}</div></div>)}</div>}
    </div></div>
  </div>;
}

const TABS = [
  {key:'overview',label:'总览'},
  {key:'timeline',label:'时间线'},
  {key:'access',label:'进服'},
  {key:'status',label:'封禁/白名单'},
  {key:'network',label:'IP 关联'},
  {key:'behavior',label:'工单'},
  {key:'audit',label:'审计'},
];

export function PlayerDetailPage() {
  const { session } = useAuth();
  const { toast } = useToast();
  const queryClient = useQueryClient();
  const token = session?.token ?? null;
  const canEdit = session?.role === 'developer' || session?.role === 'admin';
  const [steamInput, setSteamInput] = useState('');
  const [lastQuery, setLastQuery] = useState('');
  const lastQueryRef = useRef('');
  const [detail, setDetail] = useState(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState('');
  const [tab, setTab] = useState('overview');
  const [internalSaving, setInternalSaving] = useState(false);
  const [globalBans, setGlobalBans] = useState(null);
  const [showGlobalBans, setShowGlobalBans] = useState(false);
  const [candidates, setCandidates] = useState([]);
  const [candidatesOpen, setCandidatesOpen] = useState(false);
  const [candidatesLoading, setCandidatesLoading] = useState(false);
  const [candidateError, setCandidateError] = useState('');
  const [activeCandidateIndex, setActiveCandidateIndex] = useState(-1);
  const searchWrapRef = useRef(null);
  const suppressCandidateSearchRef = useRef(false);

  // 加载玩家详情
  async function loadDetail(input, resetTab = true) {
    const q = input.trim(); if(!q){setError('请输入 SteamID。');return;}
    setCandidates([]);setCandidateError('');setCandidatesLoading(false);setCandidatesOpen(false);setActiveCandidateIndex(-1);
    try{setLoading(true);setError('');const r=await api.playerDetail(token,q);setDetail(r.data);const nextInput=r.data?.profile?.display_name||r.data?.profile?.steamid64||q;if(nextInput!==steamInput){suppressCandidateSearchRef.current=true;setSteamInput(nextInput);}else{suppressCandidateSearchRef.current=false;}setLastQuery(q);lastQueryRef.current=q;if(resetTab)setTab('overview');}catch(e){setDetail(null);setError(e.message||'加载失败');}finally{setLoading(false);}
  }
  function selectCandidate(candidate) { if(!candidate)return;loadDetail(candidate.steamid64); }
  function submitSearch() {
    if(candidatesLoading){setCandidatesOpen(true);return;}
    const picked=activeCandidateIndex>=0?candidates[activeCandidateIndex]:candidates.length===1?candidates[0]:null;
    if(picked){selectCandidate(picked);return;}
    if(candidates.length>1){setCandidatesOpen(true);setError('找到多个匹配玩家，请在下拉框中选择。');return;}
    loadDetail(steamInput);
  }
  function handleSearchKeyDown(e) {
    if(!candidatesOpen||candidates.length===0){if(e.key==='Escape')setCandidatesOpen(false);return;}
    if(e.key==='ArrowDown'){e.preventDefault();setActiveCandidateIndex(i=>(i+1)%candidates.length);}
    else if(e.key==='ArrowUp'){e.preventDefault();setActiveCandidateIndex(i=>(i<=0?candidates.length-1:i-1));}
    else if(e.key==='Enter'&&activeCandidateIndex>=0){e.preventDefault();selectCandidate(candidates[activeCandidateIndex]);}
    else if(e.key==='Escape'){e.preventDefault();setCandidatesOpen(false);setActiveCandidateIndex(-1);}
  }
  function refreshDetail() { const q = lastQueryRef.current || lastQuery; if(q) loadDetail(q, false); queryClient.invalidateQueries({queryKey:['whitelist']}); queryClient.invalidateQueries({queryKey:['bans']}); queryClient.invalidateQueries({queryKey:['banAppeals']}); queryClient.invalidateQueries({queryKey:['playerReports']}); queryClient.invalidateQueries({queryKey:['mapFeedback']}); }
  // 内部备注
  async function handleSaveInternal(body) {
    if(!detail)return;try{setInternalSaving(true);const r=await api.updatePlayerInternalProfile(token,detail.profile.steamid64,body);setDetail(p=>p?{...p,internal_profile:r.item}:p);toast({title:'保存成功'});}catch(e){toast({title:'保存失败',message:e.message,tone:'danger'});}finally{setInternalSaving(false);}
  }

  // 查询全球封禁
  useEffect(()=>{
    if(!detail?.profile?.steamid64){React.startTransition(()=>setGlobalBans(null));return;}
    let c=false;(async()=>{try{const r=await api.searchGlobalBans(token,{steam_input:detail.profile.steamid64});if(!c)setGlobalBans(r.items||[]);}catch{if(!c)setGlobalBans(null);}})();
    return ()=>{c=true;};
  },[detail?.profile?.steamid64,token]);

  useEffect(()=>{
    function closeOnOutsideClick(event) {
      if(searchWrapRef.current&&!searchWrapRef.current.contains(event.target)) {
        setCandidatesOpen(false);
        setActiveCandidateIndex(-1);
      }
    }
    document.addEventListener('mousedown', closeOnOutsideClick);
    return ()=>document.removeEventListener('mousedown', closeOnOutsideClick);
  },[]);

  useEffect(()=>{
    const q=steamInput.trim();
    if(!token||q.length<2){suppressCandidateSearchRef.current=false;React.startTransition(()=>{setCandidates([]);setCandidateError('');setCandidatesLoading(false);setActiveCandidateIndex(-1);});return;}
    if(suppressCandidateSearchRef.current){suppressCandidateSearchRef.current=false;React.startTransition(()=>{setCandidates([]);setCandidateError('');setCandidatesOpen(false);setCandidatesLoading(false);setActiveCandidateIndex(-1);});return;}
    let cancelled=false;
    React.startTransition(()=>setCandidatesLoading(true));
    const timer=window.setTimeout(async()=>{
      try{
        const r=await api.playerDetailCandidates(token,q);
        if(cancelled)return;
        const items=r.items||[];
        setCandidates(items);
        setCandidateError('');
        setCandidatesOpen(Boolean(document.activeElement&&searchWrapRef.current?.contains(document.activeElement)));
        setActiveCandidateIndex(items.length===1?0:-1);
      }catch(e){
        if(cancelled)return;
        setCandidates([]);
        setCandidateError(e.message||'候选查询失败');
        setCandidatesOpen(true);
        setActiveCandidateIndex(-1);
      }finally{
        if(!cancelled)setCandidatesLoading(false);
      }
    },260);
    return ()=>{cancelled=true;window.clearTimeout(timer);};
  },[steamInput,token]);

  return (<div id="player-detail" className="content-section active">
    <div className="breadcrumb"><span>核心管理</span><span className="sep">›</span><span className="current">玩家全息档案</span></div>
    <div className="page-header"><div><div className="page-title">玩家详情</div><div className="page-sub">集中查看身份、风险、封禁、进服和工单记录。</div></div></div>

    <div className="card player-search-card">
      <form className="player-detail-search" onSubmit={e=>{e.preventDefault();submitSearch();}}>
        <div className="player-search-combobox" ref={searchWrapRef}>
          <div className="search-bar-box">
            <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" width="15" height="15" style={{flexShrink:0,color:'var(--text3)'}}><circle cx="7" cy="7" r="4.5"/><path d="M10.5 10.5L14 14"/></svg>
            <input
              type="text"
              placeholder="输入玩家名称 / SteamID64 / SteamID2 / SteamID3 / IP / 联系方式..."
              value={steamInput}
              autoComplete="off"
              role="combobox"
              aria-expanded={candidatesOpen}
              aria-controls="player-search-candidates"
              aria-autocomplete="list"
              aria-activedescendant={activeCandidateIndex>=0?`player-candidate-${activeCandidateIndex}`:undefined}
              onFocus={()=>{if(steamInput.trim().length>=2&&(candidates.length>0||candidateError||candidatesLoading))setCandidatesOpen(true);}}
              onChange={e=>{setSteamInput(e.target.value);setError('');setCandidatesOpen(true);}}
              onKeyDown={handleSearchKeyDown}
            />
            {candidatesLoading&&<span className="player-search-spinner" aria-hidden="true"/>}
          </div>
          {candidatesOpen&&steamInput.trim().length>=2&&(
            <div className="player-candidate-menu" id="player-search-candidates" role="listbox">
              {candidateError?<div className="player-candidate-state">{candidateError}</div>:null}
              {!candidateError&&candidatesLoading&&candidates.length===0?<div className="player-candidate-state">正在匹配玩家...</div>:null}
              {!candidateError&&!candidatesLoading&&candidates.length===0?<div className="player-candidate-state">没有找到匹配玩家</div>:null}
              {!candidateError&&candidates.map((candidate,index)=>{
                const meta=candidateMeta(candidate);
                return <button
                  key={candidate.steamid64}
                  id={`player-candidate-${index}`}
                  className={`player-candidate-item ${index===activeCandidateIndex?'active':''}`}
                  type="button"
                  role="option"
                  aria-selected={index===activeCandidateIndex}
                  onMouseEnter={()=>setActiveCandidateIndex(index)}
                  onClick={()=>selectCandidate(candidate)}
                >
                  <span className="player-candidate-main">
                    <span className="player-candidate-name">{candidate.display_name||candidate.steamid64}</span>
                    <span className="player-candidate-id">{candidate.steamid64}</span>
                  </span>
                  <span className="player-candidate-secondary">
                    {candidate.steamid&&<span>{candidate.steamid}</span>}
                    {candidate.steamid3&&<span>{candidate.steamid3}</span>}
                  </span>
                  <span className="player-candidate-meta">
                    {candidate.sources.slice(0,4).map(source=><span className="player-candidate-source" key={source}>{source}</span>)}
                    {meta.map(text=><span key={text}>{text}</span>)}
                  </span>
                </button>;
              })}
            </div>
          )}
        </div>
        {lastQuery&&<button className="btn btn-outline" type="button" disabled={loading} onClick={refreshDetail}>刷新</button>}
      </form>
      {error&&<div className="player-detail-error">{error}</div>}
    </div>

    {!detail&&!loading?(<div className="player-welcome-card"><div className="player-welcome-inner"><div className="player-welcome-icon"><svg viewBox="0 0 24 24" fill="none" stroke="#fff" strokeWidth="2" width="28" height="28"><circle cx="9" cy="7" r="4"/><path d="M2 21v-2a4 4 0 0 1 4-4h6a4 4 0 0 1 4 4v2"/></svg></div><div className="player-welcome-title">查询玩家全息档案</div><div className="player-welcome-desc">输入 SteamID 查询玩家的完整信息档案。</div></div></div>):null}

    {detail?<>
      <div className="player-detail-layout">
        <PlayerSummaryRail
          detail={detail}
          globalBans={globalBans}
          canEdit={canEdit}
          onSaveInternal={handleSaveInternal}
          internalSaving={internalSaving}
          onShowGlobalBans={()=>setShowGlobalBans(true)}
        />
        <main className="player-detail-main">
          <OverviewStrip detail={detail} globalBans={globalBans}/>

          <div className="profile-tabs player-detail-tabs">{TABS.map(t=><button key={t.key} className={`profile-tab ${tab===t.key?'active':''}`} onClick={()=>setTab(t.key)} type="button">{t.label}</button>)}</div>
          <div className="profile-tab-pane active">
            {tab==='overview'&&<OverviewTab detail={detail} globalBans={globalBans}/>}
            {tab==='timeline'&&<TimelineTab detail={detail}/>}
            {tab==='access'&&<AccessTab detail={detail}/>}
            {tab==='status'&&<StatusTab detail={detail} token={token} onRefresh={refreshDetail}/>}
            {tab==='network'&&<NetworkTab detail={detail}/>}
            {tab==='behavior'&&<BehaviorTab detail={detail} token={token} onRefresh={refreshDetail}/>}
            {tab==='audit'&&<AuditTab detail={detail}/>}
          </div>
        </main>
      </div>
    </>:null}

    {showGlobalBans&&<GlobalBanPopup items={globalBans||[]} onClose={()=>setShowGlobalBans(false)}/>}
  </div>);
}

function InternalBtn({detail, onSave, saving}) {
  const [open, setOpen] = useState(false);
  const [note, setNote] = useState(detail.internal_profile?.note??'');
  const [tt, setTt] = useState(tagsToText(detail.internal_profile?.tags??[]));
  if(!open)return <button className="btn btn-outline player-internal-edit-btn" onClick={()=>setOpen(true)}>编辑备注</button>;
  return <div className="player-internal-editor">
    <input className="form-control" value={note} onChange={e=>setNote(e.target.value)} placeholder="内部备注"/>
    <div className="player-internal-editor-row">
      <input className="form-control" value={tt} onChange={e=>setTt(e.target.value)} placeholder="标签"/>
      <button className="btn btn-primary btn-sm" disabled={saving} onClick={()=>{onSave({note,tags:textToTags(tt)});setOpen(false);}}>{saving?'保存中...':'保存'}</button>
      <button className="btn btn-outline btn-sm" onClick={()=>setOpen(false)}>取消</button>
    </div>
  </div>;
}

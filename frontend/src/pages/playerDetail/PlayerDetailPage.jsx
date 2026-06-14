import React, { useEffect, useRef, useState } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { api } from '../../lib/api.js';
import { useAuth } from '../../state/auth.jsx';
import { useToast, ToastContainer } from '../../shared/Toast.jsx';
import { StatusPill } from '../../shared/StatusPill.jsx';
import { Modal } from '../../shared/Modal.jsx';
import { formatChinaDateTime } from '../../shared/time.js';

const STATUS_LABELS = { active:'生效中', inactive:'已失效', pending:'待审核', approved:'已通过', rejected:'已驳回', revoked:'已撤销', resolved:'已解决', online:'在线', success:'成功', failed:'失败' };
function stKind(s,c){if(s==='active'||c==='ban')return s==='inactive'?'offline':'danger';if(s==='online'||s==='approved'||s==='success'||s==='resolved')return'success';if(s==='pending')return'warning';if(s==='failed'||s==='rejected')return'danger';if(s==='revoked'||s==='inactive')return'offline';return'default'}
function stLabel(s){return STATUS_LABELS[s]||s||'-'}
function durLabel(m){const v=Number(m);if(!Number.isFinite(v))return'-';if(v===0)return'永久';if(v<60)return`${v} 分钟`;if(v%1440===0)return`${v/1440} 天`;if(v%60===0)return`${v/60} 小时`;return`${v} 分钟`}
function tagsToText(t=[]){return t.join(', ')}
function textToTags(v){return v.split(/[,，\n]/).map(t=>t.trim().replace(/^#/,'')).filter(Boolean).filter((t,i,a)=>a.findIndex(n=>n.toLowerCase()===t.toLowerCase())===i)}
function Empty({children}){return<div className="player-detail-empty">{children}</div>}
function feedbackTypeLabel(t){const m={missing:'地图缺失',broken:'地图损坏',request:'地图请求'};return m[t]||t||'-'}

// ── Ban Detail Popup ──
function BanDetailPopup({item, onClose, onAction, token, toast}) {
  const [acting, setActing] = useState(false);
  if (!item) return null;
  const isActive = item.status === 'active';

  async function doUnban() { if(acting)return;setActing(true);try{await api.unban(token,item.id);toast({title:'解封成功'});onAction();onClose();}catch(e){toast({title:'操作失败',message:e.message,tone:'danger'})}finally{setActing(false)} }
  async function doDelete() { if(acting||!confirm(`确定删除封禁记录吗？`))return;setActing(true);try{await api.deleteBan(token,item.id);toast({title:'删除成功'});onAction();onClose();}catch(e){toast({title:'操作失败',message:e.message,tone:'danger'})}finally{setActing(false)} }

  return (<Modal open={true} title="封禁详情" onClose={onClose} footer={<><button className="btn btn-outline" onClick={onClose}>关闭</button>{isActive&&<><button className="action-btn action-btn-success" onClick={doUnban} disabled={acting}>解封</button><button className="action-btn action-btn-danger" onClick={doDelete} disabled={acting}>删除</button></>}</>} >
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
  </Modal>);
}

// ── Whitelist Detail Popup ──
function WhitelistDetailPopup({item, onClose, onAction, token, toast}) {
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
function AppealDetailPopup({item, onClose, onAction, token, toast}) {
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
function ReportDetailPopup({item, onClose, onAction, token, toast}) {
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
function MapFeedbackDetailPopup({item, onClose, onAction, token, toast}) {
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

// ── Tabs ──
function StatusTab({detail, token, onRefresh, toast}) {
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
    {popup?.type==='ban'&&<BanDetailPopup item={popup.item} onClose={()=>setPopup(null)} onAction={onRefresh} token={token} toast={toast}/>}
    {popup?.type==='whitelist'&&<WhitelistDetailPopup item={popup.item} onClose={()=>setPopup(null)} onAction={onRefresh} token={token} toast={toast}/>}
  </>;
}

function NetworkTab({detail}) {
  const ipHistory = detail.ip_history || [];
  const onlineRecords = detail.online_records || [];
  return <>
    <div className="card"><div className="card-header"><div><div className="card-title">深度 IP 交叉与设备追踪表</div><div className="card-sub"><strong style={{color:'var(--accent)'}}>逆向检索同 IP 的关联 Steam 账号</strong>，打击小号。</div></div></div><div className="card-body p-0">
      {ipHistory.length===0?<Empty>暂无 IP 登录记录。</Empty>:<div className="table-responsive"><table className="data-table tree-table"><thead><tr><th>IP</th><th>首次/最后活跃</th><th>服务器</th><th>关联账号</th></tr></thead><tbody>
        {ipHistory.map(entry=>{const lb=entry.linked_accounts?.filter(a=>a.has_local_ban).length||0;return <React.Fragment key={entry.ip}>
          <tr><td><code className="steam-id" style={{fontWeight:700,fontSize:'13px'}}>{entry.ip}</code></td>
          <td style={{fontFamily:'var(--mono)',color:'var(--text2)',fontSize:'12px'}}>首:{entry.first_seen?formatChinaDateTime(entry.first_seen,{seconds:false}):'-'}<br/>末:{entry.last_seen?formatChinaDateTime(entry.last_seen,{seconds:false}):'-'}</td>
          <td style={{color:'var(--text2)'}}>{entry.servers?.map(s=>`${s.server_name}${s.server_port?`:${s.server_port}`:''}`).join(', ')||'-'}</td>
          <td>{entry.linked_accounts?.length>0?<span style={{color:lb>0?'var(--danger-text)':'var(--text2)',fontWeight:600}}>⚠ {entry.linked_accounts.length}个关联{lb>0?`（${lb}个封禁）`:''}</span>:<span style={{color:'var(--text3)'}}>无</span>}</td></tr>
          {entry.linked_accounts?.map(acc=><tr key={acc.steam_id64}><td className="nested">└─关联</td><td style={{fontSize:'11px',color:'var(--text3)'}}>访问 {acc.access_count||0}次</td><td>{acc.servers?.join(', ')||'-'}</td>
          <td><span style={{fontWeight:600}}>{acc.player_name||'(未知)'}</span> <code className="steam-id" style={{fontSize:'11px'}}>{acc.steam_id64}</code>{acc.has_local_ban&&<StatusPill kind="danger" style={{marginLeft:4,padding:'2px 6px'}}>有封禁</StatusPill>}</td></tr>)}
        </React.Fragment>})}
      </tbody></table></div>}
    </div></div>
    <div className="card"><div className="card-header"><div><div className="card-title">服务器会话与在线记录</div></div></div><div className="card-body p-0">
      {onlineRecords.length===0?<Empty>暂无在线记录。</Empty>:<div className="table-responsive"><table className="data-table player-record-table"><thead><tr><th>服务器</th><th>上报时间</th><th>IP</th><th>Ping</th><th>地图</th></tr></thead><tbody>
        {onlineRecords.map((item,i)=><tr key={`${item.server_id}-${item.reported_at}-${i}`}>
          <td className="fw-600">{item.server_name}</td><td style={{fontFamily:'var(--mono)',fontSize:'12px',whiteSpace:'nowrap'}}>{formatChinaDateTime(item.reported_at,{seconds:false})}</td>
          <td className="steam-id">{item.ip}</td><td>{item.ping}</td><td>{item.current_map||'-'}</td></tr>)}
      </tbody></table></div>}
    </div></div>
  </>;
}

function BehaviorTab({detail, token, onRefresh, toast}) {
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
        {detail.reports.length===0?<Empty>暂无举报记录。</Empty>:<table className="data-table"><thead><tr><th>举报时间</th><th>理由</th><th>状态</th></tr></thead><tbody>
          {detail.reports.map(item=><tr key={item.id} style={{cursor:'pointer'}} onClick={()=>setPopup({type:'report',item})}>
            <td style={{fontFamily:'var(--mono)',fontSize:'12px',whiteSpace:'nowrap'}}>{formatChinaDateTime(item.created_at,{seconds:false})}</td>
            <td className="text-ellipsis" style={{maxWidth:180}}>{item.report_reason}</td>
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
    {popup?.type==='appeal'&&<AppealDetailPopup item={popup.item} onClose={()=>setPopup(null)} onAction={onRefresh} token={token} toast={toast}/>}
    {popup?.type==='report'&&<ReportDetailPopup item={popup.item} onClose={()=>setPopup(null)} onAction={onRefresh} token={token} toast={toast}/>}
    {popup?.type==='feedback'&&<MapFeedbackDetailPopup item={popup.item} onClose={()=>setPopup(null)} onAction={onRefresh} token={token} toast={toast}/>}
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

const TABS = [{key:'status',label:'封禁与白名单'},{key:'network',label:'IP网络与在线轨迹'},{key:'behavior',label:'申诉、举报与反馈'},{key:'audit',label:'全站双向审计'}];

export function PlayerDetailPage() {
  const { session } = useAuth();
  const { toast, toasts, dismiss } = useToast();
  const queryClient = useQueryClient();
  const token = session?.token ?? null;
  const canEdit = session?.role === 'developer' || session?.role === 'admin';
  const [steamInput, setSteamInput] = useState('');
  const [lastQuery, setLastQuery] = useState('');
  const lastQueryRef = useRef('');
  const [detail, setDetail] = useState(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState('');
  const [tab, setTab] = useState('status');
  const [internalSaving, setInternalSaving] = useState(false);
  const [globalBans, setGlobalBans] = useState(null);
  const [showGlobalBans, setShowGlobalBans] = useState(false);

  // 加载玩家详情
  async function loadDetail(input, resetTab = true) {
    const q = input.trim(); if(!q){setError('请输入 SteamID。');return;}
    try{setLoading(true);setError('');const r=await api.playerDetail(token,q);setDetail(r.data);setLastQuery(q);lastQueryRef.current=q;if(resetTab)setTab('status');}catch(e){setDetail(null);setError(e.message||'加载失败');}finally{setLoading(false);}
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

  const profile = detail?.profile;
  const summary = detail?.summary;
  const hasActiveGlobal = globalBans?.some(i=>{const e=i.ban.expires_on;if(!e||e.startsWith('9999'))return true;const d=new Date(e);return !isNaN(d.getTime())&&d>new Date();});
  const hasExpiredGlobal = globalBans&&!hasActiveGlobal&&globalBans.length>0;

  return (<div id="player-detail" className="content-section active">
    <div className="breadcrumb"><span>核心管理</span><span className="sep">›</span><span className="current">玩家全息档案</span></div>
    <div className="page-header"><div><div className="page-title">玩家全息档案矩阵</div><div className="page-sub">封禁履历、IP交叉网络、操作留痕及所有证据工单。</div></div></div>

    <div className="card player-search-card">
      <form className="player-detail-search" onSubmit={e=>{e.preventDefault();loadDetail(steamInput);}}>
        <div className="search-bar-box" style={{maxWidth:380}}>
          <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" width="15" height="15" style={{flexShrink:0,color:'var(--text3)'}}><circle cx="7" cy="7" r="4.5"/><path d="M10.5 10.5L14 14"/></svg>
          <input type="text" placeholder="SteamID64 / SteamID2 / SteamID3 / 主页/ 名称 / IP..." value={steamInput} onChange={e=>setSteamInput(e.target.value)}/>
        </div>
        <button className="btn btn-primary" type="submit" disabled={loading}>{loading?'检索中...':'检索档案'}</button>
        {lastQuery&&<button className="btn btn-outline" type="button" disabled={loading} onClick={refreshDetail}>刷新</button>}
      </form>
      {error&&<div className="player-detail-error">{error}</div>}
    </div>

    {!detail&&!loading?(<div className="player-welcome-card"><div className="player-welcome-inner"><div className="player-welcome-icon"><svg viewBox="0 0 24 24" fill="none" stroke="#fff" strokeWidth="2" width="28" height="28"><circle cx="9" cy="7" r="4"/><path d="M2 21v-2a4 4 0 0 1 4-4h6a4 4 0 0 1 4 4v2"/></svg></div><div className="player-welcome-title">查询玩家全息档案</div><div className="player-welcome-desc">输入 SteamID 查询玩家的完整信息档案。</div></div></div>):null}

    {detail?<>
      <div className="profile-header-card">
        <div className="avatar" style={{width:64,height:64,fontSize:24,flexShrink:0,background:summary.active_ban_count>0?'linear-gradient(135deg,var(--danger-text),#ff708f)':'linear-gradient(135deg,var(--accent),#f0a07a)',color:'#fff'}}>
          {(profile.display_name||profile.steamid64).slice(0,2).toUpperCase()}
        </div>
        <div className="profile-main-info">
          <div className="profile-name-row">
            <div className="profile-name">{profile.display_name||profile.steamid64}</div>
            {detail.internal_profile?.note&&<span className="badge-tag">⚠ {detail.internal_profile.note}</span>}
            {canEdit&&<InternalBtn key={detail.profile.steamid64} detail={detail} onSave={handleSaveInternal} saving={internalSaving}/>}
          </div>
          {hasActiveGlobal&&<div className="status-banner" style={{background:'var(--danger-bg)',color:'var(--danger-text)',borderColor:'var(--danger-text)',cursor:'pointer'}} onClick={()=>setShowGlobalBans(true)}>
            ⚠ 该玩家存在<strong>活跃的</strong>全球封禁记录 ({globalBans.length}条) — 点击查看详情
          </div>}
          {hasExpiredGlobal&&<div className="status-banner offline" style={{cursor:'pointer'}} onClick={()=>setShowGlobalBans(true)}>
            ℹ 该玩家存在<strong>已过期的</strong>全球封禁记录 — 点击查看详情
          </div>}
          <div className={`status-banner offline`}>
            <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" style={{width:14,height:14}}><circle cx="8" cy="8" r="6"/><path d="M8 4v4l2 2"/></svg>
            {summary.last_seen_at?`最后活跃: ${formatChinaDateTime(summary.last_seen_at,{seconds:false})}`:'暂无在线记录'}
            {summary.active_ban_count>0?` · 存在 ${summary.active_ban_count} 条活跃封禁`:' · 无活跃封禁'}
          </div>
          <div className="id-grid">
            <div className="id-item"><span className="id-label">SteamID64</span><code className="steam-id" style={{fontSize:'13px',fontWeight:600}}>{profile.steamid64}</code></div>
            {profile.steamid&&<div className="id-item"><span className="id-label">SteamID2</span><code className="steam-id" style={{color:'var(--text2)'}}>{profile.steamid}</code></div>}
            {profile.steamid3&&<div className="id-item"><span className="id-label">SteamID3</span><code className="steam-id" style={{color:'var(--text2)'}}>{profile.steamid3}</code></div>}
          </div>
        </div>
      </div>

      <div className="profile-tabs">{TABS.map(t=><button key={t.key} className={`profile-tab ${tab===t.key?'active':''}`} onClick={()=>setTab(t.key)} type="button">{t.label}</button>)}</div>
      <div className="profile-tab-pane active">
        {tab==='status'&&<StatusTab detail={detail} token={token} onRefresh={refreshDetail} toast={toast}/>}
        {tab==='network'&&<NetworkTab detail={detail}/>}
        {tab==='behavior'&&<BehaviorTab detail={detail} token={token} onRefresh={refreshDetail} toast={toast}/>}
        {tab==='audit'&&<AuditTab detail={detail}/>}
      </div>
    </>:null}

    {showGlobalBans&&<GlobalBanPopup items={globalBans||[]} onClose={()=>setShowGlobalBans(false)}/>}
    <ToastContainer toasts={toasts} onDismiss={dismiss}/>
  </div>);
}

function InternalBtn({detail, onSave, saving}) {
  const [open, setOpen] = useState(false);
  const [note, setNote] = useState(detail.internal_profile?.note??'');
  const [tt, setTt] = useState(tagsToText(detail.internal_profile?.tags??[]));
  if(!open)return <button className="btn btn-outline" style={{padding:'4px 10px',fontSize:'11.5px',marginLeft:'auto'}} onClick={()=>setOpen(true)}>✎ 编辑备注</button>;
  return <div style={{marginLeft:'auto',display:'flex',flexDirection:'column',gap:6,width:'100%',maxWidth:400}}>
    <input className="form-control" value={note} onChange={e=>setNote(e.target.value)} placeholder="内部备注" style={{fontSize:'12.5px'}}/>
    <div style={{display:'flex',gap:6}}>
      <input className="form-control" value={tt} onChange={e=>setTt(e.target.value)} placeholder="标签" style={{fontSize:'12.5px',flex:1}}/>
      <button className="btn btn-primary btn-sm" disabled={saving} onClick={()=>{onSave({note,tags:textToTags(tt)});setOpen(false);}}>{saving?'保存中...':'保存'}</button>
      <button className="btn btn-outline btn-sm" onClick={()=>setOpen(false)}>取消</button>
    </div>
  </div>;
}

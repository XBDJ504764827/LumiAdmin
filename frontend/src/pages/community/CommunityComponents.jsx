import { useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { BAN_DURATION_OPTIONS, BAN_REASON_OPTIONS } from './onlinePlayers.js';
import { InternalNoteBadge } from '../../shared/InternalNote.jsx';
import { formatChinaDateTime } from '../../shared/time.js';
import { api } from '../../lib/api.js';
import { useAuth } from '../../state/store.js';
import { useToast } from '../../shared/Toast.jsx';

/**
 * 在线玩家卡片组件
 * 从 CommunityPage 提取，用于在线玩家列表展示。
 * 除基础信息（昵称/SteamID/IP/Ping）外，还展示后端批量带出的关联信息：
 * 本地/全球封禁标记、白名单状态、进服统计、IP 关联账号数、当前地图，
 * 并支持懒加载单玩家风险摘要、跳转玩家全息档案。
 */
export function OnlinePlayerCard({ player, serverId, canOperate, onKick, onBan }) {
  const { session } = useAuth();
  const { toast } = useToast();
  const navigate = useNavigate();
  const token = session?.token ?? null;

  const [showKickForm, setShowKickForm] = useState(false);
  const [kickReason, setKickReason] = useState('');
  const [showBanForm, setShowBanForm] = useState(false);
  const [banDuration, setBanDuration] = useState(0);
  const [banReason, setBanReason] = useState(BAN_REASON_OPTIONS[0]);
  const [risk, setRisk] = useState(null);
  const [riskLoading, setRiskLoading] = useState(false);
  const [riskError, setRiskError] = useState('');

  const initial = (player.name || '?')[0].toUpperCase();

  // 风险摘要懒加载：只在用户点开时请求一次
  async function toggleRisk() {
    if (risk) { setRisk(null); return; }
    if (riskLoading || riskError) return;
    setRiskLoading(true);
    setRiskError('');
    try {
      const r = await api.communityPlayerRisk(token, serverId, player.steam_id64);
      setRisk(r.risk_profile ?? null);
    } catch (e) {
      setRiskError(e.message || '加载失败');
      toast({ title: '风险摘要加载失败', message: e.message, tone: 'danger' });
    } finally {
      setRiskLoading(false);
    }
  }

  function handleKick() {
    if (!kickReason.trim()) return;
    onKick(kickReason.trim());
    setShowKickForm(false);
    setKickReason('');
  }

  function handleBan() {
    onBan(banDuration, banReason);
    setShowBanForm(false);
  }

  const whitelistLabel = {
    approved: { text: '白名单已通过', cls: 'online-player-wl-approved' },
    pending: { text: '白名单待审核', cls: 'online-player-wl-pending' },
    rejected: { text: '白名单已拒绝', cls: 'online-player-wl-rejected' },
    revoked: { text: '白名单已撤销', cls: 'online-player-wl-rejected' },
  }[player.whitelist_status] || { text: '无白名单', cls: 'online-player-wl-none' };

  const hasBanBadge = (player.local_ban_count ?? 0) > 0 || (player.global_ban_count ?? 0) > 0;
  const ipLinked = (player.ip_account_count ?? 1) > 1;

  return (
    <div className="online-player-card">
      <div className="online-player-row">
        <div className="online-player-info">
          <div className={`online-player-avatar ${hasBanBadge ? 'online-player-avatar-banned' : ''}`}>{initial}</div>
          <div className="online-player-detail">
            <span className="online-player-name">
              {player.name}
              {(player.local_ban_count ?? 0) > 0 && <span className="online-player-badge online-player-badge-ban" title="存在活跃本地封禁">⚑ 本地封禁</span>}
              {(player.global_ban_count ?? 0) > 0 && <span className="online-player-badge online-player-badge-global" title="存在活跃 KZTimer 全球封禁">⚠ 全球封禁</span>}
            </span>
            <div className="online-player-meta">
              <span className="online-player-tag">{player.steam_id64}</span>
              <span className={`online-player-tag ${ipLinked ? 'online-player-tag-warn' : ''}`} title={ipLinked ? `该 IP 历史上出现过多达 ${player.ip_account_count} 个账号` : undefined}>
                {player.ip}{ipLinked ? `（关联 ${player.ip_account_count} 账号）` : ''}
              </span>
              <span className="online-player-tag online-player-tag-tag-ping">{player.ping}ms</span>
              <span className={`online-player-tag ${whitelistLabel.cls}`}>{whitelistLabel.text}</span>
              {(player.session_count ?? 0) > 0 && (
                <span className="online-player-tag" title={player.last_seen_at ? `最近进服：${formatChinaDateTime(player.last_seen_at, { seconds: false })}` : undefined}>
                  累计 {player.session_count} 次
                </span>
              )}
              {(player.session_count ?? 0) === 0 && <span className="online-player-tag online-player-tag-warn">新面孔（无会话记录）</span>}
            </div>
          </div>
        </div>
        {canOperate ? (
          <div className="online-player-actions">
            <button className="btn btn-outline btn-sm" onClick={() => setShowKickForm(!showKickForm)}>
              {showKickForm ? '取消' : '踢出'}
            </button>
            <button className="btn btn-outline btn-sm text-accent" onClick={() => setShowBanForm(!showBanForm)}>
              {showKickForm ? '取消' : '封禁'}
            </button>
            <button
              className="btn btn-outline btn-sm"
              title="在玩家全息档案中查看完整调查信息"
              onClick={() => navigate(`/player-detail?steamid=${encodeURIComponent(player.steam_id64)}`)}
            >
              详情
            </button>
            <button className="btn btn-outline btn-sm" onClick={toggleRisk} disabled={riskLoading}>
              {riskLoading ? '加载中...' : risk ? '收起风险' : hasBanBadge ? '风险摘要' : '风险'}
            </button>
          </div>
        ) : null}
      </div>
      {risk ? (
        <div className="online-player-risk-panel">
          <div className="online-player-risk-head">
            <span className={`online-player-risk-pill online-player-risk-${risk.severity?.toLowerCase?.() || 'info'}`}>
              风险等级：{{ block: '高', warning: '中', info: '低' }[risk.severity?.toLowerCase?.()] || risk.severity || '未知'}
            </span>
            {risk.summary ? <span className="online-player-risk-summary">{risk.summary}</span> : null}
          </div>
          {risk.recommendation ? <div className="online-player-risk-recommendation">建议：{risk.recommendation}</div> : null}
          {Array.isArray(risk.reasons) && risk.reasons.length > 0 ? (
            <ul className="online-player-risk-reasons">
              {risk.reasons.map((r, i) => <li key={i}>{r.message || r.summary || r.reason || JSON.stringify(r)}</li>)}
            </ul>
          ) : null}
        </div>
      ) : null}
      <InternalNoteBadge steamid64={player.steam_id64} />
      {showKickForm ? (
        <div className="online-player-action-form">
          <div className="action-form-input-row">
            <input
              type="text"
              className="action-form-input"
              placeholder="请输入踢出理由"
              value={kickReason}
              onChange={(e) => setKickReason(e.target.value)}
              onKeyDown={(e) => { if (e.key === 'Enter') handleKick(); }}
              autoFocus
              aria-label="踢出理由"
            />
            <button className="btn btn-danger btn-sm" onClick={handleKick} disabled={!kickReason.trim()}>确认踢出</button>
          </div>
        </div>
      ) : null}
      {showBanForm ? (
        <div className="online-player-ban-form">
          <div className="ban-form-row">
            <div className="ban-form-field">
              <label htmlFor={`ban-duration-${player.steam_id64 || 'unknown'}`}>时长</label>
              <select
                id={`ban-duration-${player.steam_id64 || 'unknown'}`}
                value={banDuration}
                onChange={(e) => setBanDuration(Number(e.target.value))}
              >
                {BAN_DURATION_OPTIONS.map((opt) => (
                  <option key={opt.value} value={opt.value}>{opt.label}</option>
                ))}
              </select>
            </div>
            <div className="ban-form-field">
              <label htmlFor={`ban-reason-${player.steam_id64 || 'unknown'}`}>理由</label>
              <select
                id={`ban-reason-${player.steam_id64 || 'unknown'}`}
                value={banReason}
                onChange={(e) => setBanReason(e.target.value)}
              >
                {BAN_REASON_OPTIONS.map((reason) => (
                  <option key={reason} value={reason}>{reason}</option>
                ))}
              </select>
            </div>
            <button className="btn btn-danger btn-sm" onClick={handleBan}>确认封禁</button>
          </div>
        </div>
      ) : null}
    </div>
  );
}

/**
 * ToggleSwitch — 开关组件
 */
export function ToggleSwitch({ checked, onChange, disabled = false }) {
  return (
    <label className="toggle-switch">
      <input type="checkbox" checked={checked} onChange={(e) => onChange(e.target.checked)} disabled={disabled} />
      <span className="toggle-slider" />
    </label>
  );
}

/**
 * FormSectionCard — 表单分区卡片
 */
export function FormSectionCard({ icon, title, children }) {
  return (
    <div className="form-section-card">
      <div className="form-section-header">
        {icon}
        <span>{title}</span>
      </div>
      {children}
    </div>
  );
}

/**
 * ServerRconFeedback — RCON 测试反馈
 */
export function ServerRconFeedback({ feedback }) {
  if (feedback.testing) {
    return (
      <div className="alert alert-info">
        <span className="alert-icon">⟳</span>
        <div className="alert-content"><div className="alert-text">正在测试 RCON 连接...</div></div>
      </div>
    );
  }
  if (!feedback.tested || !feedback.message) return null;

  const alertClass = feedback.ok ? 'alert-success' : 'alert-error';
  const iconText = feedback.ok ? '✓' : '✕';
  const titleText = feedback.ok ? '连接成功' : '连接失败';

  return (
    <div className={`alert ${alertClass}`}>
      <span className="alert-icon">{iconText}</span>
      <div className="alert-content">
        <div className="alert-title">{titleText}</div>
        <div className="alert-text">
          {feedback.message}
          {feedback.ok && feedback.players.length > 0 ? ` — 检测到 ${feedback.players.length} 名在线玩家` : ''}
        </div>
      </div>
    </div>
  );
}
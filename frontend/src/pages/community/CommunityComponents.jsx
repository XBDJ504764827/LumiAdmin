import React, { useState } from 'react';
import { BAN_DURATION_OPTIONS, BAN_REASON_OPTIONS } from './onlinePlayers.js';

/**
 * 在线玩家卡片组件
 * 从 CommunityPage 提取，用于在线玩家列表展示
 */
export function OnlinePlayerCard({ player, canOperate, onKick, onBan }) {
  const [showKickForm, setShowKickForm] = useState(false);
  const [kickReason, setKickReason] = useState('');
  const [showBanForm, setShowBanForm] = useState(false);
  const [banDuration, setBanDuration] = useState(0);
  const [banReason, setBanReason] = useState(BAN_REASON_OPTIONS[0]);

  const initial = (player.name || '?')[0].toUpperCase();

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

  return (
    <div className="online-player-card">
      <div className="online-player-row">
        <div className="online-player-info">
          <div className="online-player-avatar">{initial}</div>
          <div className="online-player-detail">
            <span className="online-player-name">{player.name}</span>
            <div className="online-player-meta">
              <span className="online-player-tag">{player.steam_id64}</span>
              <span className="online-player-tag">{player.ip}</span>
              <span className="online-player-tag online-player-tag-tag-ping">{player.ping}ms</span>
            </div>
          </div>
        </div>
        {canOperate ? (
          <div className="online-player-actions">
            <button className="btn btn-outline btn-sm" onClick={() => setShowKickForm(!showKickForm)}>
              {showKickForm ? '取消' : '踢出'}
            </button>
            <button className="btn btn-outline btn-sm text-accent" onClick={() => setShowBanForm(!showBanForm)}>
              {showBanForm ? '取消' : '封禁'}
            </button>
          </div>
        ) : null}
      </div>
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

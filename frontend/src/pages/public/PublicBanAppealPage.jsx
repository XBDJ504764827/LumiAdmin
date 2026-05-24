import React, { useState } from 'react';
import { api } from '../../lib/api.js';
import { PublicPageShell } from './PublicPageShell.jsx';

function formatDuration(minutes) {
  if (!minutes || minutes === 0) return '永久';
  if (minutes < 60) return `${minutes} 分钟`;
  if (minutes < 1440) return `${Math.floor(minutes / 60)} 小时`;
  return `${Math.floor(minutes / 1440)} 天`;
}

function formatDateTime(isoString) {
  if (!isoString) return '-';
  try {
    const d = new Date(isoString);
    return d.toLocaleString('zh-CN', { year: 'numeric', month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit' });
  } catch { return isoString; }
}

export function PublicBanAppealPage() {
  const [steamInput, setSteamInput] = useState('');
  const [nickname, setNickname] = useState('');
  const [resolving, setResolving] = useState(false);
  const [resolveError, setResolveError] = useState('');
  const [steamid64, setSteamid64] = useState('');

  const [bans, setBans] = useState(null);
  const [loadingBans, setLoadingBans] = useState(false);
  const [selectedBanId, setSelectedBanId] = useState('');
  const [reason, setReason] = useState('');

  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState('');
  const [message, setMessage] = useState('');

  function handleSteamChange(value) {
    setSteamInput(value);
    setResolveError('');
    setBans(null);
    setSelectedBanId('');
    setSteamid64('');
  }

  async function handleSteamBlur() {
    const trimmed = steamInput.trim();
    if (!trimmed) return;
    setResolving(true);
    setResolveError('');
    try {
      const result = await api.resolveSteam({ steam_input: trimmed });
      if (result.persona_name) setNickname(result.persona_name);
      else setResolveError('未能自动获取 Steam 名称，请手动填写。');
      setSteamid64(result.steamid64);

      // 自动查询封禁记录
      setLoadingBans(true);
      try {
        const banResult = await api.queryActiveBans({ steam_input: trimmed });
        setBans(banResult.bans ?? []);
      } catch {
        setBans([]);
      } finally {
        setLoadingBans(false);
      }
    } catch {
      setResolveError('无法解析 Steam 标识符，请检查输入。');
      setBans(null);
    } finally {
      setResolving(false);
    }
  }

  async function handleSubmit() {
    if (!selectedBanId || !reason.trim() || !steamid64 || !nickname.trim()) return;
    setSubmitting(true);
    setError('');
    setMessage('');
    try {
      await api.submitBanAppeal({
        ban_id: selectedBanId,
        steam_id: steamid64,
        player_name: nickname.trim(),
        appeal_reason: reason.trim(),
      });
      setMessage('申诉提交成功，管理员将会尽快审核您的申诉。');
    } catch (e) {
      setError(e.message || '提交失败，请稍后重试。');
    } finally {
      setSubmitting(false);
    }
  }

  function renderFeedback() {
    if (error) {
      return (
        <div className="alert alert-error">
          <span className="alert-icon">✕</span>
          <span className="alert-text">{error}</span>
        </div>
      );
    }
    if (message) {
      return (
        <div className="alert alert-success">
          <span className="alert-icon">✓</span>
          <div className="alert-content">
            <div className="alert-title">申诉已提交</div>
            <div className="alert-text">{message}</div>
          </div>
        </div>
      );
    }
    return null;
  }

  return (
    <PublicPageShell>
      <div className="public-hero">
        <div className="public-hero-icon" style={{ background: 'linear-gradient(135deg, var(--warning-text, #f59e0b), var(--accent))' }}>
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z" />
          </svg>
        </div>
        <h1>封禁申诉</h1>
        <p>如果您认为自己被误封，请通过此页面提交申诉，管理员将会重新审核您的封禁记录。</p>
      </div>

      <div style={{ maxWidth: 520, margin: '0 auto' }}>
        <div className="public-card">
          <div className="public-card-body">
            {/* Steam 标识符输入 */}
            <div className="form-group">
              <label>Steam 标识符 <span style={{ color: 'var(--accent)' }}>*</span></label>
              <input
                type="text"
                className="form-control"
                value={steamInput}
                onChange={(e) => handleSteamChange(e.target.value)}
                onBlur={handleSteamBlur}
                placeholder="SteamID64 / SteamID / 个人主页链接"
                disabled={submitting || resolving}
              />
              <div className="form-hint">
                支持 SteamID64、Steam2、Steam3 和 Steam 个人主页链接
                {resolving && <span className="form-hint-loading">正在查询信息...</span>}
              </div>
              {resolveError && (
                <div className="form-hint" style={{ color: 'var(--warning-text)', marginTop: 4 }}>{resolveError}</div>
              )}
            </div>

            {/* 游戏昵称 */}
            <div className="form-group">
              <label>游戏昵称 <span style={{ color: 'var(--accent)' }}>*</span></label>
              <input
                type="text"
                className="form-control"
                value={nickname}
                onChange={(e) => setNickname(e.target.value)}
                placeholder="您的 Steam 名称"
                disabled={submitting}
              />
              <div className="form-hint">输入 Steam 标识符后将自动获取昵称</div>
            </div>

            {/* 封禁记录选择 */}
            {loadingBans && (
              <div className="form-group">
                <div style={{ padding: '12px 0', color: 'var(--text3)', fontSize: 13 }}>正在查询封禁记录...</div>
              </div>
            )}

            {bans !== null && !loadingBans && (
              <div className="form-group">
                <label>选择要申诉的封禁记录 <span style={{ color: 'var(--accent)' }}>*</span></label>
                {bans.length === 0 ? (
                  <div style={{ padding: '12px 0', color: 'var(--text3)', fontSize: 13, background: 'var(--hover)', borderRadius: 6, textAlign: 'center' }}>
                    未找到该 SteamID 的活跃封禁记录，无需申诉。
                  </div>
                ) : (
                  <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
                    {bans.map((ban) => (
                      <label
                        key={ban.id}
                        className={`ban-appeal-record ${selectedBanId === ban.id ? 'selected' : ''}`}
                        style={{ cursor: 'pointer' }}
                      >
                        <input
                          type="radio"
                          name="banRecord"
                          value={ban.id}
                          checked={selectedBanId === ban.id}
                          onChange={() => setSelectedBanId(ban.id)}
                          disabled={submitting}
                          style={{ marginTop: 2 }}
                        />
                        <div style={{ flex: 1, minWidth: 0 }}>
                          <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 4 }}>
                            <span className="status-pill pill-accent" style={{ fontSize: 11 }}>
                              {ban.ban_type === 'steam' ? 'Steam 封禁' : 'IP 封禁'}
                            </span>
                            <span style={{ fontSize: 12, color: 'var(--text3)' }}>
                              {formatDuration(ban.duration_minutes)}
                              {ban.server_name ? ` · ${ban.server_name}` : ''}
                            </span>
                          </div>
                          <div style={{ fontSize: 13, color: 'var(--text2)', marginBottom: 2 }}>
                            <strong>封禁原因：</strong>{ban.reason}
                          </div>
                          <div style={{ fontSize: 12, color: 'var(--text3)' }}>
                            封禁时间：{formatDateTime(ban.created_at)} · 操作人：{ban.operator_name}
                            {ban.source === 'game_plugin' ? ' · 来源：游戏插件' : ''}
                          </div>
                        </div>
                      </label>
                    ))}
                  </div>
                )}
              </div>
            )}

            {/* 申诉理由 */}
            {bans !== null && bans.length > 0 && (
              <div className="form-group">
                <label>申诉理由 <span style={{ color: 'var(--accent)' }}>*</span></label>
                <textarea
                  className="form-control"
                  value={reason}
                  onChange={(e) => setReason(e.target.value)}
                  placeholder="请详细说明您认为该封禁有误的原因，或提供相关证据..."
                  rows={4}
                  disabled={submitting}
                  style={{ resize: 'vertical', minHeight: 100 }}
                />
                <div className="form-hint">请如实填写，管理员将根据您的理由重新审核封禁记录。</div>
              </div>
            )}

            {renderFeedback()}

            {/* 提交按钮 */}
            {bans !== null && bans.length > 0 && (
              <button
                className="btn btn-accent"
                style={{ width: '100%', padding: 12, fontSize: 14, marginTop: 6 }}
                type="button"
                disabled={submitting || resolving || !selectedBanId || !reason.trim()}
                onClick={handleSubmit}
              >
                {submitting ? '提交中...' : '提交封禁申诉'}
              </button>
            )}
          </div>
        </div>

        <div style={{ textAlign: 'center', marginTop: 16, fontSize: 12, color: 'var(--text3)' }}>
          提交申诉后请耐心等待，管理员会尽快审核
        </div>
      </div>
    </PublicPageShell>
  );
}

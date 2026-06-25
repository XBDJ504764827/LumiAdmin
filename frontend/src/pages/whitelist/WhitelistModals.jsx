import React, { useCallback, useEffect, useRef, useState } from 'react';
import { Modal } from '../../shared/Modal.jsx';
import { formatChinaDateTime } from '../../shared/time.js';
import { InternalNoteBadge } from '../../shared/InternalNote.jsx';

// ---------------------------------------------------------------------------
// 全球封禁记录列表（共享组件）
// ---------------------------------------------------------------------------

function GlobalBanRecordList({ bans = [] }) {
  if (!bans.length) {
    return <div className="global-ban-empty">暂无封禁记录</div>;
  }

  return (
    <div className="global-ban-list">
      {bans.map((ban, index) => (
        <div key={index} className="global-ban-item">
          <div className="global-ban-item-header">
            <span className="global-ban-type">{ban.ban_type || '作弊'}</span>
            {ban.expires_on ? (
              <span className="global-ban-temporary">临时</span>
            ) : (
              <span className="global-ban-permanent">永久</span>
            )}
          </div>
          <div className="global-ban-item-body">
            {ban.player_name && (
              <div className="global-ban-field">
                <span className="global-ban-label">玩家</span>
                <span className="global-ban-value">{ban.player_name}</span>
              </div>
            )}
            {ban.notes && (
              <div className="global-ban-field">
                <span className="global-ban-label">备注</span>
                <span className="global-ban-value">{ban.notes}</span>
              </div>
            )}
            {ban.stats && (
              <div className="global-ban-field">
                <span className="global-ban-label">统计</span>
                <span className="global-ban-value global-ban-stats">{ban.stats}</span>
              </div>
            )}
            {ban.created_on && (
              <div className="global-ban-field">
                <span className="global-ban-label">封禁时间</span>
                <span className="global-ban-value">{formatChinaDateTime(ban.created_on)}</span>
              </div>
            )}
            {ban.expires_on && (
              <div className="global-ban-field">
                <span className="global-ban-label">到期时间</span>
                <span className="global-ban-value">{formatChinaDateTime(ban.expires_on)}</span>
              </div>
            )}
            {ban.server_name && (
              <div className="global-ban-field">
                <span className="global-ban-label">服务器</span>
                <span className="global-ban-value">{ban.server_name}</span>
              </div>
            )}
          </div>
        </div>
      ))}
    </div>
  );
}

function riskActionLabel(action) {
  if (action === 'deny') return '高风险玩家';
  if (action === 'require_force') return '高风险玩家';
  if (action === 'warn') return '中风险玩家';
  return '低风险玩家';
}

function riskTone(profile) {
  if (profile?.action === 'deny' || profile?.action === 'require_force') return 'danger';
  if (profile?.action === 'warn') return 'warning';
  return 'default';
}

function RiskProfilePanel({ profile }) {
  if (!profile || profile.action === 'allow') return null;
  const tone = riskTone(profile);
  return (
    <div className={`whitelist-risk-panel ${tone}`}>
      <div className="whitelist-risk-panel-head">
        <span>⚠</span>
        <strong>{riskActionLabel(profile.action)}</strong>
      </div>
      <div className="whitelist-risk-summary">{profile.summary}</div>
      {profile.recommendation ? <div className="whitelist-risk-recommendation">{profile.recommendation}</div> : null}
      {profile.reasons?.length > 0 ? (
        <div className="whitelist-risk-reason-list">
          {profile.reasons.map((reason, index) => (
            <div key={`${reason.code}-${index}`} className="whitelist-risk-reason">
              <span>{reason.message}</span>
              {reason.ip ? <code>{reason.ip}</code> : null}
              {reason.steamid64 ? <code>{reason.steamid64}</code> : null}
            </div>
          ))}
        </div>
      ) : null}
      {profile.linked_accounts?.length > 0 ? (
        <div className="whitelist-risk-linked">
          {profile.linked_accounts.slice(0, 5).map((account) => (
            <div key={account.steamid64} className="whitelist-risk-linked-row">
              <span>{account.player_name || '(未知)'}</span>
              <code>{account.steamid64}</code>
              <span>{account.has_active_global_ban ? '全球封禁' : account.has_active_local_ban ? '本地封禁' : '历史风险'}</span>
            </div>
          ))}
        </div>
      ) : null}
    </div>
  );
}

// ---------------------------------------------------------------------------
// 手动添加白名单 Modal
// ---------------------------------------------------------------------------

export function ManualCreateModal({ open, onClose, form, setForm, error, onSubmit, submitting }) {
  return (
    <Modal
      open={open}
      title="手动添加白名单"
      onClose={onClose}
      footer={
        <>
          <button className="btn btn-outline" onClick={onClose}>取消</button>
          <button className="btn btn-primary" onClick={onSubmit} disabled={submitting}>添加</button>
        </>
      }
    >
      <div className="form-group">
        <label>玩家名称</label>
        <input
          type="text"
          className="form-control"
          value={form.nickname}
          onChange={(event) => setForm((prev) => ({ ...prev, nickname: event.target.value }))}
          placeholder="玩家名称"
        />
      </div>
      <div className="form-group">
        <label>玩家标识</label>
        <input
          type="text"
          className="form-control"
          value={form.steam_input}
          onChange={(event) => setForm((prev) => ({ ...prev, steam_input: event.target.value }))}
          placeholder="SteamID64 / SteamID / Steam 个人主页链接"
        />
      </div>
      <label className="checkbox-line mb-12">
        <input
          type="checkbox"
          checked={Boolean(form.force)}
          onChange={(event) => setForm((prev) => ({ ...prev, force: event.target.checked }))}
        />
        <span>强制通过风险检查</span>
      </label>
      {form.force ? (
        <div className="form-group">
          <label>强制通过原因</label>
          <textarea
            className="form-control"
            rows={3}
            value={form.reason || ''}
            onChange={(event) => setForm((prev) => ({ ...prev, reason: event.target.value }))}
            placeholder="请说明为什么需要绕过同 IP 风险或历史风险"
          />
        </div>
      ) : null}
      {error ? <div className="text-accent">{error}</div> : null}
    </Modal>
  );
}

// ---------------------------------------------------------------------------
// 拒绝白名单申请 Modal
// ---------------------------------------------------------------------------

export function RejectModal({ open, onClose, reason, setReason, error, onSubmit, submitting }) {
  return (
    <Modal
      open={open}
      title="拒绝白名单申请"
      onClose={onClose}
      footer={
        <>
          <button className="btn btn-outline" onClick={onClose}>取消</button>
          <button className="btn btn-primary" onClick={onSubmit} disabled={submitting}>确认拒绝</button>
        </>
      }
    >
      <div className="form-group">
        <label>拒绝理由</label>
        <textarea
          className="form-control"
          rows={4}
          value={reason}
          onChange={(event) => setReason(event.target.value)}
          placeholder="请输入拒绝理由"
        />
      </div>
      {error ? <div className="text-accent">{error}</div> : null}
    </Modal>
  );
}

// ---------------------------------------------------------------------------
// 通过白名单申请（含风险检查）Modal
// ---------------------------------------------------------------------------

export function ApproveModal({ open, onClose, mode = 'approve', item, bans = [], risk, riskProfile, reason, setReason, error, secondsRemaining, onSubmit, submitting }) {
  const forceRequired = ['deny', 'require_force'].includes(riskProfile?.action) || bans.length > 0;
  const titleText = mode === 'restore'
    ? forceRequired ? '恢复白名单（强制通过）' : '恢复白名单（风险确认）'
    : forceRequired ? '通过白名单申请（强制通过）' : '通过白名单申请（风险确认）';
  const submitText = forceRequired ? '确认强制通过' : '确认通过';
  return (
    <Modal
      open={open}
      title={
        <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
          <span style={{ fontSize: 20, color: 'var(--accent)' }}>⚠</span>
          <span>{titleText}</span>
        </div>
      }
      onClose={onClose}
      footer={
        <>
          <button className="btn btn-outline" onClick={onClose}>取消</button>
          <button
            className="btn btn-primary"
            onClick={onSubmit}
            disabled={submitting || secondsRemaining > 0}
          >
            {secondsRemaining > 0 ? `${secondsRemaining} 秒后可${forceRequired ? '强制通过' : '通过'}` : submitting ? '处理中...' : submitText}
          </button>
        </>
      }
    >
      <div className="global-ban-alert mb-12">
        <div className="global-ban-alert-icon">⚠</div>
        <div className="global-ban-alert-text">
          该玩家命中白名单风险策略。请完整查看下方风险详情，倒计时结束并填写通过理由后才可继续。
        </div>
      </div>
      <RiskProfilePanel profile={riskProfile} />
      {risk ? (
        <div className={`global-ban-risk global-ban-risk-${risk.tone}`}>
          <div className="global-ban-risk-title">{risk.title}</div>
          {risk.reasons.length > 0 ? (
            <div className="global-ban-risk-reasons">
              {risk.reasons.map((r) => (
                <span key={r}>{r}</span>
              ))}
            </div>
          ) : null}
        </div>
      ) : null}
      <div className="global-ban-info">
        <div><strong>玩家:</strong> {item?.nickname ?? '-'}</div>
        <div><strong>SteamID64:</strong> <code>{item?.steamid64 ?? '-'}</code></div>
      </div>
      <InternalNoteBadge steamid64={item?.steamid64} />
      {bans.length > 0 ? (
        <div className="mb-16">
          <GlobalBanRecordList bans={bans} />
        </div>
      ) : null}
      <div className="form-group">
        <label>{forceRequired ? '强制通过理由' : '通过理由'}</label>
        <textarea
          className="form-control"
          rows={4}
          value={reason}
          onChange={(event) => setReason(event.target.value)}
          placeholder={forceRequired ? '请说明为什么需要强制通过该玩家' : '请说明为什么在命中风险的情况下仍然通过'}
        />
      </div>
      {error ? <div className="text-accent">{error}</div> : null}
    </Modal>
  );
}

export function RiskDetailModal({ open, onClose, item }) {
  return (
    <Modal
      open={open}
      title="关联账号风险"
      onClose={onClose}
      footer={<button className="btn btn-primary" type="button" onClick={onClose}>关闭</button>}
    >
      <div className="global-ban-info">
        <div><strong>玩家:</strong> {item?.nickname ?? '-'}</div>
        <div><strong>SteamID64:</strong> <code>{item?.steamid64 ?? '-'}</code></div>
      </div>
      <RiskProfilePanel profile={item?.risk_profile} />
      {!item?.risk_profile || item.risk_profile.action === 'allow' ? (
        <div className="global-ban-empty">当前没有关联账号风险。</div>
      ) : null}
    </Modal>
  );
}

// ---------------------------------------------------------------------------
// 全球封禁详情 Modal
// ---------------------------------------------------------------------------

export function BanDetailModal({ open, onClose, steamid64, bans }) {
  return (
    <Modal
      open={open}
      title={
        <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
          <span style={{ fontSize: 20, color: 'var(--accent)' }}>⚠</span>
          <span>全球封禁记录</span>
        </div>
      }
      onClose={onClose}
      footer={<button className="btn btn-primary" onClick={onClose}>关闭</button>}
    >
      <div className="global-ban-detail">
        <div className="global-ban-alert">
          <div className="global-ban-alert-icon">⚠</div>
          <div className="global-ban-alert-text">
            该玩家在全球 KZ 封禁库中有 <strong>{bans.length}</strong> 条封禁记录，请谨慎审核！
          </div>
        </div>
        <div className="global-ban-info">
          <div><strong>SteamID64:</strong> <code>{steamid64}</code></div>
        </div>
        <GlobalBanRecordList bans={bans} />
      </div>
    </Modal>
  );
}

// ---------------------------------------------------------------------------
// gokz.top 数据获取（会话级缓存 + 批量接口）
// ---------------------------------------------------------------------------

const GOKZ_CACHE = new Map();

const KZ_MODES = [
  { key: 'KZT', label: 'KZT' },
  { key: 'SKZ', label: 'SKZ' },
  { key: 'VNL', label: 'VNL' },
  { key: 'OVR', label: 'OVR' },
];

async function fetchPlayerKzStats(steamid64) {
  if (GOKZ_CACHE.has(steamid64)) return GOKZ_CACHE.get(steamid64);

  const results = {};
  try {
    const response = await fetch('/api/public/gokz/player-stats/batch', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ steamid64 }),
    });
    if (response.ok) {
      const data = await response.json();
      for (const mode of KZ_MODES) {
        const s = data[mode.key];
        results[mode.key] = s && s.rating != null ? {
          rating: s.rating,
          rank: s.rank ?? null,
          points: s.points ?? 0,
          mapFinish: s.unique_map_finishes ?? 0,
        } : null;
      }
    }
  } catch { /* 批量请求失败，全部置 null */ }

  GOKZ_CACHE.set(steamid64, results);
  return results;
}

// ---------------------------------------------------------------------------
// 玩家详细信息 Modal（白名单待审核）
// ---------------------------------------------------------------------------

export function PlayerDetailModal({ open, onClose, item, canReview, submitting, onApprove, onReject }) {
  const [stats, setStats] = useState(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState('');
  const itemRef = useRef(item);

  useEffect(() => { itemRef.current = item; }, [item]);

  const loadStats = useCallback(async () => {
    const steamid64 = itemRef.current?.steamid64;
    if (!steamid64) return;
    // 前端会话缓存命中时直接使用，不显示 loading
    if (GOKZ_CACHE.has(steamid64)) {
      React.startTransition(() => { setStats(GOKZ_CACHE.get(steamid64)); });
      return;
    }
    try {
      setLoading(true);
      setError('');
      const data = await fetchPlayerKzStats(steamid64);
      React.startTransition(() => { setStats(data); });
    } catch {
      setError('加载 KZ 统计数据失败，请稍后重试。');
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    if (open && item?.steamid64) React.startTransition(() => { loadStats(); });
    if (!open) React.startTransition(() => { setError(''); });
  }, [open, item?.steamid64, loadStats]);
  const forceApprove = item?.risk_profile?.action === 'deny';

  return (
    <Modal
      open={open}
      title="玩家详细信息"
      onClose={onClose}
      footer={
        canReview && item ? (
          <>
            <button className="btn btn-outline" onClick={onClose}>关闭</button>
            <button className="action-btn action-btn-danger" onClick={() => { onClose(); onReject(item); }} disabled={submitting}>拒绝</button>
            <button className="action-btn action-btn-success" style={{ color: forceApprove ? 'var(--danger-text)' : '#22c55e' }} onClick={() => { onClose(); onApprove(item); }} disabled={submitting} title={forceApprove ? '需要填写理由后强制通过' : undefined}>{forceApprove ? '强制通过' : '通过'}</button>
          </>
        ) : (
          <button className="btn btn-outline" onClick={onClose}>关闭</button>
        )
      }
    >
      {item ? (
        <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
          <div className="form-group">
            <label className="mb-4">玩家信息</label>
            <div style={{ color: 'var(--text2)', fontSize: 13 }}>
              <div>游戏昵称：{item.nickname || '-'}</div>
              <div>Steam 名称：{item.steam_persona_name || '-'}</div>
              <div>SteamID64：{item.steamid64 || '-'}</div>
              <div>SteamID2：{item.steamid || '-'}</div>
              <div>SteamID3：{item.steamid3 || '-'}</div>
            </div>
          </div>

          <div className="form-group">
            <label className="mb-4">申请信息</label>
            <div style={{ color: 'var(--text2)', fontSize: 13 }}>
              <div>申请时间：{item.applied_at ? formatChinaDateTime(item.applied_at) : '-'}</div>
            </div>
          </div>

          <RiskProfilePanel profile={item.risk_profile} />

          <div className="form-group">
            <label className="mb-4">GOKZ.TOP 统计</label>
            {loading ? (
              <div className="gokz-loading">加载中…</div>
            ) : error ? (
              <div className="gokz-error">{error}</div>
            ) : stats ? (() => {
              const validRows = KZ_MODES
                .map((mode) => ({ mode, s: stats[mode.key] }))
                .filter(({ s }) => s);
              if (validRows.length === 0) {
                return (
                  <div className="gokz-empty">
                    <div className="gokz-empty-icon">ℹ</div>
                    <div className="gokz-empty-title">该玩家在 GOKZ.TOP 暂无跳图记录</div>
                    <div className="gokz-empty-desc">可能原因：玩家从未在全球站跳过图，或所有记录仅在未验证服务器。这不代表数据加载失败。</div>
                  </div>
                );
              }
              return (
                <div className="gokz-list">
                  {validRows.map(({ mode, s }) => (
                    <div key={mode.key} className={`gokz-row gokz-row-${mode.key.toLowerCase()}`}>
                      <span className="gokz-row-mode">{mode.label}</span>
                      <span className="gokz-row-val">{s.rating !== null ? s.rating.toFixed(2) : '-'}</span>
                      <span className="gokz-row-val">{s.rank !== null ? `#${s.rank}` : '-'}</span>
                      <span className="gokz-row-val">{s.mapFinish} 张</span>
                    </div>
                  ))}
                </div>
              );
            })() : (
              <div className="gokz-empty">
                <div className="gokz-empty-icon">ℹ</div>
                <div className="gokz-empty-title">未能获取 GOKZ.TOP 数据</div>
                <div className="gokz-empty-desc">外部 API 暂不可用，请稍后重试。</div>
              </div>
            )}
          </div>

          <InternalNoteBadge steamid64={item?.steamid64} />
        </div>
      ) : null}
    </Modal>
  );
}

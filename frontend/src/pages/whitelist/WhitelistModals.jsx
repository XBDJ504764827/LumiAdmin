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

const RISK_GRAPH_LIMIT = 8;

function RiskChip({ tone = 'default', children }) {
  return <span className={`risk-chip risk-chip-${tone}`}>{children}</span>;
}

function accountStatusChips(account) {
  const chips = [];
  if (account.has_active_global_ban) chips.push(<RiskChip key="global" tone="danger">全球封禁</RiskChip>);
  if (account.has_active_local_ban) chips.push(<RiskChip key="local" tone="danger">本地封禁</RiskChip>);
  if (account.rejected_whitelist_count > 0) chips.push(<RiskChip key="rejected" tone="warning">白名单被拒 {account.rejected_whitelist_count} 次</RiskChip>);
  if (chips.length === 0) chips.push(<RiskChip key="clean" tone="default">无风险标记</RiskChip>);
  return chips;
}

// 消息中可能内嵌 RFC3339 时间戳（历史数据），统一替换为本地可读时间
const ISO_TIME_RE = /\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:\d{2})/g;

function friendlyReasonMessage(message) {
  return String(message || '').replace(ISO_TIME_RE, (raw) => {
    try {
      const formatted = formatChinaDateTime(raw, { seconds: false });
      return formatted === raw ? raw : formatted;
    } catch {
      return raw;
    }
  });
}

// 风险原因按来源分组展示
const REASON_GROUPS = [
  { key: 'self', title: '当前账号风险', match: (code) => code.startsWith('self_') },
  { key: 'linked', title: '关联账号风险', match: (code) => code.startsWith('linked_') },
  { key: 'other', title: '其他提示', match: () => true },
];

function severityTone(severity) {
  if (severity === 'block') return 'danger';
  if (severity === 'warning') return 'warning';
  return 'default';
}

function severityIcon(severity) {
  if (severity === 'block') return '⛔';
  if (severity === 'warning') return '⚠';
  return 'ℹ';
}

function RiskReasonItem({ reason, accountName }) {
  const tone = severityTone(reason.severity);
  return (
    <div className={`whitelist-risk-reason risk-severity-${tone}`}>
      <span className="whitelist-risk-reason-icon">{severityIcon(reason.severity)}</span>
      <span className="whitelist-risk-reason-text">{friendlyReasonMessage(reason.message)}</span>
      <span className="whitelist-risk-reason-meta">
        {reason.ip ? <code>IP：{reason.ip}</code> : null}
        {reason.steamid64 ? <code>{accountName ? `${accountName} · ` : ''}{reason.steamid64}</code> : null}
      </span>
    </div>
  );
}

// 用连线图展示当前玩家与各关联账号之间的关系，线上的标签即关联依据（共享 IP）
function RiskLinkGraph({ profile, mainName }) {
  const accounts = profile.linked_accounts || [];
  const visible = accounts.slice(0, RISK_GRAPH_LIMIT);
  const more = accounts.length - visible.length;
  if (visible.length === 0) return null;

  const selfReasons = (profile.reasons || []).filter((reason) => reason.code.startsWith('self_'));
  const selfChips = [];
  if (selfReasons.some((reason) => reason.code.includes('global'))) {
    selfChips.push(<RiskChip key="global" tone="danger">全球封禁</RiskChip>);
  }
  if (selfReasons.some((reason) => reason.code.includes('local'))) {
    selfChips.push(<RiskChip key="local" tone="danger">本地封禁</RiskChip>);
  }

  return (
    <div className="whitelist-risk-graph">
      <div className="risk-graph-row risk-graph-row-main">
        <div className="risk-graph-stub" />
        <div className="risk-graph-node risk-graph-node-main">
          <div className="risk-graph-node-head">
            <span className="risk-graph-node-role">当前玩家</span>
            {selfChips}
          </div>
          <div className="risk-graph-node-name">{mainName || '(未知玩家)'}</div>
          <code className="risk-graph-node-id">{profile.steamid64}</code>
        </div>
      </div>
      {visible.map((account) => (
        <div key={account.steamid64} className="risk-graph-row">
          <div className="risk-graph-stub" />
          <div className="risk-graph-arm">
            <span className="risk-graph-ip" title="关联方式（共享 IP）">
              共享IP：{account.shared_ips?.length ? account.shared_ips.join('、') : '未知'}
            </span>
          </div>
          <div className="risk-graph-node">
            <div className="risk-graph-node-name">{account.player_name || '(未知玩家)'}</div>
            <code className="risk-graph-node-id">{account.steamid64}</code>
            <div className="risk-graph-node-meta">
              {accountStatusChips(account)}
              {account.last_seen_at ? (
                <span className="risk-graph-node-seen">最近出现 {formatChinaDateTime(account.last_seen_at, { seconds: false })}</span>
              ) : null}
            </div>
          </div>
        </div>
      ))}
      {more > 0 ? <div className="risk-graph-more">另有 {more} 个关联账号未在图中展示，详见下方风险原因。</div> : null}
    </div>
  );
}

function RiskProfilePanel({ profile, mainName }) {
  if (!profile || profile.action === 'allow') return null;
  const tone = riskTone(profile);
  const reasons = profile.reasons || [];
  const linkedAccounts = profile.linked_accounts || [];
  return (
    <div className={`whitelist-risk-panel ${tone}`}>
      <div className="whitelist-risk-panel-head">
        <span>⚠</span>
        <strong>{riskActionLabel(profile.action)}</strong>
      </div>
      <div className="whitelist-risk-summary">{profile.summary}</div>
      {linkedAccounts.length > 0 ? (
        <div className="whitelist-risk-section">
          <div className="whitelist-risk-section-title">关联账号</div>
          <RiskLinkGraph profile={profile} mainName={mainName} />
        </div>
      ) : null}
      {reasons.length > 0 ? (
        <div className="whitelist-risk-section">
          <div className="whitelist-risk-section-title">风险原因</div>
          <div className="whitelist-risk-reason-groups">
            {REASON_GROUPS.map((group) => {
              const groupReasons = reasons.filter((reason) => group.match(reason.code));
              if (groupReasons.length === 0) return null;
              return (
                <div key={group.key} className="whitelist-risk-reason-group">
                  <div className="whitelist-risk-reason-group-title">
                    <span>{group.title}</span>
                    <span className="whitelist-risk-reason-group-count">{groupReasons.length}</span>
                  </div>
                  <div className="whitelist-risk-reason-list">
                    {groupReasons.map((reason, index) => {
                      const account = (profile.linked_accounts || []).find((a) => a.steamid64 === reason.steamid64);
                      return (
                        <RiskReasonItem
                          key={`${reason.code}-${index}`}
                          reason={reason}
                          accountName={account?.player_name}
                        />
                      );
                    })}
                  </div>
                </div>
              );
            })}
          </div>
        </div>
      ) : null}
      {profile.recommendation ? (
        <div className="whitelist-risk-recommendation">
          <span className="whitelist-risk-recommendation-label">处理建议</span>
          <span>{profile.recommendation}</span>
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
      wide
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
      <RiskProfilePanel profile={riskProfile} mainName={item?.nickname} />
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
      wide
      title="关联账号风险"
      onClose={onClose}
      footer={<button className="btn btn-primary" type="button" onClick={onClose}>关闭</button>}
    >
      <div className="global-ban-info">
        <div><strong>玩家:</strong> {item?.nickname ?? '-'}</div>
        <div><strong>SteamID64:</strong> <code>{item?.steamid64 ?? '-'}</code></div>
      </div>
      <RiskProfilePanel profile={item?.risk_profile} mainName={item?.nickname} />
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
  const canReviewPending = canReview && item?.status === 'pending';

  return (
    <Modal
      open={open}
      wide
      title="玩家详细信息"
      onClose={onClose}
      footer={
        canReviewPending ? (
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
              <div>联系方式：{item.contact || '-'}</div>
              <div>申请时间：{item.applied_at ? formatChinaDateTime(item.applied_at) : '-'}</div>
            </div>
          </div>

          <RiskProfilePanel profile={item.risk_profile} mainName={item.nickname} />

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

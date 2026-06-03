import React from 'react';
import { Modal } from '../../shared/Modal.jsx';
import { formatChinaDateTime } from '../../shared/time.js';

// ---------------------------------------------------------------------------
// 全球封禁记录列表（共享组件）
// ---------------------------------------------------------------------------

function GlobalBanRecordList({ bans }) {
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
      {error ? <div style={{ color: 'var(--accent)' }}>{error}</div> : null}
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
      {error ? <div style={{ color: 'var(--accent)' }}>{error}</div> : null}
    </Modal>
  );
}

// ---------------------------------------------------------------------------
// 通过白名单申请（含全球封禁检查）Modal
// ---------------------------------------------------------------------------

export function ApproveModal({ open, onClose, item, bans, risk, reason, setReason, error, secondsRemaining, onSubmit, submitting }) {
  return (
    <Modal
      open={open}
      title={
        <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
          <span style={{ fontSize: 20, color: 'var(--accent)' }}>⚠</span>
          <span>通过白名单申请（全球封禁）</span>
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
            {secondsRemaining > 0 ? `${secondsRemaining} 秒后可通过` : submitting ? '处理中...' : '确认通过'}
          </button>
        </>
      }
    >
      <div className="global-ban-alert" style={{ marginBottom: 12 }}>
        <div className="global-ban-alert-icon">⚠</div>
        <div className="global-ban-alert-text">
          该玩家在全球 KZ 封禁库中有 <strong>{bans.length}</strong> 条封禁记录。请完整查看下方封禁详情，倒计时结束并填写通过理由后才可正式通过。
        </div>
      </div>
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
      <div style={{ marginBottom: 16 }}>
        <GlobalBanRecordList bans={bans} />
      </div>
      <div className="form-group">
        <label>通过理由</label>
        <textarea
          className="form-control"
          rows={4}
          value={reason}
          onChange={(event) => setReason(event.target.value)}
          placeholder="请说明为什么在有全球封禁记录的情况下仍然通过"
        />
      </div>
      {error ? <div style={{ color: 'var(--accent)' }}>{error}</div> : null}
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

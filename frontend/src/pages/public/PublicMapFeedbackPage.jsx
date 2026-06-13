import { useState } from 'react';
import { api } from '../../lib/api.js';
import { PublicPageShell } from './PublicPageShell.jsx';

const FEEDBACK_TYPES = [
  {
    value: 'missing',
    label: '地图缺失',
    icon: '🗺',
    desc: '服务器上缺少某张地图，无法选择或游玩',
    placeholder: '请说明缺失的地图名称、在哪个服务器发现缺失、以及该地图的下载链接（如有）',
  },
  {
    value: 'broken',
    label: '地图异常',
    icon: '🔧',
    desc: '地图存在但无法正常游玩（如掉帧、贴图丢失、机关失灵等）',
    placeholder: '请说明地图名称、异常的具体表现、在哪个服务器出现、以及复现步骤',
  },
  {
    value: 'request',
    label: '需要添加地图',
    icon: '➕',
    desc: '希望服务器添加新的地图',
    placeholder: '请说明希望添加的地图名称、下载链接（如有）、以及添加理由',
  },
];

const STATUS_MAP = {
  pending:  { label: '待处理', icon: '⏳', color: '#f59e0b', bg: '#fef3c7' },
  resolved: { label: '已处理', icon: '✅', color: '#22c55e', bg: '#dcfce7' },
  rejected: { label: '已驳回', icon: '❌', color: '#ef4444', bg: '#fee2e2' },
};

function formatChinaDateTime(iso, opts) {
  if (!iso) return '';
  try {
    const d = new Date(iso);
    const pad = (n) => String(n).padStart(2, '0');
    const base = `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
    if (opts?.seconds !== false) return `${base}:${pad(d.getSeconds())}`;
    return base;
  } catch { return iso; }
}

export function PublicMapFeedbackPage() {
  const [feedbackType, setFeedbackType] = useState('');
  const [steamInput, setSteamInput] = useState('');
  const [contact, setContact] = useState('');
  const [detail, setDetail] = useState('');
  const [phase, setPhase] = useState('idle');
  const [error, setError] = useState('');
  const [message, setMessage] = useState('');

  // 查询历史反馈
  const [existingFeedback, setExistingFeedback] = useState(null);
  const [loadingFeedback, setLoadingFeedback] = useState(false);

  const selectedType = FEEDBACK_TYPES.find((t) => t.value === feedbackType);

  async function handleSteamBlur() {
    if (!steamInput.trim()) {
      setExistingFeedback(null);
      return;
    }
    setLoadingFeedback(true);
    try {
      const result = await api.queryMapFeedbackStatus({ steam_input: steamInput.trim() });
      setExistingFeedback(result.feedback ?? []);
    } catch {
      setExistingFeedback(null);
    } finally {
      setLoadingFeedback(false);
    }
  }

  async function handleSubmit() {
    if (!feedbackType || !detail.trim() || phase !== 'idle') return;
    setPhase('submitting');
    setError('');
    setMessage('');

    try {
      await api.submitMapFeedback({
        feedback_type: feedbackType,
        steam_input: steamInput.trim() || null,
        contact: contact.trim() || null,
        detail: detail.trim(),
      });
      setMessage('反馈已提交，管理员将会尽快处理。');
      setPhase('done');
    } catch (submitError) {
      setError(submitError.message || '提交失败，请稍后重试。');
      setPhase('idle');
    }
  }

  const busy = phase === 'submitting';

  return (
    <PublicPageShell>
      <div className="public-hero">
        <div className="public-hero-icon">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <path d="M9 20l-5.45-2.73a1 1 0 0 1-.55-.9V4.38a1 1 0 0 1 1.45-.9L9 6m0 14V6m0 14l6-3m0-11L9 6m6 11l4.55 2.27a1 1 0 0 0 1.45-.9V5.18a1 1 0 0 0-.55-.9L15 2M15 17V2m0 15l-6-3" />
          </svg>
        </div>
        <h1>地图反馈</h1>
        <p>反馈地图缺失、地图异常或需要添加的地图，管理员会根据反馈内容进行处理。</p>
      </div>

      <div style={{ maxWidth: 620, margin: '0 auto' }}>
        <div className="public-card">
          <div className="public-card-body">
            {/* 反馈类型选择 */}
            <div className="form-group">
              <label>反馈类型 <span className="text-accent">*</span></label>
              <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
                {FEEDBACK_TYPES.map((t) => (
                  <button
                    key={t.value}
                    type="button"
                    className={`feedback-type-card ${feedbackType === t.value ? 'active' : ''}`}
                    onClick={() => !busy && setFeedbackType(t.value)}
                    disabled={busy}
                  >
                    <span className="feedback-type-icon">{t.icon}</span>
                    <div className="feedback-type-body">
                      <div className="feedback-type-label">{t.label}</div>
                      <div className="feedback-type-desc">{t.desc}</div>
                    </div>
                  </button>
                ))}
              </div>
            </div>

            {/* 历史反馈记录 */}
            {loadingFeedback && (
              <div style={{ color: 'var(--text3)', fontSize: 13, padding: '8px 0 16px', textAlign: 'center' }}>
                正在查询历史反馈记录...
              </div>
            )}
            {existingFeedback !== null && !loadingFeedback && existingFeedback.length > 0 && (
              <div className="form-section-card mb-16">
                <div className="form-section-header">
                  <span>历史反馈</span>
                </div>
                <div style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
                  {existingFeedback.map((fb) => {
                    const st = STATUS_MAP[fb.status] || { label: fb.status, icon: '📋', color: 'var(--text2)', bg: 'var(--surface2)' };
                    const typeLabel = FEEDBACK_TYPES.find((t) => t.value === fb.feedback_type)?.label || fb.feedback_type;
                    return (
                      <div key={fb.id} style={{ border: '1px solid var(--border)', borderRadius: 10, overflow: 'hidden' }}>
                        <div style={{
                          display: 'flex', alignItems: 'center', justifyContent: 'space-between',
                          padding: '10px 14px', background: st.bg, borderBottom: '1px solid var(--border)',
                        }}>
                          <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                            <span style={{ fontSize: 16 }}>{st.icon}</span>
                            <span style={{ fontWeight: 600, fontSize: 13, color: st.color }}>{typeLabel} · {st.label}</span>
                          </div>
                          <span style={{ fontSize: 11, color: 'var(--text3)' }}>
                            {formatChinaDateTime(fb.created_at, { seconds: false })}
                          </span>
                        </div>
                        <div style={{ padding: '12px 14px', fontSize: 13 }}>
                          <div style={{ whiteSpace: 'pre-wrap', wordBreak: 'break-word', marginBottom: 8 }}>{fb.detail}</div>
                          {fb.review_note && (
                            <div style={{
                              fontSize: 13, padding: '8px 10px', borderRadius: 6,
                              background: fb.status === 'resolved' ? 'var(--success-bg, #dcfce7)' : 'var(--danger-bg, #fee2e2)',
                              border: `1px solid ${fb.status === 'resolved' ? 'var(--success-text, #22c55e)' : 'var(--danger-text, #ef4444)'}33`,
                            }}>
                              <span style={{ fontSize: 11, color: 'var(--text3)' }}>管理员回复：</span>
                              <span style={{ whiteSpace: 'pre-wrap', wordBreak: 'break-word' }}>{fb.review_note}</span>
                            </div>
                          )}
                        </div>
                      </div>
                    );
                  })}
                </div>
              </div>
            )}

            {/* Steam 标识符（可选） */}
            <div className="form-group">
              <label>Steam 标识符</label>
              <input
                className="form-control"
                value={steamInput}
                onChange={(e) => setSteamInput(e.target.value)}
                onBlur={handleSteamBlur}
                disabled={busy}
                placeholder="可选，SteamID64 / SteamID / 个人主页链接"
              />
              <div className="form-hint">填写后可查询您的历史反馈记录</div>
            </div>

            {/* 联系方式（可选） */}
            <div className="form-group">
              <label>联系方式</label>
              <input
                className="form-control"
                value={contact}
                onChange={(e) => setContact(e.target.value)}
                disabled={busy}
                placeholder="可选，便于管理员补充核实（QQ / Discord / 邮箱等）"
              />
            </div>

            {/* 详细内容（必填） */}
            <div className="form-group">
              <label>详细内容 <span className="text-accent">*</span></label>
              <textarea
                className="form-control"
                value={detail}
                onChange={(e) => setDetail(e.target.value)}
                rows={6}
                disabled={busy}
                placeholder={selectedType ? selectedType.placeholder : '请先选择反馈类型'}
              />
            </div>

            {error ? <div className="alert alert-error"><span className="alert-icon">✕</span><div className="alert-content"><div className="alert-title">提交失败</div><div className="alert-text">{error}</div></div></div> : null}
            {message ? <div className="alert alert-success"><span className="alert-icon">✓</span><div className="alert-content"><div className="alert-title">反馈已提交</div><div className="alert-text">{message}</div></div></div> : null}

            {phase !== 'done' ? (
              <button
                className="btn btn-accent"
                type="button"
                style={{ width: '100%', padding: 12 }}
                disabled={busy || !feedbackType || !detail.trim()}
                onClick={handleSubmit}
              >
                {busy ? '正在提交...' : '提交反馈'}
              </button>
            ) : null}
          </div>
        </div>
      </div>
    </PublicPageShell>
  );
}

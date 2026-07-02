import { useState } from 'react';
import { publicApi } from '../../lib/publicApi.js';
import { Modal } from '../../shared/Modal.jsx';
import { PublicPageShell } from './PublicPageShell.jsx';

export function PublicApplyPage() {
  const [steamInput, setSteamInput] = useState('');
  const [nickname, setNickname] = useState('');
  const [contact, setContact] = useState('');
  const [contactPromptOpen, setContactPromptOpen] = useState(false);
  const [contactPromptValue, setContactPromptValue] = useState('');
  const [message, setMessage] = useState('');
  const [error, setError] = useState('');
  const [submitting, setSubmitting] = useState(false);
  const [resolving, setResolving] = useState(false);
  const [resolveError, setResolveError] = useState('');

  async function handleSteamBlur() {
    if (!steamInput.trim()) return;
    setResolving(true);
    setResolveError('');
    try {
      const result = await publicApi.resolveSteam({ steam_input: steamInput.trim() });
      if (result.persona_name) {
        setNickname(result.persona_name);
      } else {
        setResolveError('未能自动获取 Steam 名称，请手动填写您的游戏昵称。');
      }
    } catch {
      setResolveError('无法获取 Steam 资料，请手动填写游戏昵称。');
    } finally {
      setResolving(false);
    }
  }

  function handleSteamChange(value) {
    setSteamInput(value);
    setResolveError('');
  }

  const submit = async (options = {}) => {
    const contactValue = options.contactValue ?? contact;
    if (!steamInput.trim()) { setError('请输入 Steam 标识符。'); return; }
    if (!nickname.trim()) { setError('请输入游戏昵称。'); return; }
    if (!options.allowEmptyContact && !contactValue.trim()) {
      setContactPromptValue(contact);
      setContactPromptOpen(true);
      return;
    }
    try {
      setSubmitting(true);
      setError('');
      setMessage('');
      await publicApi.submitWhitelist({
        steam_input: steamInput.trim(),
        nickname: nickname.trim(),
        contact: contactValue.trim() || undefined,
      });
      setMessage('申请已提交，请等待管理员审核。');
      setSteamInput('');
      setNickname('');
      setContact('');
      setContactPromptValue('');
      setContactPromptOpen(false);
      setResolveError('');
    } catch (submitError) {
      setError(submitError.message);
    } finally {
      setSubmitting(false);
    }
  };

  const submitWithPromptContact = () => {
    setContact(contactPromptValue);
    setContactPromptOpen(false);
    submit({ allowEmptyContact: true, contactValue: contactPromptValue });
  };

  const submitWithoutContact = () => {
    setContact('');
    setContactPromptValue('');
    setContactPromptOpen(false);
    submit({ allowEmptyContact: true, contactValue: '' });
  };

  function getErrorType(msg) {
    if (msg.includes('已通过')) return 'success';
    if (msg.includes('审核中')) return 'warning';
    return 'error';
  }

  function renderFeedback() {
    if (error) {
      const type = getErrorType(error);
      if (type === 'success') return (
        <div className="alert alert-success">
          <span className="alert-icon">✓</span>
          <div className="alert-content">
            <div className="alert-title">白名单已通过</div>
            <div className="alert-text">{error}</div>
          </div>
        </div>
      );
      if (type === 'warning') return (
        <div className="alert alert-warning">
          <span className="alert-icon">⏳</span>
          <div className="alert-content">
            <div className="alert-title">审核中</div>
            <div className="alert-text">{error}</div>
          </div>
        </div>
      );
      return (
        <div className="alert alert-error">
          <span className="alert-icon">✕</span>
          <span className="alert-text">{error}</span>
        </div>
      );
    }
    if (message) return (
      <div className="alert alert-success">
        <span className="alert-icon">✓</span>
        <div className="alert-content">
          <div className="alert-title">申请提交成功</div>
          <div className="alert-text">请等待管理员审核，审核通过后即可进入服务器。</div>
        </div>
      </div>
    );
    return null;
  }

  return (
    <PublicPageShell>
      <div className="public-hero">
        <div className="public-hero-icon">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <path d="M9 11l3 3L22 4" /><path d="M21 12v7a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11" />
          </svg>
        </div>
        <h1>白名单申请</h1>
        <p>填写您的 Steam 信息提交申请，管理员审核通过后即可加入服务器。</p>
      </div>

      <div style={{ maxWidth: 480, margin: '0 auto' }}>
        <div className="public-card">
          <div className="public-card-body">
            <div className="form-group">
              <label>Steam 标识符 <span className="text-accent">*</span></label>
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
                {resolving && <span className="form-hint-loading">正在获取 Steam 资料...</span>}
              </div>
            </div>
            <div className="form-group">
              <label>游戏昵称 <span className="text-accent">*</span></label>
              <input
                type="text"
                className="form-control"
                value={nickname}
                onChange={(e) => setNickname(e.target.value)}
                placeholder="您的 Steam 名称"
                disabled={submitting}
              />
              <div className="form-hint">
                {resolveError
                  ? <span style={{ color: 'var(--warning-text)' }}>{resolveError}</span>
                  : '输入 Steam 标识符后将自动获取昵称'}
              </div>
            </div>
            <div className="form-group">
              <label>联系方式</label>
              <input
                type="text"
                className="form-control"
                value={contact}
                onChange={(e) => setContact(e.target.value)}
                placeholder="QQ / 微信 / 邮箱等"
                disabled={submitting}
              />
              <div className="form-hint">非必填，但建议填写，方便审核员后续与您联系。</div>
            </div>

            {renderFeedback()}

            <button
              className="btn btn-accent"
              style={{ width: '100%', padding: 12, fontSize: 14, marginTop: 6 }}
              type="button"
              onClick={() => submit()}
              disabled={submitting || resolving}
            >
              {submitting ? '提交中...' : '提交白名单申请'}
            </button>
          </div>
        </div>

        <div style={{ textAlign: 'center', marginTop: 16, fontSize: 12, color: 'var(--text3)' }}>
          提交后可在「白名单公示」页查看审核状态
        </div>
      </div>

      <Modal
        open={contactPromptOpen}
        title="建议填写联系方式"
        onClose={() => setContactPromptOpen(false)}
        footer={
          <>
            <button className="btn btn-outline" type="button" onClick={submitWithoutContact} disabled={submitting}>不填写，继续提交</button>
            <button className="btn btn-primary" type="button" onClick={submitWithPromptContact} disabled={submitting || !contactPromptValue.trim()}>{submitting ? '提交中...' : '填写并提交'}</button>
          </>
        }
      >
        <div className="alert alert-warning">
          <span className="alert-icon">!</span>
          <div className="alert-content">
            <div className="alert-title">强烈建议您填写联系方式</div>
            <div className="alert-text">QQ / 微信 / 邮箱等联系方式可以帮助管理员在审核时与您确认信息。不填写也可以继续提交申请。</div>
          </div>
        </div>
        <div className="form-group">
          <label>联系方式</label>
          <input
            type="text"
            className="form-control"
            value={contactPromptValue}
            onChange={(e) => setContactPromptValue(e.target.value)}
            placeholder="QQ / 微信 / 邮箱等"
            disabled={submitting}
            autoFocus
          />
        </div>
      </Modal>
    </PublicPageShell>
  );
}

import { useState, useEffect, useCallback, useRef } from 'react';
import { publicApi } from '../../lib/publicApi.js';
import { PublicPageShell } from './PublicPageShell.jsx';

const STEAM_AUTH_ERROR_REASONS = {
  invalid_mode: 'Steam 登录响应无效，请重试。',
  invalid_claimed_id: '无法从 Steam 获取身份信息，请重试。',
  missing_claimed_id: 'Steam 未返回身份信息，请重试。',
  verification_failed: 'Steam 身份验证失败，请重试。',
  invalid_state: 'Steam 登录会话已过期，请重试。',
  missing_token: 'Steam 登录信息缺失，请重试。',
};

const QQ_POLL_INTERVAL_MS = 3000;

export function PublicApplyPage() {
  // 首次渲染时一次性解析 Steam 回调 URL 参数（steam_token / steam_auth / reason）。
  const [steamCallback] = useState(() => {
    const params = new URLSearchParams(window.location.search);
    return {
      token: params.get('steam_token'),
      cancelled: params.get('steam_auth'),
      errorReason: params.get('reason'),
    };
  });

  const steamAuthError = steamCallback.cancelled === 'cancelled'
    ? '您取消了 Steam 登录。'
    : steamCallback.cancelled === 'error'
      ? (STEAM_AUTH_ERROR_REASONS[steamCallback.errorReason] || 'Steam 登录失败，请重试。')
      : '';

  // Steam 认证状态
  const [steamVerified, setSteamVerified] = useState(false);
  const [steamToken, setSteamToken] = useState('');
  const [steamInfo, setSteamInfo] = useState(null);
  // 手动输入模式（当没有 Steam 认证时）
  const [manualMode, setManualMode] = useState(false);
  const [steamInput, setSteamInput] = useState('');
  const [nickname, setNickname] = useState('');
  const [resolving, setResolving] = useState(false);
  const [resolveError, setResolveError] = useState('');
  const [authLoading, setAuthLoading] = useState(() => Boolean(steamCallback.token));
  const [authError, setAuthError] = useState(steamAuthError);

  // Steam 身份确认后进入第二步所需的稳定 Steam 标识
  const [confirmedSteamId, setConfirmedSteamId] = useState('');

  // 第二步：QQ 群绑定
  const [qqCode, setQqCode] = useState(null); // { code, expires_at, ttl_seconds, group_number, group_link }
  const [qqBound, setQqBound] = useState(false);
  const [qqUserName, setQqUserName] = useState('');
  const [qqLoading, setQqLoading] = useState(false);
  const [qqError, setQqError] = useState('');
  const [codeRemaining, setCodeRemaining] = useState(0);
  const [copied, setCopied] = useState(false);

  // 第三步：申请理由
  const [reason, setReason] = useState('');

  // 提交状态
  const [message, setMessage] = useState('');
  const [error, setError] = useState('');
  const [submitting, setSubmitting] = useState(false);

  // 记录当前步骤：1 Steam 验证，2 QQ 绑定，3 填写理由
  const step = !confirmedSteamId ? 1 : (!qqBound ? 2 : 3);

  // ——————————————————————————————————————————————————————————————
  // Steam 回调：获取已验证会话
  // ——————————————————————————————————————————————————————————————
  useEffect(() => {
    const url = new URL(window.location);
    if (
      url.searchParams.has('steam_token') ||
      url.searchParams.has('steam_auth') ||
      url.searchParams.has('reason')
    ) {
      url.searchParams.delete('steam_token');
      url.searchParams.delete('steam_auth');
      url.searchParams.delete('reason');
      window.history.replaceState({}, '', url);
    }

    if (!steamCallback.token) return;

    publicApi
      .getSteamSession(steamCallback.token)
      .then((data) => {
        setSteamVerified(true);
        setSteamToken(data.token);
        setSteamInfo({
          steamid64: data.steamid64,
          steamid: data.steamid,
          steamid3: data.steamid3,
          profileUrl: data.profile_url,
          personaName: data.persona_name,
        });
        setNickname(data.persona_name || '');
        setConfirmedSteamId(data.steamid64);
        setAuthLoading(false);
      })
      .catch((err) => {
        setAuthError(err.message || 'Steam 会话验证失败');
        setAuthLoading(false);
      });
  }, [steamCallback]);

  // ——————————————————————————————————————————————————————————————
  // Steam 登录按钮
  // ——————————————————————————————————————————————————————————————
  const handleSteamLogin = useCallback(() => {
    setAuthError('');
    publicApi
      .getSteamLoginInfo()
      .then((loginInfo) => {
        window.location.href = loginInfo.login_url;
      })
      .catch((err) => {
        setAuthError('无法获取 Steam 登录配置：' + (err.message || '请稍后重试'));
      });
  }, []);

  // ——————————————————————————————————————————————————————————————
  // 手动输入：Steam 标识解析
  // ——————————————————————————————————————————————————————————————
  async function handleManualConfirm() {
    if (!steamInput.trim()) { setResolveError('请输入 Steam 标识符。'); return; }
    setResolving(true);
    setResolveError('');
    try {
      const result = await publicApi.resolveSteam({ steam_input: steamInput.trim() });
      if (result.persona_name) setNickname(result.persona_name);
      setConfirmedSteamId(result.steamid64);
      if (!steamInfo) {
        setSteamInfo({
          steamid64: result.steamid64,
          steamid: result.steamid,
          steamid3: result.steamid3,
          profileUrl: result.profile_url,
          personaName: result.persona_name,
        });
      }
    } catch (err) {
      setResolveError(err.message || '无法解析 Steam 标识，请检查后重试。');
    } finally {
      setResolving(false);
    }
  }

  function handleSteamChange(value) {
    setSteamInput(value);
    setResolveError('');
  }

  // ——————————————————————————————————————————————————————————————
  // 第二步：生成验证码 + 轮询绑定状态
  // ——————————————————————————————————————————————————————————————
  const refreshBindStatus = useCallback(async () => {
    if (!confirmedSteamId) return null;
    try {
      const data = await publicApi.qqBindStatus(confirmedSteamId);
      if (data.bound) {
        setQqBound(true);
        setQqUserName(data.qq_username || '');
        if (data.group_number) {
          setQqCode((prev) => prev || { group_number: data.group_number, group_link: data.group_link });
        }
      }
      return data;
    } catch {
      // 静默失败，下一轮再试
      return null;
    }
  }, [confirmedSteamId]);

  const issueCode = useCallback(async () => {
    if (!confirmedSteamId) return;
    setQqLoading(true);
    setQqError('');
    setCopied(false);
    try {
      const body = steamVerified ? { steam_token: steamToken } : { steam_input: confirmedSteamId };
      const data = await publicApi.issueQqCode(body);
      setQqCode(data);
      setCodeRemaining(data.ttl_seconds || 300);
    } catch (err) {
      // 已绑定时会返回提示，直接刷新状态
      setQqError(err.message || '生成验证码失败，请重试。');
      await refreshBindStatus();
    } finally {
      setQqLoading(false);
    }
  }, [confirmedSteamId, steamVerified, steamToken, refreshBindStatus]);

  // 进入第二步时自动生成验证码（每次进入只自动尝试一次，失败由用户手动重试）
  const autoIssueRef = useRef(false);
  useEffect(() => {
    if (step !== 2 || qqBound) return;
    if (autoIssueRef.current) return;
    autoIssueRef.current = true;
    refreshBindStatus().then((data) => {
      if (!data?.bound) issueCode();
    });
  }, [step, qqBound, refreshBindStatus, issueCode]);

  // 验证码倒计时
  useEffect(() => {
    if (step !== 2 || qqBound || codeRemaining <= 0) return undefined;
    const timer = window.setTimeout(() => setCodeRemaining((prev) => Math.max(0, prev - 1)), 1000);
    return () => window.clearTimeout(timer);
  }, [step, qqBound, codeRemaining]);

  // 轮询绑定状态
  const qqBoundRef = useRef(false);
  useEffect(() => { qqBoundRef.current = qqBound; }, [qqBound]);
  useEffect(() => {
    if (step !== 2 || qqBound) return undefined;
    const timer = window.setInterval(async () => {
      if (qqBoundRef.current) return;
      const data = await refreshBindStatus();
      if (data?.bound) window.clearInterval(timer);
    }, QQ_POLL_INTERVAL_MS);
    return () => window.clearInterval(timer);
  }, [step, qqBound, refreshBindStatus]);

  async function copyCode() {
    if (!qqCode?.code) return;
    try {
      await navigator.clipboard.writeText(qqCode.code);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 2000);
    } catch {
      setQqError('复制失败，请手动选中验证码复制。');
    }
  }

  // ——————————————————————————————————————————————————————————————
  // 第三步：提交白名单
  // ——————————————————————————————————————————————————————————————
  const submit = async () => {
    if (!reason.trim()) { setError('请填写申请理由。'); return; }
    setSubmitting(true);
    setError('');
    setMessage('');
    try {
      await publicApi.submitWhitelist({
        ...(steamVerified ? { steam_token: steamToken } : { steam_input: confirmedSteamId }),
        nickname: nickname.trim() || undefined,
        reason: reason.trim(),
      });
      setMessage('申请已提交，请等待管理员审核。');
      setReason('');
    } catch (submitError) {
      setError(submitError.message);
    } finally {
      setSubmitting(false);
    }
  };

  function handleLogout() {
    setSteamVerified(false);
    setSteamToken('');
    setSteamInfo(null);
    setManualMode(false);
    setSteamInput('');
    setNickname('');
    setConfirmedSteamId('');
    setQqCode(null);
    setQqBound(false);
    setQqUserName('');
    setReason('');
    setMessage('');
    setError('');
    setAuthError('');
    setQqError('');
  }

  // ——————————————————————————————————————————————————————————————
  // 渲染辅助
  // ——————————————————————————————————————————————————————————————
  function renderFeedback() {
    if (error) {
      const type = error.includes('已通过') ? 'success' : error.includes('审核中') ? 'warning' : 'error';
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

  function formatRemaining(seconds) {
    const m = Math.floor(seconds / 60);
    const s = seconds % 60;
    return `${m}:${String(s).padStart(2, '0')}`;
  }

  // 步骤指示器
  function renderSteps() {
    const steps = [
      { n: 1, label: 'Steam 验证' },
      { n: 2, label: '加入 QQ 群验证' },
      { n: 3, label: '填写理由' },
    ];
    return (
      <div style={{ display: 'flex', justifyContent: 'center', gap: 8, marginBottom: 16, flexWrap: 'wrap' }}>
        {steps.map((s, i) => {
          const active = step === s.n;
          const done = step > s.n;
          return (
            <div key={s.n} style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
              <div style={{
                display: 'flex', alignItems: 'center', gap: 6,
                padding: '4px 10px', borderRadius: 999,
                background: done ? 'var(--success-bg, rgba(34,197,94,.15))' : active ? 'var(--surface2)' : 'transparent',
                border: '1px solid var(--border)',
                opacity: done || active ? 1 : 0.5,
              }}>
                <span style={{
                  width: 18, height: 18, borderRadius: '50%', fontSize: 11,
                  display: 'inline-flex', alignItems: 'center', justifyContent: 'center',
                  background: done ? '#22c55e' : active ? 'var(--accent-color)' : 'var(--surface3, #333)',
                  color: '#fff',
                }}>{done ? '✓' : s.n}</span>
                <span style={{ fontSize: 12, fontWeight: active ? 600 : 400 }}>{s.label}</span>
              </div>
              {i < steps.length - 1 && <span style={{ color: 'var(--text4)', fontSize: 12 }}>→</span>}
            </div>
          );
        })}
      </div>
    );
  }

  if (authLoading) {
    return (
      <PublicPageShell>
        <div className="public-hero">
          <div className="public-hero-icon">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <path d="M9 11l3 3L22 4" /><path d="M21 12v7a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11" />
            </svg>
          </div>
          <h1>白名单申请</h1>
          <p>正在验证 Steam 身份...</p>
        </div>
      </PublicPageShell>
    );
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
        <p>两步验证：先验证 Steam 身份，再加入 QQ 群完成验证，最后填写申请理由。</p>
      </div>

      <div style={{ maxWidth: 520, margin: '0 auto' }}>
        {renderSteps()}
        <div className="public-card">
          <div className="public-card-body">
            {/* ————————————————— Step 1: Steam 验证 ————————————————— */}
            {step === 1 && (
              <>
                <div style={{ textAlign: 'center', padding: '8px 0 16px' }}>
                  <div style={{
                    width: 64, height: 64, margin: '0 auto 16px',
                    background: 'var(--surface2)', borderRadius: '50%',
                    display: 'flex', alignItems: 'center', justifyContent: 'center',
                  }}>
                    <svg width="32" height="32" viewBox="0 0 24 24" fill="currentColor" style={{ color: 'var(--text2)' }}>
                      <path d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm-2 15l-5-5 1.41-1.41L10 14.17l7.59-7.59L19 8l-9 9z"/>
                    </svg>
                  </div>
                  <h3 style={{ margin: '0 0 8px', fontSize: 16 }}>第一步：验证 Steam 身份</h3>
                  <p style={{ margin: '0 0 16px', fontSize: 13, color: 'var(--text3)' }}>
                    通过 Steam 官方登录验证，或手动填写 Steam 标识符。
                  </p>

                  {authError && (
                    <div className="alert alert-error" style={{ marginBottom: 12 }}>
                      <span className="alert-icon">✕</span>
                      <span className="alert-text">{authError}</span>
                    </div>
                  )}

                  {!manualMode ? (
                    <>
                      <button
                        className="btn btn-accent"
                        style={{ padding: '12px 32px', fontSize: 14, gap: 8, display: 'inline-flex', alignItems: 'center' }}
                        type="button"
                        onClick={handleSteamLogin}
                      >
                        <svg width="20" height="20" viewBox="0 0 24 24" fill="currentColor">
                          <path d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm-2 15l-5-5 1.41-1.41L10 14.17l7.59-7.59L19 8l-9 9z"/>
                        </svg>
                        通过 Steam 登录验证
                      </button>
                      <div style={{ marginTop: 16 }}>
                        <button className="btn btn-outline" style={{ fontSize: 12 }} type="button" onClick={() => setManualMode(true)}>
                          无法使用 Steam 登录？手动输入 Steam 标识符
                        </button>
                      </div>
                      <p style={{ marginTop: 12, fontSize: 11, color: 'var(--text4)' }}>
                        点击上方按钮将跳转到 Steam 官方页面进行登录，<br />登录后自动返回本页面。
                      </p>
                    </>
                  ) : (
                    <>
                      <div className="form-group" style={{ textAlign: 'left' }}>
                        <label>Steam 标识符 <span className="text-accent">*</span></label>
                        <input
                          type="text"
                          className="form-control"
                          value={steamInput}
                          onChange={(e) => handleSteamChange(e.target.value)}
                          placeholder="SteamID64 / SteamID / 个人主页链接"
                          disabled={resolving}
                        />
                        <div className="form-hint">
                          支持 SteamID64、Steam2、Steam3 和 Steam 个人主页链接
                          {resolving && <span className="form-hint-loading">正在获取 Steam 资料...</span>}
                        </div>
                        {resolveError && <div className="form-hint" style={{ color: 'var(--warning-text)' }}>{resolveError}</div>}
                      </div>
                      <div className="form-group" style={{ textAlign: 'left' }}>
                        <label>游戏昵称</label>
                        <input
                          type="text"
                          className="form-control"
                          value={nickname}
                          onChange={(e) => setNickname(e.target.value)}
                          placeholder="输入 Steam 标识符后自动获取，可手动修改"
                        />
                      </div>
                      <button
                        className="btn btn-accent"
                        style={{ width: '100%', padding: 12, fontSize: 14 }}
                        type="button"
                        onClick={handleManualConfirm}
                        disabled={resolving}
                      >
                        {resolving ? '验证中...' : '确认 Steam 信息，下一步'}
                      </button>
                      <div style={{ marginTop: 12 }}>
                        <button className="btn btn-outline" style={{ fontSize: 12 }} type="button" onClick={() => setManualMode(false)}>
                          返回 Steam 登录
                        </button>
                      </div>
                    </>
                  )}
                </div>
              </>
            )}

            {/* ————————————————— Step 2: QQ 群绑定 ————————————————— */}
            {step === 2 && (
              <>
                <div className="alert alert-success" style={{ marginBottom: 16 }}>
                  <span className="alert-icon">✓</span>
                  <div className="alert-content">
                    <div className="alert-title">Steam 身份已确认</div>
                    <div className="alert-text">
                      <div style={{ marginTop: 4 }}><strong>SteamID64:</strong> {confirmedSteamId}</div>
                      {nickname && <div><strong>昵称:</strong> {nickname}</div>}
                      {!steamVerified && <div style={{ color: 'var(--warning-text)' }}>手动填写（未通过 Steam 登录验证）</div>}
                    </div>
                  </div>
                </div>

                <h3 style={{ margin: '0 0 8px', fontSize: 16, textAlign: 'center' }}>第二步：加入 QQ 群并发送验证码</h3>
                <p style={{ margin: '0 0 16px', fontSize: 13, color: 'var(--text3)', textAlign: 'center' }}>
                  加入 QQ 群后，在群内 <strong>@机器人</strong> 发送下方验证码即可完成绑定。
                </p>

                {qqError && (
                  <div className="alert alert-error" style={{ marginBottom: 12 }}>
                    <span className="alert-icon">✕</span>
                    <span className="alert-text">{qqError}</span>
                  </div>
                )}

                {qqCode?.group_link && (
                  <a
                    className="btn btn-accent"
                    style={{ width: '100%', padding: 12, fontSize: 14, display: 'block', textAlign: 'center', textDecoration: 'none', marginBottom: 12 }}
                    href={qqCode.group_link}
                    target="_blank"
                    rel="noopener noreferrer"
                  >
                    一键加入 QQ 群（{qqCode.group_number}）
                  </a>
                )}
                {qqCode && !qqCode.group_link && qqCode.group_number && (
                  <div style={{ textAlign: 'center', marginBottom: 12, fontSize: 13, color: 'var(--text2)' }}>
                    请手动搜索并加入 QQ 群：<strong>{qqCode.group_number}</strong>
                  </div>
                )}

                {qqCode?.code ? (
                  <>
                    <div style={{
                      background: 'var(--surface2)', border: '1px solid var(--border)',
                      borderRadius: 10, padding: 20, textAlign: 'center', marginBottom: 8,
                    }}>
                      <div style={{ fontSize: 12, color: 'var(--text3)', marginBottom: 8 }}>您的验证码</div>
                      <div style={{ fontSize: 28, fontWeight: 700, letterSpacing: 4, fontFamily: 'var(--mono)' }}>
                        {qqCode.code}
                      </div>
                      <div style={{ fontSize: 12, color: codeRemaining > 0 ? 'var(--text3)' : 'var(--danger-text)', marginTop: 8 }}>
                        {codeRemaining > 0 ? `剩余有效时间 ${formatRemaining(codeRemaining)}（过期请重新生成）` : '验证码已过期'}
                      </div>
                    </div>
                    <button
                      className="btn btn-outline"
                      style={{ width: '100%', padding: 10, fontSize: 13, marginBottom: 8 }}
                      type="button"
                      onClick={copyCode}
                    >
                      {copied ? '已复制 ✓' : '复制验证码'}
                    </button>
                    <div style={{ fontSize: 12, color: 'var(--text4)', textAlign: 'center', marginBottom: 8 }}>
                      在 QQ 群内发送：<code>绑定 {qqCode.code}</code>（需 @机器人）
                    </div>
                    {(codeRemaining <= 0 || qqError) && (
                      <button
                        className="btn btn-accent"
                        style={{ width: '100%', padding: 10, fontSize: 13 }}
                        type="button"
                        onClick={issueCode}
                        disabled={qqLoading}
                      >
                        {qqLoading ? '生成中...' : '重新生成验证码'}
                      </button>
                    )}
                  </>
                ) : (
                  <div style={{ textAlign: 'center', padding: 16 }}>
                    <div className="public-loading-spinner" style={{ margin: '0 auto 12px' }} />
                    <div style={{ fontSize: 13, color: 'var(--text3)' }}>{qqLoading ? '正在生成验证码...' : '正在检查绑定状态...'}</div>
                    <button className="btn btn-outline" style={{ marginTop: 12, fontSize: 12 }} type="button" onClick={issueCode} disabled={qqLoading}>
                      手动生成验证码
                    </button>
                  </div>
                )}

                <div style={{ marginTop: 16, paddingTop: 12, borderTop: '1px dashed var(--border)' }}>
                  <div style={{ display: 'flex', alignItems: 'center', gap: 8, fontSize: 12, color: 'var(--text3)' }}>
                    <span className="public-loading-spinner" style={{ width: 12, height: 12 }} />
                    正在等待您完成群内验证，完成后会自动进入下一步...
                  </div>
                  <button className="btn btn-outline" style={{ marginTop: 12, fontSize: 12 }} type="button" onClick={refreshBindStatus}>
                    我已发送，立即检查
                  </button>
                </div>

                <div style={{ marginTop: 12, textAlign: 'center' }}>
                  <button className="btn btn-outline" style={{ fontSize: 12 }} type="button" onClick={handleLogout}>
                    返回第一步
                  </button>
                </div>
              </>
            )}

            {/* ————————————————— Step 3: 填写理由 & 提交 ————————————————— */}
            {step === 3 && (
              <>
                <div className="alert alert-success" style={{ marginBottom: 16 }}>
                  <span className="alert-icon">✓</span>
                  <div className="alert-content">
                    <div className="alert-title">两步验证已完成</div>
                    <div className="alert-text">
                      <div style={{ marginTop: 4 }}><strong>SteamID64:</strong> {confirmedSteamId}</div>
                      {nickname && <div><strong>昵称:</strong> {nickname}</div>}
                      <div><strong>QQ 绑定:</strong> 已绑定{qqUserName ? `（${qqUserName}）` : ''}</div>
                    </div>
                  </div>
                </div>

                <h3 style={{ margin: '0 0 12px', fontSize: 16 }}>第三步：填写申请理由</h3>

                <div className="form-group">
                  <label>申请理由 <span className="text-accent">*</span></label>
                  <textarea
                    className="form-control"
                    rows={4}
                    value={reason}
                    onChange={(e) => setReason(e.target.value)}
                    placeholder="请填写您申请白名单的理由，例如游戏时长、KZ 经历、加入社区的原因等"
                    disabled={submitting}
                  />
                  <div className="form-hint">必填，管理员将根据申请理由进行审核。</div>
                </div>

                {renderFeedback()}

                <button
                  className="btn btn-accent"
                  style={{ width: '100%', padding: 12, fontSize: 14, marginTop: 6 }}
                  type="button"
                  onClick={submit}
                  disabled={submitting}
                >
                  {submitting ? '提交中...' : '提交白名单申请'}
                </button>

                <div style={{ marginTop: 12, textAlign: 'center' }}>
                  <button className="btn btn-outline" style={{ fontSize: 12 }} type="button" onClick={handleLogout}>
                    重新开始申请
                  </button>
                </div>
              </>
            )}
          </div>
        </div>

        <div style={{ textAlign: 'center', marginTop: 16, fontSize: 12, color: 'var(--text3)' }}>
          提交后可在「白名单公示」页查看审核状态
        </div>
      </div>
    </PublicPageShell>
  );
}

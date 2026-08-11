import { useState, useEffect, useCallback } from 'react';
import { publicApi } from '../../lib/publicApi.js';
import { Modal } from '../../shared/Modal.jsx';
import { PublicPageShell } from './PublicPageShell.jsx';

const STEAM_OPENID_URL = 'https://steamcommunity.com/openid/login';

// Steam 回调错误码 → 用户提示文案
const STEAM_AUTH_ERROR_REASONS = {
  invalid_mode: 'Steam 登录响应无效，请重试。',
  invalid_claimed_id: '无法从 Steam 获取身份信息，请重试。',
  missing_claimed_id: 'Steam 未返回身份信息，请重试。',
  verification_failed: 'Steam 身份验证失败，请重试。',
};

function buildSteamLoginUrl(callbackUrl, realm) {
  // 使用后端配置的 callback_url 和 realm 构建 Steam OpenID 登录 URL
  const params = new URLSearchParams();
  params.set('openid.ns', 'http://specs.openid.net/auth/2.0');
  params.set('openid.mode', 'checkid_setup');
  params.set('openid.return_to', callbackUrl);
  params.set('openid.realm', realm);
  params.set('openid.identity', 'http://specs.openid.net/auth/2.0/identifier_select');
  params.set('openid.claimed_id', 'http://specs.openid.net/auth/2.0/identifier_select');
  return `${STEAM_OPENID_URL}?${params.toString()}`;
}

export function PublicApplyPage() {
  // 首次渲染时一次性解析 Steam 回调 URL 参数（steam_token / steam_auth / reason）。
  // 用惰性初始化读取，避免在 effect 中同步 setState（react-hooks/set-state-in-effect）。
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
  // 有 token 时初始为加载中（等待异步获取会话）；无 token / 回调失败时直接为就绪
  const [authLoading, setAuthLoading] = useState(() => Boolean(steamCallback.token));
  const [authError, setAuthError] = useState(steamAuthError);

  // 额外资料状态
  const [steamLevel, setSteamLevel] = useState(null);
  const [gokzStats, setGokzStats] = useState(null);
  const [statsLoading, setStatsLoading] = useState(false);

  // 手动输入模式（当没有 Steam 认证时）
  const [manualMode, setManualMode] = useState(false);
  const [steamInput, setSteamInput] = useState('');
  const [nickname, setNickname] = useState('');
  const [contact, setContact] = useState('');
  const [resolving, setResolving] = useState(false);
  const [resolveError, setResolveError] = useState('');

  // 提交状态
  const [contactPromptOpen, setContactPromptOpen] = useState(false);
  const [contactPromptValue, setContactPromptValue] = useState('');
  const [message, setMessage] = useState('');
  const [error, setError] = useState('');
  const [submitting, setSubmitting] = useState(false);

  // ——————————————————————————————————————————————————————————————
  // 初始化：检查 URL 中是否有 steam_token（Steam 回调返回）
  // URL 参数已在首次渲染时通过惰性初始化读取（不会在 effect 中同步 setState）；
  // 本 effect 只负责清理 URL 与异步获取 Steam 会话信息
  // ——————————————————————————————————————————————————————————————
  useEffect(() => {
    // 清理 URL 中的临时回调参数
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

    // 无 token：认证状态已在初始化时确定为非加载中
    if (!steamCallback.token) return;

    // 获取已验证的 Steam 会话信息
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
          setSteamLevel(data.steam_level ?? null);
          setNickname(data.persona_name || '');
          setAuthLoading(false);

          // 异步获取 GOKZ stats（不阻塞页面）
          if (data.steamid64) {
            setStatsLoading(true);
            publicApi
              .getGokzPlayerStatsBatch(data.steamid64)
              .then((stats) => {
                setGokzStats(stats);
                setStatsLoading(false);
              })
              .catch(() => {
                setGokzStats(null);
                setStatsLoading(false);
              });
          }
        })
        .catch((err) => {
          setAuthError(err.message || 'Steam 会话验证失败');
          setAuthLoading(false);
        });
  }, [steamCallback]);

  // ——————————————————————————————————————————————————————————————
  // 登录按钮点击
  // ——————————————————————————————————————————————————————————————
  const handleSteamLogin = useCallback(() => {
    setAuthError('');
    // 从后端获取配置的 callback_url 和 realm，确保与后端配置一致
    publicApi
      .getSteamLoginInfo()
      .then((loginInfo) => {
        const loginUrl = buildSteamLoginUrl(loginInfo.callback_url, loginInfo.realm);
        window.location.href = loginUrl;
      })
      .catch((err) => {
        setAuthError('无法获取 Steam 登录配置：' + (err.message || '请稍后重试'));
      });
  }, []);

  // ——————————————————————————————————————————————————————————————
  // 手动输入模式：Steam 标识符失焦时解析
  // ——————————————————————————————————————————————————————————————
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

  // ——————————————————————————————————————————————————————————————
  // 提交白名单
  // ——————————————————————————————————————————————————————————————
  const submit = async (options = {}) => {
    const contactValue = options.contactValue ?? contact;

    if (steamVerified) {
      // 通过 Steam OpenID 认证的提交
      if (!contactValue.trim() && !options.allowEmptyContact) {
        setContactPromptValue(contact);
        setContactPromptOpen(true);
        return;
      }

      try {
        setSubmitting(true);
        setError('');
        setMessage('');
        await publicApi.submitWhitelist({
          steam_token: steamToken,
          nickname: nickname.trim() || undefined,
          contact: contactValue.trim() || undefined,
        });
        setMessage('申请已提交，请等待管理员审核。');
        setContact('');
        setContactPromptValue('');
        setContactPromptOpen(false);
      } catch (submitError) {
        setError(submitError.message);
      } finally {
        setSubmitting(false);
      }
    } else {
      // 手动输入模式的提交
      if (!steamInput.trim()) { setError('请输入 Steam 标识符。'); return; }
      if (!nickname.trim()) { setError('请输入游戏昵称。'); return; }
      if (!contactValue.trim() && !options.allowEmptyContact) {
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

  // ——————————————————————————————————————————————————————————————
  // 退出登录
  // ——————————————————————————————————————————————————————————————
  function handleLogout() {
    setSteamVerified(false);
    setSteamToken('');
    setSteamInfo(null);
    setSteamLevel(null);
    setGokzStats(null);
    setNickname('');
    setContact('');
    setMessage('');
    setError('');
    setAuthError('');
  }

  // ——————————————————————————————————————————————————————————————
  // 渲染辅助
  // ——————————————————————————————————————————————————————————————
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

  // ——————————————————————————————————————————————————————————————
  // 加载状态
  // ——————————————————————————————————————————————————————————————
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

  // ——————————————————————————————————————————————————————————————
  // 主渲染
  // ——————————————————————————————————————————————————————————————
  return (
    <PublicPageShell>
      <div className="public-hero">
        <div className="public-hero-icon">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <path d="M9 11l3 3L22 4" /><path d="M21 12v7a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11" />
          </svg>
        </div>
        <h1>白名单申请</h1>
        <p>验证您的 Steam 身份并填写联系方式，管理员审核通过后即可加入服务器。</p>
      </div>

      <div style={{ maxWidth: 480, margin: '0 auto' }}>
        <div className="public-card">
          <div className="public-card-body">
            {!steamVerified && !manualMode ? (
              // ——————————————————————————————————————————————————————————
              // Step 1: Steam 登录验证
              // ——————————————————————————————————————————————————————————
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
                    通过 Steam 官方登录验证您的身份，我们将获取您的 SteamID64 和公开昵称。
                  </p>

                  {authError && (
                    <div className="alert alert-error" style={{ marginBottom: 12 }}>
                      <span className="alert-icon">✕</span>
                      <span className="alert-text">{authError}</span>
                    </div>
                  )}

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
                    <button
                      className="btn btn-outline"
                      style={{ fontSize: 12 }}
                      type="button"
                      onClick={() => setManualMode(true)}
                    >
                      无法使用 Steam 登录？手动输入 Steam 标识符
                    </button>
                  </div>

                  <p style={{ marginTop: 12, fontSize: 11, color: 'var(--text4)' }}>
                    点击上方按钮将跳转到 Steam 官方页面进行登录，<br />
                    登录后自动返回本页面。
                  </p>
                </div>
              </>
            ) : (
              // ——————————————————————————————————————————————————————————
              // Step 2: 填写信息 & 提交
              // ——————————————————————————————————————————————————————————
              <>
                {steamVerified && steamInfo && (
                  // 已验证 Steam 身份的信息卡片
                  <>
                  <div className="alert alert-success" style={{ marginBottom: 16 }}>
                    <span className="alert-icon">✓</span>
                    <div className="alert-content">
                      <div className="alert-title">Steam 身份已验证</div>
                      <div className="alert-text">
                        <div style={{ marginTop: 4 }}>
                          <strong>SteamID64:</strong> {steamInfo.steamid64}
                        </div>
                        {steamInfo.personaName && (
                          <div><strong>昵称:</strong> {steamInfo.personaName}</div>
                        )}
                        <div style={{ marginTop: 6 }}>
                          <button
                            className="btn btn-outline"
                            style={{ fontSize: 11, padding: '2px 10px' }}
                            type="button"
                            onClick={handleLogout}
                          >
                            退出登录
                          </button>
                        </div>
                      </div>
                    </div>
                  </div>

                  {/* Steam 等级 + KZ 统计面板 */}
                  {(steamLevel != null || gokzStats) && (
                    <div style={{
                      background: 'var(--surface2)',
                      border: '1px solid var(--border)',
                      borderRadius: 10,
                      padding: 16,
                      marginBottom: 16,
                    }}>
                      <div style={{
                        display: 'flex', alignItems: 'center', gap: 8,
                        marginBottom: 14, paddingBottom: 10,
                        borderBottom: '1px solid var(--border)',
                      }}>
                        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="var(--accent-color)" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                          <path d="M3 3v18h18"/><path d="M7 16l4-8 4 4 4-6"/>
                        </svg>
                        <span style={{ fontWeight: 600, fontSize: 14, color: 'var(--text1)' }}>账号数据</span>
                      </div>

                      <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(100px, 1fr))', gap: 10 }}>
                        {/* Steam 等级 */}
                        {steamLevel != null && (
                          <div style={{
                            background: 'var(--surface1)',
                            border: '1px solid var(--border)',
                            borderRadius: 8,
                            padding: '12px 12px 10px',
                            textAlign: 'center',
                          }}>
                            <div style={{ fontSize: 12, color: 'var(--text3)', marginBottom: 6 }}>等级</div>
                            <div style={{
                              fontSize: 22, fontWeight: 700, color: 'var(--accent-color)'
                            }}>
                              {steamLevel}
                            </div>
                          </div>
                        )}

                        {/* KZ 模式 */}
                        {['KZT', 'SKZ', 'VNL', 'OVR'].map((mode) => {
                          const data = gokzStats?.[mode];
                          const colorMap = { KZT: '#f59e0b', SKZ: '#3b82f6', VNL: '#10b981', OVR: '#ec4899' };
                          const modeColor = colorMap[mode] || 'var(--accent-color)';
                          if (!data) return null;
                          return (
                            <div
                              key={mode}
                              style={{
                                background: 'var(--surface1)',
                                border: '1px solid var(--border)',
                                borderTopColor: data.rating != null ? modeColor : 'var(--border)',
                                borderTopWidth: 3,
                                borderRadius: 8,
                                padding: '12px 12px 10px',
                                textAlign: 'center',
                              }}
                            >
                              <div style={{ fontSize: 12, fontWeight: 600, color: modeColor, marginBottom: 6 }}>{mode}</div>
                              {data.rating != null ? (
                                <>
                                  <div style={{ fontSize: 16, fontWeight: 700, color: 'var(--text1)' }}>
                                    {Number(data.rating).toFixed(1)}
                                  </div>
                                  <div style={{ fontSize: 11, color: 'var(--text4)', marginTop: 3 }}>
                                    Rating{data.rank != null ? `  ·  #${data.rank}` : ''}
                                  </div>
                                </>
                              ) : (
                                <div style={{ fontSize: 14, color: 'var(--text4)', padding: '4px 0' }}>暂无</div>
                              )}
                            </div>
                          );
                        })}
                      </div>

                      {statsLoading && (
                        <div style={{ textAlign: 'center', fontSize: 12, color: 'var(--text4)', marginTop: 10 }}>
                          正在加载 KZ 数据...
                        </div>
                      )}
                    </div>
                  )}
                  </>
                )}

                {manualMode && (
                  <div className="alert alert-warning" style={{ marginBottom: 16 }}>
                    <span className="alert-icon">!</span>
                    <div className="alert-content">
                      <div className="alert-title">手动输入模式</div>
                      <div className="alert-text">建议通过 Steam 登录验证以确保身份准确。</div>
                    </div>
                  </div>
                )}

                {/* 未通过 Steam OpenID 登录时，显示 Steam 输入框 */}
                {!steamVerified && manualMode && (
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
                    {resolveError && (
                      <div className="form-hint" style={{ color: 'var(--warning-text)' }}>
                        {resolveError}
                      </div>
                    )}
                  </div>
                )}

                {/* 昵称输入（手动模式时使用） */}
                {!steamVerified && manualMode && (
                  <div className="form-group">
                    <label>游戏昵称 <span className="text-accent">*</span></label>
                    <input
                      type="text"
                      className="form-control"
                      value={nickname}
                      onChange={(e) => setNickname(e.target.value)}
                      placeholder="您的游戏昵称"
                      disabled={submitting}
                    />
                    <div className="form-hint">
                      {steamInput.trim()
                        ? '输入 Steam 标识符后将自动获取昵称'
                        : '请输入 Steam 标识符以自动获取昵称'}
                    </div>
                  </div>
                )}

                {/* 联系方式 */}
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

                {manualMode && !steamVerified && (
                  <div style={{ marginTop: 12, textAlign: 'center' }}>
                    <button
                      className="btn btn-outline"
                      style={{ fontSize: 12 }}
                      type="button"
                      onClick={() => setManualMode(false)}
                    >
                      返回 Steam 登录
                    </button>
                  </div>
                )}
              </>
            )}
          </div>
        </div>

        <div style={{ textAlign: 'center', marginTop: 16, fontSize: 12, color: 'var(--text3)' }}>
          提交后可在「白名单公示」页查看审核状态
        </div>
      </div>

      {/* 联系方式提示弹窗 */}
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

import React, { useCallback, useRef, useState } from 'react';
import { api } from '../../lib/api.js';
import { PublicPageShell } from './PublicPageShell.jsx';
import { formatChinaDateTime } from '../../shared/time.js';

const MAX_FILE_SIZE = 100 * 1024 * 1024; // 100MB
const ALLOWED_VIDEO = '.mp4,.avi,.mov,.webm,.mkv';
const ALLOWED_IMAGE = '.jpg,.jpeg,.png,.gif,.webp,.bmp';
const ALLOWED_AUDIO = '.mp3,.wav,.ogg,.m4a,.flac';
const API_BASE = import.meta.env.VITE_API_BASE ?? '';

function formatDuration(minutes) {
  if (!minutes || minutes === 0) return '永久';
  if (minutes < 60) return `${minutes} 分钟`;
  if (minutes < 1440) return `${Math.floor(minutes / 60)} 小时`;
  return `${Math.floor(minutes / 1440)} 天`;
}

function formatFileSize(bytes) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

function fileCategoryIcon(cat) {
  if (cat === 'video') return '🎬';
  if (cat === 'image') return '🖼';
  if (cat === 'audio') return '🎵';
  return '📎';
}

const APPEAL_STATUS_MAP = {
  pending: { label: '待审核', icon: '⏳', color: 'var(--warning-text, #f59e0b)', bg: 'var(--warning-bg, #fef3c7)' },
  approved: { label: '已通过（已解封）', icon: '✅', color: 'var(--success-text, #22c55e)', bg: 'var(--success-bg, #dcfce7)' },
  rejected: { label: '已驳回', icon: '❌', color: 'var(--danger-text, #ef4444)', bg: 'var(--danger-bg, #fee2e2)' },
};

function getFileCategory(name) {
  const lower = name.toLowerCase();
  if (lower.match(/\.(mp4|avi|mov|webm|mkv)$/)) return 'video';
  if (lower.match(/\.(jpg|jpeg|png|gif|webp|bmp)$/)) return 'image';
  if (lower.match(/\.(mp3|wav|ogg|m4a|flac)$/)) return 'audio';
  return 'other';
}

function normalizeAppealCreateResponse(result) {
  const payload = result?.data ?? result ?? {};
  return {
    appealId: payload.item?.id ?? payload.appeal_id ?? payload.id,
    uploadToken: payload.item?.upload_token ?? payload.upload_token,
  };
}

function responseSummary(result) {
  if (!result || typeof result !== 'object') return typeof result;
  const keys = Object.keys(result);
  const itemKeys = result.item && typeof result.item === 'object' ? Object.keys(result.item) : [];
  const dataKeys = result.data && typeof result.data === 'object' ? Object.keys(result.data) : [];
  return `keys=${keys.join(',') || 'none'} itemKeys=${itemKeys.join(',') || 'none'} dataKeys=${dataKeys.join(',') || 'none'}`;
}

// 用 XMLHttpRequest 上传以便获取进度
function uploadWithProgress(url, formData, uploadToken, onProgress) {
  return new Promise((resolve, reject) => {
    const xhr = new XMLHttpRequest();
    xhr.open('POST', url);
    xhr.setRequestHeader('X-Appeal-Upload-Token', uploadToken);

    xhr.upload.addEventListener('progress', (e) => {
      if (e.lengthComputable && onProgress) {
        onProgress(Math.round((e.loaded / e.total) * 100));
      }
    });

    xhr.addEventListener('load', () => {
      if (xhr.status >= 200 && xhr.status < 300) {
        try {
          resolve(JSON.parse(xhr.responseText));
        } catch {
          resolve({ uploaded: [], errors: ['服务器返回了无效的响应'] });
        }
      } else {
        try {
          const err = JSON.parse(xhr.responseText);
          reject(new Error(err.error || `上传失败 (${xhr.status})`));
        } catch {
          reject(new Error(`上传失败 (${xhr.status})`));
        }
      }
    });

    xhr.addEventListener('error', () => reject(new Error('网络错误，上传中断')));
    xhr.addEventListener('abort', () => reject(new Error('上传已取消')));
    xhr.addEventListener('timeout', () => reject(new Error('上传超时，请检查网络后重试')));
    xhr.timeout = 600_000; // 10分钟超时
    xhr.send(formData);
  });
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

  // 玩家已有的申诉记录
  const [existingAppeals, setExistingAppeals] = useState(null);
  const [loadingAppeals, setLoadingAppeals] = useState(false);

  // 流程状态: 'idle' | 'submitting' | 'uploading' | 'done'
  const [phase, setPhase] = useState('idle');
  const [error, setError] = useState('');
  const [message, setMessage] = useState('');

  // 文件列表，每项: { id, file, category, status:'pending'|'uploading'|'done'|'error', error? }
  const [selectedFiles, setSelectedFiles] = useState([]);
  const [uploadProgress, setUploadProgress] = useState(0);
  const fileIdCounter = useRef(0);

  const videoRef = useRef(null);
  const imageRef = useRef(null);
  const audioRef = useRef(null);

  function handleSteamChange(value) {
    setSteamInput(value);
    setResolveError('');
    setBans(null);
    setSelectedBanId('');
    setSteamid64('');
    setExistingAppeals(null);
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

      // 并发查询封禁记录和已有申诉
      setLoadingBans(true);
      setLoadingAppeals(true);
      try {
        const banResult = await api.queryActiveBans({ steam_input: trimmed });
        setBans(banResult.bans ?? []);
      } catch {
        setBans([]);
      } finally {
        setLoadingBans(false);
      }

      try {
        const appealResult = await api.queryAppealStatus({ steam_input: trimmed });
        setExistingAppeals(appealResult.appeals ?? []);
      } catch {
        setExistingAppeals([]);
      } finally {
        setLoadingAppeals(false);
      }
    } catch {
      setResolveError('无法解析 Steam 标识符，请检查输入。');
      setBans(null);
      setExistingAppeals(null);
    } finally {
      setResolving(false);
    }
  }

  function handleFileSelect(files, inputRef) {
    if (!files || files.length === 0) return;

    const newFiles = [];
    let firstError = '';
    for (const file of files) {
      if (file.size > MAX_FILE_SIZE) {
        firstError = firstError || `文件 "${file.name}" 超过 100MB 大小限制`;
        continue;
      }
      if (file.size === 0) continue;
      const category = getFileCategory(file.name);
      if (category === 'other') {
        firstError = firstError || `不支持的文件类型: ${file.name}`;
        continue;
      }
      const id = ++fileIdCounter.current;
      newFiles.push({ id, file, category, status: 'pending' });
    }
    if (firstError) setError(firstError);
    else setError('');
    setSelectedFiles((prev) => [...prev, ...newFiles]);
    if (inputRef.current) inputRef.current.value = '';
  }

  const removeFile = useCallback((id) => {
    setSelectedFiles((prev) => prev.filter((f) => f.id !== id));
  }, []);

  async function handleSubmit() {
    if (!selectedBanId || !reason.trim() || !steamid64 || !nickname.trim()) return;
    if (phase !== 'idle') return;

    setPhase('submitting');
    setError('');
    setMessage('');
    setUploadProgress(0);

    let appealId;
    let uploadToken;
    try {
      const result = await api.submitBanAppeal({
        ban_id: selectedBanId,
        steam_id: steamid64,
        player_name: nickname.trim(),
        appeal_reason: reason.trim(),
      });
      ({ appealId, uploadToken } = normalizeAppealCreateResponse(result));
      if (!appealId) {
        console.error('Invalid ban appeal create response:', result);
        throw new Error(`服务器未返回申诉编号，请刷新页面后重试。响应摘要：${responseSummary(result)}`);
      }
    } catch (e) {
      setError(e.message || '提交失败，请稍后重试。');
      setPhase('idle');
      return;
    }

    // 无文件，直接完成
    if (selectedFiles.length === 0) {
      setMessage('申诉提交成功，管理员将会尽快审核您的申诉。');
      setPhase('done');
      setSelectedFiles([]);
      return;
    }

    // 开始上传文件
    if (!uploadToken) {
      setError('服务器未返回上传凭证，请重新提交申诉。');
      setPhase('idle');
      return;
    }

    setPhase('uploading');
    // 将所有待上传文件标记为 uploading
    setSelectedFiles((prev) =>
      prev.map((f) => (f.status === 'pending' ? { ...f, status: 'uploading' } : f)),
    );

    const formData = new FormData();
    selectedFiles.forEach(({ file }) => {
      formData.append('files', file, file.name);
    });

    try {
      const url = `${API_BASE}/api/public/ban-appeals/${appealId}/files`;
      const uploadRes = await uploadWithProgress(url, formData, uploadToken, (pct) => {
        setUploadProgress(pct);
      });

      // 标记成功上传的文件
      const uploadedNames = new Set((uploadRes.uploaded || []).map((u) => u.file_name));
      setSelectedFiles((prev) =>
        prev.map((f) =>
          uploadedNames.has(f.file.name)
            ? { ...f, status: 'done' }
            : { ...f, status: 'error', error: '服务器未确认上传' },
        ),
      );

      const failedCount = selectedFiles.length - uploadedNames.size + (uploadRes.errors?.length || 0);
      if (failedCount > 0) {
        setMessage(
          `申诉已提交。${uploadRes.uploaded?.length || 0} 个文件上传成功，${failedCount} 个文件上传失败。管理员将会尽快审核您的申诉。`,
        );
      } else {
        setMessage('申诉提交成功，辅助文件已全部上传。管理员将会尽快审核您的申诉。');
      }
    } catch (uploadErr) {
      // 文件上传失败但申诉已成功
      setSelectedFiles((prev) =>
        prev.map((f) =>
          f.status === 'uploading' ? { ...f, status: 'error', error: uploadErr.message } : f,
        ),
      );
      setMessage('申诉已提交，但辅助文件上传失败（' + uploadErr.message + '）。管理员仍会审核您的申诉。');
    }

    setPhase('done');
  }

  function isSubmitting() {
    return phase === 'submitting' || phase === 'uploading';
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

      <div style={{ maxWidth: 560, margin: '0 auto' }}>
        <div className="public-card">
          <div className="public-card-body">
            {/* Steam 标识符 */}
            <div className="form-group">
              <label>Steam 标识符 <span className="text-accent">*</span></label>
              <input
                type="text" className="form-control"
                value={steamInput}
                onChange={(e) => handleSteamChange(e.target.value)}
                onBlur={handleSteamBlur}
                placeholder="SteamID64 / SteamID / 个人主页链接"
                disabled={isSubmitting() || resolving}
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
              <label>游戏昵称 <span className="text-accent">*</span></label>
              <input
                type="text" className="form-control"
                value={nickname}
                onChange={(e) => setNickname(e.target.value)}
                placeholder="您的 Steam 名称"
                disabled={isSubmitting()}
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
                <label>选择要申诉的封禁记录 <span className="text-accent">*</span></label>
                {bans.length === 0 ? (
                  <div style={{ padding: 14, color: 'var(--text3)', fontSize: 13, background: 'var(--surface2)', borderRadius: 8, textAlign: 'center' }}>
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
                        <input type="radio" name="banRecord" value={ban.id}
                          checked={selectedBanId === ban.id}
                          onChange={() => setSelectedBanId(ban.id)}
                          disabled={isSubmitting()}
                          style={{ marginTop: 2 }}
                        />
                        <div style={{ flex: 1, minWidth: 0 }}>
                          <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 4 }}>
                            <span className="status-pill pill-accent fs-11">
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
                            封禁时间：{formatChinaDateTime(ban.created_at, { seconds: false })} · 操作人：{ban.operator_name}
                            {ban.source === 'game_plugin' ? ' · 来源：游戏插件' : ''}
                          </div>
                        </div>
                      </label>
                    ))}
                  </div>
                )}
              </div>
            )}

            {/* 历史申诉记录 */}
            {steamid64 && !loadingAppeals && existingAppeals !== null && existingAppeals.length > 0 && (
              <div className="form-section-card mb-16">
                <div className="form-section-header">
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" width="16" height="16">
                    <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
                    <polyline points="14 2 14 8 20 8" />
                    <line x1="16" y1="13" x2="8" y2="13" />
                    <line x1="16" y1="17" x2="8" y2="17" />
                  </svg>
                  <span>我的申诉记录</span>
                </div>
                <div className="form-hint" style={{ marginBottom: 14 }}>
                  以下是您之前提交的封禁申诉及其审核结果。
                </div>
                <div style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
                  {existingAppeals.map((appeal) => {
                    const st = APPEAL_STATUS_MAP[appeal.status] || { label: appeal.status, icon: '📋', color: 'var(--text2)', bg: 'var(--surface2)' };
                    return (
                      <div
                        key={appeal.id}
                        style={{
                          border: '1px solid var(--border)',
                          borderRadius: 10,
                          overflow: 'hidden',
                          transition: 'border-color 0.2s',
                        }}
                      >
                        {/* 申诉卡片头部 - 状态 */}
                        <div style={{
                          display: 'flex', alignItems: 'center', justifyContent: 'space-between',
                          padding: '10px 14px',
                          background: st.bg,
                          borderBottom: '1px solid var(--border)',
                        }}>
                          <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                            <span style={{ fontSize: 16 }}>{st.icon}</span>
                            <span style={{ fontWeight: 600, fontSize: 13, color: st.color }}>{st.label}</span>
                          </div>
                          <span style={{ fontSize: 11, color: 'var(--text3)' }}>
                            提交于 {formatChinaDateTime(appeal.created_at, { seconds: false })}
                          </span>
                        </div>

                        {/* 申诉卡片内容 */}
                        <div style={{ padding: '12px 14px' }}>
                          {/* 封禁信息 */}
                          <div style={{ fontSize: 13, color: 'var(--text2)', marginBottom: 8 }}>
                            <div style={{ marginBottom: 3 }}>
                              <strong>封禁原因：</strong>{appeal.ban_reason || '-'}
                            </div>
                            <div style={{ fontSize: 12, color: 'var(--text3)' }}>
                              {appeal.ban_type === 'steam' ? 'Steam 封禁' : appeal.ban_type === 'ip' ? 'IP 封禁' : ''}
                              {appeal.ban_server_name ? ` · ${appeal.ban_server_name}` : ''}
                            </div>
                          </div>

                          {/* 玩家的申诉理由 */}
                          <div style={{
                            fontSize: 13, color: 'var(--text2)',
                            padding: '8px 10px', background: 'var(--surface2)', borderRadius: 6,
                            marginBottom: 8,
                          }}>
                            <div style={{ fontSize: 11, color: 'var(--text3)', marginBottom: 4 }}>我的申诉理由：</div>
                            {appeal.appeal_reason}
                          </div>

                          {/* 管理员审核结果 */}
                          {appeal.status !== 'pending' && (
                            <div style={{
                              fontSize: 13,
                              padding: '8px 10px', borderRadius: 6,
                              background: appeal.status === 'approved' ? 'var(--success-bg, #dcfce7)' : 'var(--danger-bg, #fee2e2)',
                              border: `1px solid ${appeal.status === 'approved' ? 'var(--success-text, #22c55e)' : 'var(--danger-text, #ef4444)'}33`,
                            }}>
                              <div style={{ display: 'flex', alignItems: 'center', gap: 6, marginBottom: 4 }}>
                                <span style={{ fontWeight: 600, color: st.color }}>
                                  {appeal.status === 'approved' ? '管理员已通过申诉并解封' : '管理员已驳回申诉'}
                                </span>
                              </div>
                              {appeal.reviewed_by && (
                                <div style={{ fontSize: 12, color: 'var(--text3)', marginBottom: 2 }}>
                                  审核人：{appeal.reviewed_by}
                                  {appeal.reviewed_at ? ` · ${formatChinaDateTime(appeal.reviewed_at, { seconds: false })}` : ''}
                                </div>
                              )}
                              {appeal.review_note && (
                                <div style={{
                                  fontSize: 13, color: 'var(--text2)',
                                  marginTop: 4, paddingTop: 6,
                                  borderTop: '1px solid var(--border)',
                                }}>
                                  <span style={{ fontSize: 11, color: 'var(--text3)' }}>管理员回复：</span>
                                  {appeal.review_note}
                                </div>
                              )}
                            </div>
                          )}
                        </div>
                      </div>
                    );
                  })}
                </div>
              </div>
            )}

            {/* 申诉理由 */}
            {bans !== null && bans.length > 0 && (
              <div className="form-group">
                <label>申诉理由 <span className="text-accent">*</span></label>
                <textarea className="form-control"
                  value={reason}
                  onChange={(e) => setReason(e.target.value)}
                  placeholder="请详细说明您认为该封禁有误的原因，或提供相关证据..."
                  rows={4}
                  disabled={isSubmitting()}
                  style={{ resize: 'vertical', minHeight: 100 }}
                />
                <div className="form-hint">请如实填写，管理员将根据您的理由重新审核封禁记录。</div>
              </div>
            )}

            {/* 辅助文件上传 */}
            {bans !== null && bans.length > 0 && (
              <div className="form-section-card mb-16">
                <div className="form-section-header">
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" width="16" height="16">
                    <path d="M21.44 11.05l-9.19 9.19a6 6 0 0 1-8.49-8.49l9.19-9.19a4 4 0 0 1 5.66 5.66l-9.2 9.19a2 2 0 0 1-2.83-2.83l8.49-8.48" />
                  </svg>
                  <span>辅助文件（选填）</span>
                </div>
                <div className="form-hint" style={{ marginBottom: 14 }}>
                  可上传录像、截图或录音来辅助申诉。单个文件最大 100MB，选择文件后在提交时一并上传。
                </div>

                <div style={{ display: 'flex', gap: 10, flexWrap: 'wrap', marginBottom: 14 }}>
                  <button type="button" className="btn btn-outline btn-sm fs-12"
                    onClick={() => videoRef.current?.click()}
                    disabled={isSubmitting()}
                  >
                    🎬 选择录像
                  </button>
                  <input ref={videoRef} type="file" accept={ALLOWED_VIDEO} multiple
                    style={{ display: 'none' }}
                    onChange={(e) => handleFileSelect(e.target.files, videoRef)}
                  />
                  <button type="button" className="btn btn-outline btn-sm fs-12"
                    onClick={() => imageRef.current?.click()}
                    disabled={isSubmitting()}
                  >
                    🖼 选择图片
                  </button>
                  <input ref={imageRef} type="file" accept={ALLOWED_IMAGE} multiple
                    style={{ display: 'none' }}
                    onChange={(e) => handleFileSelect(e.target.files, imageRef)}
                  />
                  <button type="button" className="btn btn-outline btn-sm fs-12"
                    onClick={() => audioRef.current?.click()}
                    disabled={isSubmitting()}
                  >
                    🎵 选择录音
                  </button>
                  <input ref={audioRef} type="file" accept={ALLOWED_AUDIO} multiple
                    style={{ display: 'none' }}
                    onChange={(e) => handleFileSelect(e.target.files, audioRef)}
                  />
                </div>

                {/* 文件列表 + 上传进度 */}
                {selectedFiles.length > 0 && (
                  <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
                    {/* 总进度条（上传中显示） */}
                    {phase === 'uploading' && (
                      <div className="mb-4">
                        <div style={{ display: 'flex', justifyContent: 'space-between', fontSize: 12, marginBottom: 4, color: 'var(--text2)' }}>
                          <span>正在上传文件...</span>
                          <span style={{ fontWeight: 600, color: 'var(--accent)' }}>{uploadProgress}%</span>
                        </div>
                        <div style={{ height: 6, background: 'var(--border)', borderRadius: 3, overflow: 'hidden' }}>
                          <div style={{
                            height: '100%', borderRadius: 3,
                            background: 'linear-gradient(90deg, var(--accent), var(--accent2))',
                            width: `${uploadProgress}%`,
                            transition: 'width 0.3s ease',
                          }} />
                        </div>
                      </div>
                    )}

                    {selectedFiles.map((f) => (
                      <div key={f.id} style={{
                        display: 'flex', alignItems: 'center', justifyContent: 'space-between',
                        padding: '10px 12px',
                        background: f.status === 'error' ? 'var(--danger-bg)'
                          : f.status === 'done' ? 'var(--success-bg)'
                          : 'var(--surface2)',
                        borderRadius: 8, fontSize: 13,
                        transition: 'background 0.2s',
                      }}>
                        <div style={{ display: 'flex', alignItems: 'center', gap: 8, minWidth: 0, flex: 1 }}>
                          {/* 状态图标 */}
                          <span style={{ fontSize: 16, flexShrink: 0 }}>
                            {f.status === 'uploading' ? (
                              <span className="loading-spinner" style={{ width: 16, height: 16, borderWidth: 2 }} />
                            ) : f.status === 'done' ? '✅' : f.status === 'error' ? '❌' : fileCategoryIcon(f.category)}
                          </span>
                          <div style={{ minWidth: 0 }}>
                            <div style={{ fontWeight: 500, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                              {f.file.name}
                            </div>
                            <div style={{ fontSize: 11, color: 'var(--text3)' }}>
                              {formatFileSize(f.file.size)}
                              {f.status === 'uploading' && ' · 上传中...'}
                              {f.status === 'done' && ' · 已上传'}
                              {f.status === 'error' && (f.error ? ` · ${f.error}` : ' · 上传失败')}
                            </div>
                          </div>
                        </div>
                        {!isSubmitting() && (
                          <button type="button" onClick={() => removeFile(f.id)}
                            style={{ background: 'none', border: 'none', color: 'var(--text3)', cursor: 'pointer', fontSize: 16, padding: '2px 6px', flexShrink: 0 }}
                            title="移除"
                          >✕</button>
                        )}
                      </div>
                    ))}
                  </div>
                )}
              </div>
            )}

            {/* 反馈信息 */}
            {error && (
              <div className="alert alert-error">
                <span className="alert-icon">✕</span>
                <div className="alert-content">
                  <div className="alert-title">提交失败</div>
                  <div className="alert-text">{error}</div>
                </div>
              </div>
            )}
            {message && (
              <div className="alert alert-success">
                <span className="alert-icon">✓</span>
                <div className="alert-content">
                  <div className="alert-title">申诉已提交</div>
                  <div className="alert-text">{message}</div>
                </div>
              </div>
            )}

            {/* 提交按钮 */}
            {bans !== null && bans.length > 0 && phase !== 'done' && (
              <button
                className="btn btn-accent"
                style={{ width: '100%', padding: 12, fontSize: 14, marginTop: 6 }}
                type="button"
                disabled={isSubmitting() || resolving || !selectedBanId || !reason.trim()}
                onClick={handleSubmit}
              >
                {phase === 'submitting' ? '正在创建申诉...' :
                 phase === 'uploading' ? `正在上传文件 (${uploadProgress}%)...` :
                 '提交封禁申诉'}
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

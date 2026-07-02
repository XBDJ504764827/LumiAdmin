import { useRef, useState } from 'react';
import { publicApi } from '../../lib/publicApi.js';
import { Modal } from '../../shared/Modal.jsx';
import { PublicPageShell } from './PublicPageShell.jsx';
import { formatChinaDateTime } from '../../shared/time.js';

const MAX_FILE_SIZE = 100 * 1024 * 1024;
const ALLOWED_VIDEO = '.mp4,.avi,.mov,.webm,.mkv';
const ALLOWED_IMAGE = '.jpg,.jpeg,.png,.gif,.webp,.bmp';
const ALLOWED_AUDIO = '.mp3,.wav,.ogg,.m4a,.flac';
const API_BASE = import.meta.env.VITE_API_BASE ?? '';

const REPORT_STATUS_MAP = {
  pending:  { label: '待审核',   icon: '⏳', color: '#f59e0b', bg: '#fef3c7' },
  approved: { label: '已处理',   icon: '✅', color: '#22c55e', bg: '#dcfce7' },
  rejected: { label: '已驳回',   icon: '❌', color: '#ef4444', bg: '#fee2e2' },
};

function getFileCategory(name) {
  const lower = name.toLowerCase();
  if (lower.match(/\.(mp4|avi|mov|webm|mkv)$/)) return 'video';
  if (lower.match(/\.(jpg|jpeg|png|gif|webp|bmp)$/)) return 'image';
  if (lower.match(/\.(mp3|wav|ogg|m4a|flac)$/)) return 'audio';
  return 'other';
}

function fileCategoryIcon(cat) {
  if (cat === 'video') return '🎬';
  if (cat === 'image') return '🖼';
  if (cat === 'audio') return '🎵';
  return '📎';
}

function formatFileSize(bytes) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

function uploadWithProgress(reportId, formData, uploadToken, onProgress) {
  return new Promise((resolve, reject) => {
    const xhr = new XMLHttpRequest();
    xhr.open('POST', `${API_BASE}/api/public/player-reports/${reportId}/files`);
    xhr.setRequestHeader('X-Report-Upload-Token', uploadToken);
    xhr.upload.addEventListener('progress', (event) => {
      if (event.lengthComputable) onProgress(Math.round((event.loaded / event.total) * 100));
    });
    xhr.addEventListener('load', () => {
      const payload = (() => {
        try { return JSON.parse(xhr.responseText); } catch { return {}; }
      })();
      if (xhr.status >= 200 && xhr.status < 300) {
        onProgress(100);
        resolve(payload);
        return;
      }
      reject(new Error(payload.error || `上传失败 (${xhr.status})`));
    });
    xhr.addEventListener('error', () => reject(new Error('网络错误，上传中断')));
    xhr.addEventListener('timeout', () => reject(new Error('上传超时，请检查网络后重试')));
    xhr.timeout = 600_000;
    xhr.send(formData);
  });
}

export function PublicPlayerReportPage() {
  const [steamInput, setSteamInput] = useState('');
  const [targetName, setTargetName] = useState('');
  const [contact, setContact] = useState('');
  const [contactPromptOpen, setContactPromptOpen] = useState(false);
  const [contactPromptValue, setContactPromptValue] = useState('');
  const [reason, setReason] = useState('');
  const [files, setFiles] = useState([]);
  const [phase, setPhase] = useState('idle');
  const [progress, setProgress] = useState(0);
  const [error, setError] = useState('');
  const [message, setMessage] = useState('');
  const videoRef = useRef(null);
  const imageRef = useRef(null);
  const audioRef = useRef(null);
  const idRef = useRef(0);

  // Query state
  const [existingReports, setExistingReports] = useState(null);
  const [loadingReports, setLoadingReports] = useState(false);

  function handleFileSelect(selected, inputRef) {
    if (!selected?.length) return;
    let firstError = '';
    const next = [];
    for (const file of selected) {
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
      next.push({ id: ++idRef.current, file, category, status: 'pending' });
    }
    if (firstError) setError(firstError);
    else setError('');
    setFiles((prev) => [...prev, ...next]);
    if (inputRef?.current) inputRef.current.value = '';
  }

  function removeFile(id) {
    setFiles((prev) => prev.filter((item) => item.id !== id));
  }

  async function handleResolve() {
    if (!steamInput.trim()) {
      setExistingReports(null);
      return;
    }
    try {
      const result = await publicApi.resolveSteam({ steam_input: steamInput.trim() });
      if (result.persona_name && !targetName.trim()) setTargetName(result.persona_name);
    } catch {}

    // Query existing reports for this player
    setLoadingReports(true);
    try {
      const result = await publicApi.queryPlayerReportStatus({ steam_input: steamInput.trim() });
      setExistingReports(result.reports ?? []);
    } catch {
      setExistingReports(null);
    } finally {
      setLoadingReports(false);
    }
  }

  async function handleSubmit(options = {}) {
    if (!steamInput.trim() || !reason.trim() || phase !== 'idle') return;
    const contactValue = options.contactValue ?? contact;
    if (!options.allowEmptyContact && !contactValue.trim()) {
      setContactPromptValue(contact);
      setContactPromptOpen(true);
      return;
    }

    setPhase('submitting');
    setError('');
    setMessage('');
    setProgress(0);

    let reportId;
    let uploadToken;
    try {
      const result = await publicApi.submitPlayerReport({
        steam_input: steamInput.trim(),
        target_player_name: targetName.trim() || null,
        reporter_contact: contactValue.trim() || null,
        report_reason: reason.trim(),
      });
      reportId = result.report_id ?? result.id ?? result.item?.id;
      uploadToken = result.upload_token ?? result.item?.upload_token;
      if (!reportId) throw new Error('服务器未返回举报编号，请稍后重试。');
    } catch (submitError) {
      setError(submitError.message || '提交失败，请稍后重试。');
      setPhase('idle');
      return;
    }

    if (files.length > 0) {
      if (!uploadToken) {
        setError('服务器未返回上传凭证，请重新提交举报。');
        setPhase('idle');
        return;
      }
      setPhase('uploading');
      setFiles((prev) =>
        prev.map((item) => (item.status === 'pending' ? { ...item, status: 'uploading' } : item)),
      );
      const formData = new FormData();
      files.forEach(({ file }) => formData.append('files', file, file.name));
      try {
        const uploadResult = await uploadWithProgress(reportId, formData, uploadToken, setProgress);
        const uploadedNames = new Set((uploadResult.uploaded || []).map((item) => item.file_name));
        setFiles((prev) =>
          prev.map((item) =>
            uploadedNames.has(item.file.name)
              ? { ...item, status: 'done' }
              : { ...item, status: 'error', error: '服务器未确认上传' },
          ),
        );

        const failedCount = files.length - uploadedNames.size + (uploadResult.errors?.length || 0);
        if (failedCount > 0) {
          setMessage(`举报已提交。${uploadResult.uploaded?.length || 0} 个文件上传成功，${failedCount} 个文件上传失败。管理员将会尽快审核。`);
        } else {
          setMessage('举报已提交，辅助文件已上传。管理员将会尽快审核。');
        }
      } catch (uploadError) {
        setFiles((prev) =>
          prev.map((item) =>
            item.status === 'uploading' ? { ...item, status: 'error', error: uploadError.message } : item,
          ),
        );
        setMessage(`举报已提交，但辅助文件上传失败：${uploadError.message}`);
      }
    } else {
      setMessage('举报已提交。管理员将会尽快审核。');
    }

    setPhase('done');
  }

  function submitWithPromptContact() {
    setContact(contactPromptValue);
    setContactPromptOpen(false);
    handleSubmit({ allowEmptyContact: true, contactValue: contactPromptValue });
  }

  function submitWithoutContact() {
    setContact('');
    setContactPromptValue('');
    setContactPromptOpen(false);
    handleSubmit({ allowEmptyContact: true, contactValue: '' });
  }

  const busy = phase === 'submitting' || phase === 'uploading';

  return (
    <PublicPageShell>
      <div className="public-hero">
        <div className="public-hero-icon">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <path d="M12 9v4" /><path d="M12 17h.01" /><path d="M10.29 3.86L1.82 18a2 2 0 001.71 3h16.94a2 2 0 001.71-3L13.71 3.86a2 2 0 00-3.42 0z" />
          </svg>
        </div>
        <h1>玩家举报</h1>
        <p>提交违规玩家信息与证据文件，管理员会根据举报内容进行审核。</p>
      </div>

      <div style={{ maxWidth: 620, margin: '0 auto' }}>
        <div className="public-card">
          <div className="public-card-body">
            <div className="form-group">
              <label>被举报玩家 Steam 标识符 <span className="text-accent">*</span></label>
              <input className="form-control" value={steamInput} onChange={(event) => setSteamInput(event.target.value)} onBlur={handleResolve} disabled={busy} placeholder="SteamID64 / SteamID / 个人主页链接" />
            </div>

            {/* 历史举报记录 */}
            {loadingReports && (
              <div style={{ color: 'var(--text3)', fontSize: 13, padding: '8px 0 16px', textAlign: 'center' }}>
                正在查询历史举报记录...
              </div>
            )}
            {existingReports !== null && !loadingReports && existingReports.length > 0 && (
              <div className="form-section-card mb-16">
                <div className="form-section-header">
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" width="16" height="16">
                    <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
                    <polyline points="14 2 14 8 20 8" />
                    <line x1="16" y1="13" x2="8" y2="13" />
                    <line x1="16" y1="17" x2="8" y2="17" />
                  </svg>
                  <span>举报记录</span>
                </div>
                <div className="form-hint" style={{ marginBottom: 14 }}>
                  以下是该玩家之前的举报记录及审核结果。
                </div>
                <div style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
                  {existingReports.map((report) => {
                    const st = REPORT_STATUS_MAP[report.status] || { label: report.status, icon: '📋', color: 'var(--text2)', bg: 'var(--surface2)' };
                    return (
                      <div
                        key={report.id}
                        style={{
                          border: '1px solid var(--border)',
                          borderRadius: 10,
                          overflow: 'hidden',
                          transition: 'border-color 0.2s',
                        }}
                      >
                        {/* 卡片头部 - 状态 */}
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
                            提交于 {formatChinaDateTime(report.created_at, { seconds: false })}
                          </span>
                        </div>

                        {/* 卡片内容 */}
                        <div style={{ padding: '12px 14px' }}>
                          {/* 举报理由 */}
                          <div style={{
                            fontSize: 13, color: 'var(--text2)',
                            padding: '8px 10px', background: 'var(--surface2)', borderRadius: 6,
                            marginBottom: 8,
                          }}>
                            <div style={{ fontSize: 11, color: 'var(--text3)', marginBottom: 4 }}>举报理由：</div>
                            <div style={{ whiteSpace: 'pre-wrap', wordBreak: 'break-word' }}>{report.report_reason}</div>
                          </div>

                          {/* 管理员审核结果 */}
                          {report.status !== 'pending' && (
                            <div style={{
                              fontSize: 13,
                              padding: '8px 10px', borderRadius: 6,
                              background: report.status === 'approved' ? 'var(--success-bg, #dcfce7)' : 'var(--danger-bg, #fee2e2)',
                              border: `1px solid ${report.status === 'approved' ? 'var(--success-text, #22c55e)' : 'var(--danger-text, #ef4444)'}33`,
                            }}>
                              <div style={{ display: 'flex', alignItems: 'center', gap: 6, marginBottom: 4 }}>
                                <span style={{ fontWeight: 600, color: st.color }}>
                                  {report.status === 'approved' ? '管理员已根据举报封禁该玩家' : '管理员已驳回举报'}
                                </span>
                              </div>
                              {report.reviewed_by && (
                                <div style={{ fontSize: 12, color: 'var(--text3)', marginBottom: 2 }}>
                                  审核人：{report.reviewed_by}
                                  {report.reviewed_at ? ` · ${formatChinaDateTime(report.reviewed_at, { seconds: false })}` : ''}
                                </div>
                              )}
                              {report.review_note && (
                                <div style={{
                                  fontSize: 13, color: 'var(--text2)',
                                  marginTop: 4, paddingTop: 6,
                                  borderTop: '1px solid var(--border)',
                                }}>
                                  <span style={{ fontSize: 11, color: 'var(--text3)' }}>管理员回复：</span>
                                  <span style={{ whiteSpace: 'pre-wrap', wordBreak: 'break-word' }}>{report.review_note}</span>
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

            <div className="form-group">
              <label>被举报玩家名称</label>
              <input className="form-control" value={targetName} onChange={(event) => setTargetName(event.target.value)} disabled={busy} placeholder="可选，输入 Steam 标识符后会尝试自动获取" />
            </div>
            <div className="form-group">
              <label>联系方式</label>
              <input className="form-control" value={contact} onChange={(event) => setContact(event.target.value)} disabled={busy} placeholder="可选，便于管理员补充核实" />
            </div>
            <div className="form-group">
              <label>举报理由 <span className="text-accent">*</span></label>
              <textarea className="form-control" value={reason} onChange={(event) => setReason(event.target.value)} rows={5} disabled={busy} placeholder="请说明违规行为、发生时间、服务器、地图等信息" />
            </div>

            <div className="form-section-card">
              <div className="form-section-header">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" width="16" height="16">
                  <path d="M21.44 11.05l-9.19 9.19a6 6 0 0 1-8.49-8.49l9.19-9.19a4 4 0 0 1 5.66 5.66l-9.2 9.19a2 2 0 0 1-2.83-2.83l8.49-8.48" />
                </svg>
                <span>举报文件（选填）</span>
              </div>
              <div className="form-hint" style={{ marginBottom: 14 }}>
                可上传录像、截图或录音来辅助举报。单个文件最大 100MB，选择文件后在提交时一并上传。
              </div>

              <div style={{ display: 'flex', gap: 10, flexWrap: 'wrap', marginBottom: 14 }}>
                <button type="button" className="btn btn-outline btn-sm fs-12" onClick={() => videoRef.current?.click()} disabled={busy}>
                  🎬 选择录像
                </button>
                <input ref={videoRef} type="file" accept={ALLOWED_VIDEO} multiple style={{ display: 'none' }} onChange={(event) => handleFileSelect(event.target.files, videoRef)} />
                <button type="button" className="btn btn-outline btn-sm fs-12" onClick={() => imageRef.current?.click()} disabled={busy}>
                  🖼 选择图片
                </button>
                <input ref={imageRef} type="file" accept={ALLOWED_IMAGE} multiple style={{ display: 'none' }} onChange={(event) => handleFileSelect(event.target.files, imageRef)} />
                <button type="button" className="btn btn-outline btn-sm fs-12" onClick={() => audioRef.current?.click()} disabled={busy}>
                  🎵 选择录音
                </button>
                <input ref={audioRef} type="file" accept={ALLOWED_AUDIO} multiple style={{ display: 'none' }} onChange={(event) => handleFileSelect(event.target.files, audioRef)} />
              </div>

              {files.length > 0 ? (
                <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
                  {phase === 'uploading' ? (
                    <div className="mb-4">
                      <div style={{ display: 'flex', justifyContent: 'space-between', fontSize: 12, marginBottom: 4, color: 'var(--text2)' }}>
                        <span>正在上传文件...</span>
                        <span style={{ fontWeight: 600, color: 'var(--accent)' }}>{progress}%</span>
                      </div>
                      <div style={{ height: 6, background: 'var(--border)', borderRadius: 3, overflow: 'hidden' }}>
                        <div style={{
                          height: '100%',
                          borderRadius: 3,
                          background: 'linear-gradient(90deg, var(--accent), var(--accent2))',
                          width: `${progress}%`,
                          transition: 'width 0.3s ease',
                        }} />
                      </div>
                    </div>
                  ) : null}

                  {files.map((item) => (
                    <div key={item.id} style={{
                      display: 'flex',
                      alignItems: 'center',
                      justifyContent: 'space-between',
                      padding: '10px 12px',
                      background: item.status === 'error' ? 'var(--danger-bg)'
                        : item.status === 'done' ? 'var(--success-bg)'
                        : 'var(--surface2)',
                      borderRadius: 8,
                      fontSize: 13,
                      transition: 'background 0.2s',
                    }}>
                      <div style={{ display: 'flex', alignItems: 'center', gap: 8, minWidth: 0, flex: 1 }}>
                        <span style={{ fontSize: 16, flexShrink: 0 }}>
                          {item.status === 'uploading' ? (
                            <span className="loading-spinner" style={{ width: 16, height: 16, borderWidth: 2 }} />
                          ) : item.status === 'done' ? '✅' : item.status === 'error' ? '❌' : fileCategoryIcon(item.category)}
                        </span>
                        <div style={{ minWidth: 0 }}>
                          <div style={{ fontWeight: 500, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                            {item.file.name}
                          </div>
                          <div style={{ fontSize: 11, color: 'var(--text3)' }}>
                            {formatFileSize(item.file.size)}
                            {item.status === 'uploading' && ' · 上传中...'}
                            {item.status === 'done' && ' · 已上传'}
                            {item.status === 'error' && (item.error ? ` · ${item.error}` : ' · 上传失败')}
                          </div>
                        </div>
                      </div>
                      {!busy ? (
                        <button
                          type="button"
                          onClick={() => removeFile(item.id)}
                          style={{ background: 'none', border: 'none', color: 'var(--text3)', cursor: 'pointer', fontSize: 16, padding: '2px 6px', flexShrink: 0 }}
                          title="移除"
                        >
                          ✕
                        </button>
                      ) : null}
                    </div>
                  ))}
                </div>
              ) : null}
            </div>

            {error ? <div className="alert alert-error"><span className="alert-icon">✕</span><div className="alert-content"><div className="alert-title">提交失败</div><div className="alert-text">{error}</div></div></div> : null}
            {message ? <div className="alert alert-success"><span className="alert-icon">✓</span><div className="alert-content"><div className="alert-title">举报已提交</div><div className="alert-text">{message}</div></div></div> : null}

            {phase !== 'done' ? (
              <button className="btn btn-accent" type="button" style={{ width: '100%', padding: 12 }} disabled={busy || !steamInput.trim() || !reason.trim()} onClick={() => handleSubmit()}>
                {phase === 'submitting' ? '正在提交...' : phase === 'uploading' ? `正在上传 (${progress}%)...` : '提交举报'}
              </button>
            ) : null}
          </div>
        </div>
      </div>

      <Modal
        open={contactPromptOpen}
        title="建议填写联系方式"
        onClose={() => setContactPromptOpen(false)}
        footer={
          <>
            <button className="btn btn-outline" type="button" onClick={submitWithoutContact} disabled={busy}>不填写，继续提交</button>
            <button className="btn btn-primary" type="button" onClick={submitWithPromptContact} disabled={busy || !contactPromptValue.trim()}>
              {busy ? '提交中...' : '填写并提交'}
            </button>
          </>
        }
      >
        <div className="alert alert-warning">
          <span className="alert-icon">!</span>
          <div className="alert-content">
            <div className="alert-title">强烈建议您填写联系方式</div>
            <div className="alert-text">QQ / 微信 / 邮箱等联系方式可以帮助管理员在处理举报时与您确认信息。不填写也可以继续提交举报。</div>
          </div>
        </div>
        <div className="form-group">
          <label>联系方式</label>
          <input
            type="text"
            className="form-control"
            value={contactPromptValue}
            onChange={(event) => setContactPromptValue(event.target.value)}
            placeholder="QQ / 微信 / 邮箱等"
            disabled={busy}
            autoFocus
          />
        </div>
      </Modal>
    </PublicPageShell>
  );
}

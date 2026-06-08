import React, { useRef, useState } from 'react';
import { api } from '../../lib/api.js';
import { Modal } from '../../shared/Modal.jsx';
import { useToast } from '../../shared/Toast.jsx';
import { BAN_TYPE_OPTIONS, banModalSubmitText, banModalTitle, buildBanFormFromRecord, buildCreateBanPayload, emptyBanForm, validateBanForm } from './banForm.js';
import { notifyPendingReviewsUpdated } from '../../hooks/usePendingReviewIndicators.js';

const MAX_FILE_SIZE = 100 * 1024 * 1024;
const ALLOWED_VIDEO = '.mp4,.avi,.mov,.webm,.mkv';
const ALLOWED_IMAGE = '.jpg,.jpeg,.png,.gif,.webp,.bmp';
const ALLOWED_AUDIO = '.mp3,.wav,.ogg,.m4a,.flac';
const API_BASE = import.meta.env.VITE_API_BASE ?? '';

function formatFileSize(bytes) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

function getFileCategory(name) {
  const lower = name.toLowerCase();
  if (lower.match(/\.(mp4|avi|mov|webm|mkv)$/)) return 'video';
  if (lower.match(/\.(jpg|jpeg|png|gif|webp|bmp)$/)) return 'image';
  if (lower.match(/\.(mp3|wav|ogg|m4a|flac)$/)) return 'audio';
  return 'other';
}

function fileCategoryLabel(category) {
  if (category === 'video') return '录像';
  if (category === 'image') return '截图';
  if (category === 'audio') return '录音';
  return '文件';
}

function uploadBanFilesWithProgress(banId, formData, token, onProgress) {
  return new Promise((resolve, reject) => {
    const xhr = new XMLHttpRequest();
    xhr.open('POST', `${API_BASE}/api/bans/${banId}/files`);
    if (token) xhr.setRequestHeader('Authorization', `Bearer ${token}`);
    xhr.upload.addEventListener('progress', (event) => {
      if (event.lengthComputable && onProgress) onProgress(Math.round((event.loaded / event.total) * 100));
    });
    xhr.addEventListener('load', () => {
      const payload = (() => { try { return JSON.parse(xhr.responseText); } catch { return {}; } })();
      if (xhr.status >= 200 && xhr.status < 300) { if (onProgress) onProgress(100); resolve(payload); return; }
      const message = payload.error || payload.message || `辅助文件上传失败 (${xhr.status})`;
      reject(new Error(Array.isArray(payload.errors) ? `${message}：${payload.errors.join('；')}` : message));
    });
    xhr.addEventListener('error', () => reject(new Error('网络错误，上传中断')));
    xhr.addEventListener('abort', () => reject(new Error('上传已取消')));
    xhr.addEventListener('timeout', () => reject(new Error('上传超时，请检查网络后重试')));
    xhr.timeout = 600_000;
    xhr.send(formData);
  });
}

/**
 * 封禁创建/编辑弹窗
 * @param {Object} props
 * @param {boolean} props.open - 是否打开
 * @param {'create'|'edit'|'reban'} props.mode - 弹窗模式
 * @param {string|null} props.editingBanId - 编辑时的封禁 ID
 * @param {Object|null} props.reportReview - 从举报页跳转时的预填数据 { reportId, player }
 * @param {Object|null} props.prefillForm - 预填表单数据（reban 时）
 * @param {Function} props.onClose - 关闭回调
 * @param {Function} props.onSuccess - 保存成功后的回调
 * @param {string} props.token - API token
 */
export function BanFormModal({ open, mode, editingBanId, reportReview, prefillForm, onClose, onSuccess, token }) {
  const { toast } = useToast();
  const [form, setForm] = useState(emptyBanForm);
  const [error, setError] = useState('');
  const [submitting, setSubmitting] = useState(false);
  const [savePhase, setSavePhase] = useState('idle');
  const [uploadProgress, setUploadProgress] = useState(0);
  const [selectedFiles, setSelectedFiles] = useState([]);
  const fileIdCounter = useRef(0);
  const videoRef = useRef(null);
  const imageRef = useRef(null);
  const audioRef = useRef(null);

  // 当外部数据变化时重置
  React.useEffect(() => {
    if (open) {
      if (prefillForm) {
        setForm(prefillForm);
      } else {
        setForm(emptyBanForm);
      }
      setError('');
      setSelectedFiles([]);
      setUploadProgress(0);
      setSavePhase('idle');
    }
  }, [open, prefillForm]);

  function handleFileSelect(files, inputRef) {
    if (!files || files.length === 0) return;
    const newFiles = [];
    let firstError = '';
    for (const file of files) {
      if (file.size > MAX_FILE_SIZE) { firstError = firstError || `文件 "${file.name}" 超过 100MB 大小限制`; continue; }
      if (file.size === 0) continue;
      const category = getFileCategory(file.name);
      if (category === 'other') { firstError = firstError || `不支持的文件类型: ${file.name}`; continue; }
      newFiles.push({ id: ++fileIdCounter.current, file, category });
    }
    if (firstError) setError(firstError);
    else setError('');
    setSelectedFiles((prev) => [...prev, ...newFiles]);
    if (inputRef.current) inputRef.current.value = '';
  }

  function removeFile(id) {
    setSelectedFiles((prev) => prev.filter((file) => file.id !== id));
  }

  async function uploadSelectedFiles(banId) {
    if (selectedFiles.length === 0) return null;
    const formData = new FormData();
    selectedFiles.forEach(({ file }) => formData.append('files', file, file.name));
    return uploadBanFilesWithProgress(banId, formData, token, setUploadProgress);
  }

  async function handleSave() {
    const validationError = validateBanForm(form);
    if (validationError) { setError(validationError); return; }

    try {
      setSubmitting(true);
      setSavePhase('saving');
      setUploadProgress(0);
      setError('');
      const payload = buildCreateBanPayload(form);
      let savedItem = null;
      let uploadWarning = '';
      let uploadedFiles = false;

      if (mode === 'edit' && editingBanId) {
        const result = await api.updateBan(token, editingBanId, payload);
        savedItem = result.item;
      } else if (reportReview?.reportId) {
        const result = await api.banPlayerReport(token, reportReview.reportId, payload);
        savedItem = result.ban;
      } else {
        const result = await api.createBan(token, payload);
        savedItem = result.item;
      }

      if (mode !== 'edit' && savedItem?.id && selectedFiles.length > 0) {
        try {
          setSavePhase('uploading');
          await uploadSelectedFiles(savedItem.id);
          uploadedFiles = true;
        } catch (uploadError) {
          uploadWarning = uploadError.message || '辅助文件上传失败';
        }
      }

      onClose();
      onSuccess({ mode, savedItem, uploadedFiles, uploadWarning, reportReview });
    } catch (requestError) {
      setError(requestError.message);
    } finally {
      setSubmitting(false);
      setSavePhase('idle');
    }
  }

  if (!open) return null;

  return (
    <Modal
      open={open}
      title={banModalTitle(mode)}
      onClose={onClose}
      wide
      footer={
        <>
          <button className="btn btn-outline" onClick={onClose} disabled={submitting}>取消</button>
          <button className="btn btn-danger" onClick={handleSave} disabled={submitting}>
            {savePhase === 'uploading' ? `正在上传文件 (${uploadProgress}%)...` : banModalSubmitText(mode, submitting)}
          </button>
        </>
      }
    >
      <div className="form-group"><label>玩家名称</label><input type="text" className="form-control" value={form.player} onChange={(event) => setForm((prev) => ({ ...prev, player: event.target.value }))} placeholder="留空后等待插件自动获取" /></div>
      <div className="form-group"><label>SteamID64 <span className="text-accent">*</span></label><input type="text" className="form-control" value={form.steam_id} onChange={(event) => setForm((prev) => ({ ...prev, steam_id: event.target.value }))} placeholder="76561198000000000" /></div>
      <div className="form-group"><label>封禁属性 <span className="text-accent">*</span></label><select className="form-control" value={form.ban_type} onChange={(event) => setForm((prev) => ({ ...prev, ban_type: event.target.value }))}><option value="">请选择封禁属性</option>{BAN_TYPE_OPTIONS.map((option) => <option key={option.value} value={option.value}>{option.label}</option>)}</select></div>
      <div className="form-group"><label>IP 地址</label><input type="text" className="form-control" value={form.ip_address} onChange={(event) => setForm((prev) => ({ ...prev, ip_address: event.target.value }))} placeholder="留空后等待插件自动获取" /></div>
      <div className="form-group"><label>封禁理由 <span className="text-accent">*</span></label><textarea className="form-control" value={form.reason} onChange={(event) => setForm((prev) => ({ ...prev, reason: event.target.value }))} placeholder="请输入封禁理由" rows={3} /></div>
      {mode !== 'edit' ? (
        <div className="form-section-card mb-16">
          <div className="form-section-header"><span>辅助文件（选填）</span></div>
          <div className="form-hint mb-12">可上传录像、截图或录音作为封禁依据，单个文件最大 100MB。</div>
          <div style={{ display: 'flex', gap: 10, flexWrap: 'wrap', marginBottom: 12 }}>
            <button type="button" className="btn btn-outline btn-sm" onClick={() => videoRef.current?.click()} disabled={submitting}>选择录像</button>
            <input ref={videoRef} type="file" accept={ALLOWED_VIDEO} multiple style={{ display: 'none' }} onChange={(event) => handleFileSelect(event.target.files, videoRef)} />
            <button type="button" className="btn btn-outline btn-sm" onClick={() => imageRef.current?.click()} disabled={submitting}>选择截图</button>
            <input ref={imageRef} type="file" accept={ALLOWED_IMAGE} multiple style={{ display: 'none' }} onChange={(event) => handleFileSelect(event.target.files, imageRef)} />
            <button type="button" className="btn btn-outline btn-sm" onClick={() => audioRef.current?.click()} disabled={submitting}>选择录音</button>
            <input ref={audioRef} type="file" accept={ALLOWED_AUDIO} multiple style={{ display: 'none' }} onChange={(event) => handleFileSelect(event.target.files, audioRef)} />
          </div>
          {selectedFiles.length > 0 ? (
            <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
              {savePhase === 'uploading' ? (
                <div className="ban-upload-progress">
                  <div className="ban-upload-progress-head">
                    <span>正在上传辅助文件</span>
                    <strong>{uploadProgress}%</strong>
                  </div>
                  <div className="ban-upload-progress-track">
                    <div className="ban-upload-progress-bar" style={{ width: `${uploadProgress}%` }} />
                  </div>
                </div>
              ) : null}
              {selectedFiles.map((item) => (
                <div key={item.id} style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: 10, padding: '8px 10px', background: 'var(--surface2)', borderRadius: 8 }}>
                  <div style={{ minWidth: 0 }}>
                    <div style={{ fontSize: 13, fontWeight: 500, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{item.file.name}</div>
                    <div style={{ fontSize: 11, color: 'var(--text3)' }}>{fileCategoryLabel(item.category)} · {formatFileSize(item.file.size)}</div>
                  </div>
                  <button type="button" className="action-btn" onClick={() => removeFile(item.id)} disabled={submitting}>移除</button>
                </div>
              ))}
            </div>
          ) : null}
        </div>
      ) : null}
      {form.ban_type === 'steam' ? <div style={{ color: 'var(--text2)', fontSize: 13 }}>账号封禁：该 SteamID64 无法进入游戏服务器。</div> : null}
      {form.ban_type === 'ip' ? <div style={{ color: 'var(--text2)', fontSize: 13 }}>IP 封禁：该玩家下次进服后将自动填写 IP 并且阻止该 IP 的玩家进入服务器。</div> : null}
      {error ? <div className="text-accent">{error}</div> : null}
    </Modal>
  );
}

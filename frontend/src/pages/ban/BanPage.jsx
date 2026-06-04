import React, { useCallback, useRef, useState } from 'react';
import { api } from '../../lib/api.js';
import { useConfirmDialog } from '../../shared/ConfirmModal.jsx';
import { Modal } from '../../shared/Modal.jsx';
import { useAuth } from '../../state/auth.jsx';
import { useToast, ToastContainer } from '../../shared/Toast.jsx';
import { BAN_TYPE_OPTIONS, banModalSubmitText, banModalTitle, banRecordAction, buildBanFormFromRecord, buildCreateBanPayload, emptyBanForm, validateBanForm } from './banForm.js';
import { formatBanDuration, formatBanSource, formatExpiresAt } from './banDisplay.js';
import { SearchBar } from '../../shared/SearchBar.jsx';
import { Pagination } from '../../shared/Pagination.jsx';
import { useLocation, useNavigate } from 'react-router-dom';
import { formatChinaDateTime } from '../../shared/time.js';
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

function buildBanFormFromPlayerReport(report) {
  return {
    player: report.player ?? '',
    steam_id: report.steamId ?? '',
    ban_type: 'steam',
    ip_address: '',
    reason: report.reason ? `玩家举报：${report.reason}` : '',
  };
}

function uploadBanFilesWithProgress(banId, formData, token, onProgress) {
  return new Promise((resolve, reject) => {
    const xhr = new XMLHttpRequest();
    xhr.open('POST', `${API_BASE}/api/bans/${banId}/files`);
    if (token) xhr.setRequestHeader('Authorization', `Bearer ${token}`);

    xhr.upload.addEventListener('progress', (event) => {
      if (event.lengthComputable && onProgress) {
        onProgress(Math.round((event.loaded / event.total) * 100));
      }
    });

    xhr.addEventListener('load', () => {
      const payload = (() => {
        try { return JSON.parse(xhr.responseText); } catch { return {}; }
      })();

      if (xhr.status >= 200 && xhr.status < 300) {
        if (onProgress) onProgress(100);
        resolve(payload);
        return;
      }

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

export function BanPage() {
  const { session } = useAuth();
  const { confirm, dialog } = useConfirmDialog();
  const { toast, toasts, dismiss: dismissToast } = useToast();
  const location = useLocation();
  const navigate = useNavigate();
  const token = session?.token ?? null;
  const [refreshKey, setRefreshKey] = useState(0);
  const [search, setSearch] = useState('');
  const [status, setStatus] = useState(undefined);
  const [page, setPage] = useState(1);
  const [data, setData] = useState(null);
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState('');
  const [open, setOpen] = useState(false);
  const [modalMode, setModalMode] = useState('create');
  const [editingBanId, setEditingBanId] = useState(null);
  const [form, setForm] = useState(emptyBanForm);
  const [error, setError] = useState('');
  const [submitting, setSubmitting] = useState(false);
  const [savePhase, setSavePhase] = useState('idle');
  const [uploadProgress, setUploadProgress] = useState(0);
  const [syncingExternalId, setSyncingExternalId] = useState(null);
  const [selectedFiles, setSelectedFiles] = useState([]);
  const [reportReviewOnSave, setReportReviewOnSave] = useState(null);
  const [detailItem, setDetailItem] = useState(null);
  const [detailLoading, setDetailLoading] = useState(false);
  const [detailFiles, setDetailFiles] = useState([]);
  const [detailFilesLoading, setDetailFilesLoading] = useState(false);
  const [detailFilesError, setDetailFilesError] = useState('');
  const [apiModalOpen, setApiModalOpen] = useState(false);
  const [apiKeys, setApiKeys] = useState([]);
  const [apiKeysLoading, setApiKeysLoading] = useState(false);
  const [apiKeyName, setApiKeyName] = useState('');
  const [apiKeyCreating, setApiKeyCreating] = useState(false);
  const [newApiToken, setNewApiToken] = useState('');
  const fileIdCounter = useRef(0);
  const videoRef = useRef(null);
  const imageRef = useRef(null);
  const audioRef = useRef(null);

  const canManageAll = session?.role === 'developer' || session?.role === 'admin';
  const canCreate = canManageAll;

  const loadItems = useCallback(async () => {
    try {
      setLoading(true);
      setLoadError('');
      const params = { page, page_size: 20 };
      if (search) params.search = search;
      if (status) params.status = status;
      const result = await api.bans(token, params);
      setData(result);
    } catch {
      setData(null);
      setLoadError('加载封禁数据失败，请稍后重试');
    } finally {
      setLoading(false);
    }
  }, [token, page, search, status]);

  React.useEffect(() => { loadItems(); }, [loadItems]);

  React.useEffect(() => {
    const prefill = location.state?.playerReportPrefill;
    if (!prefill || !canCreate) return;

    setModalMode('create');
    setEditingBanId(null);
    setForm(buildBanFormFromPlayerReport(prefill));
    setError('');
    setSelectedFiles([]);
    setUploadProgress(0);
    setSavePhase('idle');
    setReportReviewOnSave({
      reportId: prefill.reportId,
      player: prefill.player || prefill.steamId,
    });
    setOpen(true);
    navigate(location.pathname, { replace: true, state: null });
  }, [location.state, location.pathname, navigate, canCreate]);

  function refresh() {
    setRefreshKey((v) => v + 1);
    loadItems();
  }

  function resetBanModal() {
    setOpen(false);
    setModalMode('create');
    setEditingBanId(null);
    setForm(emptyBanForm);
    setError('');
    setSelectedFiles([]);
    setReportReviewOnSave(null);
    setUploadProgress(0);
    setSavePhase('idle');
  }

  function openCreateModal() {
    setModalMode('create');
    setEditingBanId(null);
    setForm(emptyBanForm);
    setError('');
    setSelectedFiles([]);
    setReportReviewOnSave(null);
    setOpen(true);
  }

  async function openEditModal(item) {
    setModalMode('edit');
    setEditingBanId(item.id);
    setError('');
    setSelectedFiles([]);
    setReportReviewOnSave(null);
    try {
      const result = await api.getBan(token, item.id);
      setForm(buildBanFormFromRecord(result.item ?? item));
      setOpen(true);
    } catch (requestError) {
      toast({ title: '加载失败', message: requestError.message || '加载封禁详情失败。', tone: 'danger' });
    }
  }

  function openRebanModal(item) {
    setModalMode('reban');
    setEditingBanId(null);
    setForm(buildBanFormFromRecord(item));
    setError('');
    setSelectedFiles([]);
    setReportReviewOnSave(null);
    setOpen(true);
  }

  async function loadApiKeys() {
    try {
      setApiKeysLoading(true);
      const result = await api.banApiKeys(token);
      setApiKeys(result.items ?? []);
    } catch (requestError) {
      toast({ title: '加载失败', message: requestError.message || '加载 API Key 失败。', tone: 'danger' });
    } finally {
      setApiKeysLoading(false);
    }
  }

  function openApiModal() {
    setApiModalOpen(true);
    setNewApiToken('');
    setApiKeyName('');
    loadApiKeys();
  }

  async function handleCreateApiKey() {
    if (!apiKeyName.trim()) {
      toast({ title: '创建失败', message: '请填写接入方名称。', tone: 'danger' });
      return;
    }
    try {
      setApiKeyCreating(true);
      const result = await api.createBanApiKey(token, { name: apiKeyName.trim() });
      setNewApiToken(result.token);
      setApiKeyName('');
      loadApiKeys();
      toast({ title: '创建成功', message: 'API Key 已生成，请立即保存。' });
    } catch (requestError) {
      toast({ title: '创建失败', message: requestError.message, tone: 'danger' });
    } finally {
      setApiKeyCreating(false);
    }
  }

  async function handleDeleteApiKey(item) {
    const confirmed = await confirm({
      title: '删除 API Key',
      message: `确定删除「${item.name}」吗？外部网站将立即无法继续接入。`,
      confirmText: '确认删除',
    });
    if (!confirmed) return;
    try {
      await api.deleteBanApiKey(token, item.id);
      loadApiKeys();
      toast({ title: '删除成功' });
    } catch (requestError) {
      toast({ title: '删除失败', message: requestError.message, tone: 'danger' });
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
    selectedFiles.forEach(({ file }) => {
      formData.append('files', file, file.name);
    });
    return uploadBanFilesWithProgress(banId, formData, token, setUploadProgress);
  }

  async function openDetailModal(item) {
    setDetailItem(item);
    setDetailLoading(true);
    setDetailFiles([]);
    setDetailFilesError('');
    setDetailFilesLoading(true);
    try {
      const [detailResult, fileResult] = await Promise.all([
        api.getBan(token, item.id),
        api.listBanFiles(token, item.id),
      ]);
      setDetailItem(detailResult.item ?? item);
      setDetailFiles(fileResult.files ?? []);
    } catch (requestError) {
      setDetailFilesError(requestError.message || '加载附件失败');
    } finally {
      setDetailLoading(false);
      setDetailFilesLoading(false);
    }
  }

  function canUnban(item) {
    return canManageAll || (session?.role === 'normal' && item.operator_name === session?.displayName);
  }

  async function handleUnban(item) {
    try {
      await api.unban(token, item.id);
      refresh();
      toast({ title: '解封成功', message: `${item.player || item.steam_id} 已解封。` });
    } catch (requestError) {
      toast({ title: '解封失败', message: requestError.message, tone: 'danger' });
    }
  }

  async function handleDeleteBan(item) {
    const confirmed = await confirm({
      title: '删除封禁记录',
      message: `确定删除 ${item.player || item.steam_id} 的封禁记录吗？`,
    });
    if (!confirmed) return;

    try {
      await api.deleteBan(token, item.id);
      refresh();
      toast({ title: '删除成功', message: `已删除 ${item.player || item.steam_id} 的封禁记录。` });
    } catch (requestError) {
      toast({ title: '删除失败', message: requestError.message, tone: 'danger' });
    }
  }

  async function handleSyncExternalBan(item) {
    try {
      setSyncingExternalId(item.id);
      const result = await api.syncExternalBan(token, item.id);
      toast({
        title: '同步成功',
        message: result.result?.message || `${item.player || item.steam_id} 已同步到外部封禁 API。`,
      });
    } catch (requestError) {
      toast({ title: '同步失败', message: requestError.message, tone: 'danger' });
    } finally {
      setSyncingExternalId(null);
    }
  }

  async function handleSaveBan() {
    const validationError = validateBanForm(form);
    if (validationError) {
      setError(validationError);
      return;
    }

    try {
      setSubmitting(true);
      setSavePhase('saving');
      setUploadProgress(0);
      setError('');
      const payload = buildCreateBanPayload(form);
      let savedItem = null;
      let uploadWarning = '';
      let uploadedFiles = false;
      if (modalMode === 'edit' && editingBanId) {
        const result = await api.updateBan(token, editingBanId, payload);
        savedItem = result.item;
      } else if (reportReviewOnSave?.reportId) {
        const result = await api.banPlayerReport(token, reportReviewOnSave.reportId, payload);
        savedItem = result.ban;
      } else {
        const result = await api.createBan(token, payload);
        savedItem = result.item;
      }
      if (modalMode !== 'edit' && savedItem?.id && selectedFiles.length > 0) {
        try {
          setSavePhase('uploading');
          await uploadSelectedFiles(savedItem.id);
          uploadedFiles = true;
        } catch (uploadError) {
          uploadWarning = uploadError.message || '辅助文件上传失败';
        }
      }
      resetBanModal();
      refresh();
      if (reportReviewOnSave?.reportId) {
        notifyPendingReviewsUpdated({ source: 'playerReport', action: 'ban' });
      }
      toast({
        title: modalMode === 'edit' ? '保存成功' : '添加成功',
        message: modalMode === 'edit'
          ? '封禁记录已更新。'
          : uploadedFiles
              ? '新封禁记录已添加，辅助文件已上传。'
              : uploadWarning
                ? `新封禁记录已添加，但辅助文件上传失败：${uploadWarning}`
                : reportReviewOnSave?.reportId
                  ? '新封禁记录已添加，玩家举报已标记为已封禁。'
                  : '新封禁记录已添加。',
      });
    } catch (requestError) {
      setError(requestError.message);
    } finally {
      setSubmitting(false);
      setSavePhase('idle');
    }
  }

  const items = data?.items ?? [];
  const total = data?.total ?? 0;

  return (
    <div id="ban" className="content-section active">
      <div className="breadcrumb"><span>核心管理</span><span className="sep">›</span><span className="current">封禁管理</span></div>
      <div className="page-header">
        <div>
          <div className="page-title">玩家封禁管理</div>
          <div className="page-sub">针对违规玩家进行账号或 IP 级别的封禁限制。</div>
        </div>
        <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap' }}>
          {canManageAll ? <button className="btn btn-outline" onClick={openApiModal}>API 接入</button> : null}
          {canCreate ? <button className="btn btn-danger" onClick={openCreateModal}>手动添加封禁</button> : null}
        </div>
      </div>

      <SearchBar
        value={search}
        onChange={(v) => { setSearch(v); setPage(1); }}
        placeholder="搜索 SteamID / 玩家名称..."
        statusOptions={[{ value: 'active', label: '生效中' }, { value: 'inactive', label: '已失效' }]}
        statusValue={status}
        onStatusChange={(v) => { setStatus(v); setPage(1); }}
      />

      <div className="card">
        <div className="card-body" className="p-0">
          <div className="table-responsive">
            <table className="data-table">
              <thead>
                <tr><th>玩家</th><th>SteamID64</th><th>封禁属性</th><th>封禁理由</th><th>时长 / 到期</th><th>状态</th><th>封禁时间</th><th className="text-right">操作</th></tr>
              </thead>
              <tbody>
                {loading ? <tr><td colSpan={8} style={{ textAlign: 'center', color: 'var(--text2)' }}>正在加载封禁数据...</td></tr> : null}
                {!loading && loadError ? <tr><td colSpan={8} style={{ textAlign: 'center', color: 'var(--danger)' }}>{loadError}</td></tr> : null}
                {!loading && items.map((x) => (
                  <tr key={x.id}>
                    <td>
                      <div className="fw-600">{x.player || '待自动获取'}</div>
                    </td>
                    <td className="steam-id">{x.steam_id}</td>
                    <td>{x.ban_type === 'ip' ? 'IP 封禁' : 'Steam 账号封禁'}</td>
                    <td style={{ color: 'var(--text2)', maxWidth: 260, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }} title={x.reason}>{x.reason}</td>
                    <td>
                      <div>{formatBanDuration(x.duration_minutes)}</div>
                      <div style={{ color: 'var(--text3)', fontSize: 12 }}>{formatExpiresAt(x.expires_at)}</div>
                    </td>
                    <td><span className={`status-pill ${x.status === 'active' ? 'pill-danger' : 'pill-offline'}`}>{x.status === 'active' ? '生效中' : '已失效'}</span></td>
                    <td className="text-muted-light">{formatChinaDateTime(x.created_at)}</td>
                    <td className="text-right">
                      <div className="action-btn-group">
                        <button className="action-btn" onClick={() => openDetailModal(x)}>详细</button>
                        {canManageAll ? <button className="action-btn action-btn-accent" onClick={() => openEditModal(x)}>编辑</button> : null}
                        {canManageAll && x.status === 'active' ? (
                          <button className="action-btn" onClick={() => handleSyncExternalBan(x)} disabled={syncingExternalId === x.id}>
                            {syncingExternalId === x.id ? '同步中' : '同步外部'}
                          </button>
                        ) : null}
                        {canUnban(x) && banRecordAction(x) === 'unban' ? <button className="action-btn action-btn-success" onClick={() => handleUnban(x)}>解封</button> : null}
                        {canCreate && banRecordAction(x) === 'reban' ? <button className="action-btn action-btn-danger" onClick={() => openRebanModal(x)}>重新封禁</button> : null}
                        {canManageAll ? <button className="action-btn action-btn-danger" onClick={() => handleDeleteBan(x)}>删除</button> : null}
                      </div>
                    </td>
                  </tr>
                ))}
                {!loading && !loadError && items.length === 0 ? <tr><td colSpan={8} style={{ textAlign: 'center', color: 'var(--text2)' }}>暂无封禁记录</td></tr> : null}
              </tbody>
            </table>
          </div>
        </div>
      </div>

      <Pagination page={page} pageSize={20} total={total} onChange={setPage} />

      {apiModalOpen ? (
        <div className="modal-overlay active" onClick={() => setApiModalOpen(false)}>
          <div className="modal" style={{ maxWidth: 780 }} onClick={(e) => e.stopPropagation()}>
            <div className="modal-header">
              <h2 style={{ fontSize: 16, fontWeight: 600, margin: 0 }}>封禁 API 接入</h2>
              <span style={{ cursor: 'pointer', color: 'var(--text3)', fontSize: 18 }} onClick={() => setApiModalOpen(false)}>&#10005;</span>
            </div>
            <div className="modal-body" style={{ display: 'grid', gap: 16 }}>
              <div style={{ display: 'grid', gridTemplateColumns: 'minmax(0, 1fr) auto', gap: 10 }}>
                <input
                  className="form-control"
                  value={apiKeyName}
                  onChange={(e) => setApiKeyName(e.target.value)}
                  placeholder="接入方名称，例如：合作网站 A"
                />
                <button className="btn btn-primary" onClick={handleCreateApiKey} disabled={apiKeyCreating}>
                  {apiKeyCreating ? '生成中...' : '生成 Key'}
                </button>
              </div>

              {newApiToken ? (
                <div className="card" style={{ margin: 0 }}>
                  <div className="card-body">
                    <div style={{ fontWeight: 600, marginBottom: 8 }}>新 API Key 只显示一次</div>
                    <code style={{ display: 'block', wordBreak: 'break-all', color: 'var(--accent)' }}>{newApiToken}</code>
                  </div>
                </div>
              ) : null}

              <div style={{ display: 'grid', gap: 8, fontSize: 13, color: 'var(--text2)' }}>
                <div>认证请求头：<code>X-API-Key: {'<API_KEY>'}</code></div>
                <div>查询封禁：<code>GET /api/integration/bans?page=1&page_size=20</code></div>
                <div>检查封禁：<code>POST /api/integration/bans/check</code>，Body: <code>{'{"steam_id":"7656119..."}'}</code></div>
                <div>创建封禁：<code>POST /api/integration/bans</code>，Body: <code>{'{"steam_id":"7656119...","ban_type":"steam","reason":"作弊","duration_minutes":0}'}</code></div>
              </div>

              <div className="table-responsive">
                <table className="data-table">
                  <thead>
                    <tr><th>名称</th><th>Key 前缀</th><th>最近使用</th><th>创建时间</th><th className="text-right">操作</th></tr>
                  </thead>
                  <tbody>
                    {apiKeysLoading ? <tr><td colSpan={5} style={{ textAlign: 'center', color: 'var(--text2)' }}>加载中...</td></tr> : null}
                    {!apiKeysLoading && apiKeys.length === 0 ? <tr><td colSpan={5} style={{ textAlign: 'center', color: 'var(--text2)' }}>暂无 API Key</td></tr> : null}
                    {apiKeys.map((item) => (
                      <tr key={item.id}>
                        <td className="fw-600">{item.name}</td>
                        <td className="steam-id">{item.token_prefix}...</td>
                        <td className="text-muted-light">{formatChinaDateTime(item.last_used_at)}</td>
                        <td className="text-muted-light">{formatChinaDateTime(item.created_at)}</td>
                        <td className="text-right">
                          <button className="action-btn action-btn-danger" onClick={() => handleDeleteApiKey(item)}>删除</button>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </div>
          </div>
        </div>
      ) : null}

      <Modal
        open={open}
        title={banModalTitle(modalMode)}
        onClose={resetBanModal}
        wide
        footer={<><button className="btn btn-outline" onClick={resetBanModal} disabled={submitting}>取消</button><button className="btn btn-danger" onClick={handleSaveBan} disabled={submitting}>{savePhase === 'uploading' ? `正在上传文件 (${uploadProgress}%)...` : banModalSubmitText(modalMode, submitting)}</button></>}
      >
        <div className="form-group"><label>玩家名称</label><input type="text" className="form-control" value={form.player} onChange={(event) => setForm((prev) => ({ ...prev, player: event.target.value }))} placeholder="留空后等待插件自动获取" /></div>
        <div className="form-group"><label>SteamID64 <span className="text-accent">*</span></label><input type="text" className="form-control" value={form.steam_id} onChange={(event) => setForm((prev) => ({ ...prev, steam_id: event.target.value }))} placeholder="76561198000000000" /></div>
        <div className="form-group"><label>封禁属性 <span className="text-accent">*</span></label><select className="form-control" value={form.ban_type} onChange={(event) => setForm((prev) => ({ ...prev, ban_type: event.target.value }))}><option value="">请选择封禁属性</option>{BAN_TYPE_OPTIONS.map((option) => <option key={option.value} value={option.value}>{option.label}</option>)}</select></div>
        <div className="form-group"><label>IP 地址</label><input type="text" className="form-control" value={form.ip_address} onChange={(event) => setForm((prev) => ({ ...prev, ip_address: event.target.value }))} placeholder="留空后等待插件自动获取" /></div>
        <div className="form-group"><label>封禁理由 <span className="text-accent">*</span></label><textarea className="form-control" value={form.reason} onChange={(event) => setForm((prev) => ({ ...prev, reason: event.target.value }))} placeholder="请输入封禁理由" rows={3} /></div>
        {modalMode !== 'edit' ? (
          <div className="form-section-card" className="mb-16">
            <div className="form-section-header"><span>辅助文件（选填）</span></div>
            <div className="form-hint" className="mb-12">可上传录像、截图或录音作为封禁依据，单个文件最大 100MB。</div>
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

      <Modal
        open={Boolean(detailItem)}
        title="封禁详细"
        onClose={() => setDetailItem(null)}
        footer={<button className="btn btn-outline" onClick={() => setDetailItem(null)}>关闭</button>}
      >
        {detailItem ? (
          <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
            <div className="form-group">
              <label className="mb-4">玩家信息</label>
              <div style={{ color: 'var(--text2)', fontSize: 13 }}>
                <div>名称：{detailItem.player || '待自动获取'}</div>
                <div>SteamID64：{detailItem.steam_id}</div>
                <div>IP 地址：{detailItem.ip_address || '待自动获取'}</div>
              </div>
            </div>

            <div className="form-group">
              <label className="mb-4">封禁信息</label>
              <div style={{ color: 'var(--text2)', fontSize: 13 }}>
                <div>封禁类型：{detailItem.ban_type === 'ip' ? 'IP 封禁' : 'Steam 账号封禁'}</div>
                <div>封禁时长：{formatBanDuration(detailItem.duration_minutes)}</div>
                <div>到期时间：{formatExpiresAt(detailItem.expires_at)}</div>
                <div>来源：{formatBanSource(detailItem.source, detailItem.operator_name)}</div>
                <div>操作人：{detailItem.operator_name}</div>
                <div>封禁时间：{detailLoading ? '正在加载完整信息...' : formatChinaDateTime(detailItem.created_at)}</div>
              </div>
            </div>

            <div className="form-group">
              <label className="mb-4">封禁理由</label>
              <div style={{ color: 'var(--text2)', fontSize: 13, whiteSpace: 'pre-wrap', background: 'var(--surface2)', padding: 8, borderRadius: 6 }}>
                {detailItem.reason}
              </div>
            </div>

            <div className="form-group">
              <label className="mb-4">处理状态</label>
              <div>
                <span className={`status-pill ${detailItem.status === 'active' ? 'pill-danger' : 'pill-offline'}`}>{detailItem.status === 'active' ? '生效中' : '已失效'}</span>
              </div>
              {detailItem.removed_reason ? (
                <div style={{ color: 'var(--text2)', fontSize: 13, marginTop: 4, whiteSpace: 'pre-wrap', background: 'var(--surface2)', padding: 8, borderRadius: 6 }}>
                  解封原因：{detailItem.removed_reason}
                </div>
              ) : null}
              {detailItem.removed_by ? (
                <div style={{ color: 'var(--text3)', fontSize: 12, marginTop: 4 }}>
                  由 {detailItem.removed_by} 于 {formatChinaDateTime(detailItem.removed_at)} 解封
                </div>
              ) : null}
            </div>

            <div className="form-group">
              <label className="mb-4">辅助文件</label>
              {detailFilesLoading ? <div style={{ color: 'var(--text3)', fontSize: 13, padding: '8px 0' }}>正在加载附件...</div> : null}
              {!detailFilesLoading && detailFilesError ? <div style={{ color: 'var(--danger)', fontSize: 13, padding: '8px 0' }}>{detailFilesError}</div> : null}
              {!detailFilesLoading && !detailFilesError && detailFiles.length === 0 ? <div style={{ color: 'var(--text3)', fontSize: 13, padding: '8px 0' }}>暂无辅助文件</div> : null}
              {!detailFilesLoading && detailFiles.length > 0 ? (
                <div style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
                  {detailFiles.map((file) => (
                    <div
                      key={file.id}
                      style={{
                        padding: '10px 14px',
                        background: 'var(--surface2)',
                        borderRadius: 10,
                        fontSize: 13,
                      }}
                    >
                      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: 12 }}>
                        <div style={{ display: 'flex', alignItems: 'center', gap: 8, minWidth: 0, flex: 1 }}>
                          <span className="status-pill">{fileCategoryLabel(file.category)}</span>
                          <div style={{ minWidth: 0 }}>
                            <div style={{ fontWeight: 500, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                              {file.file_name}
                            </div>
                            <div style={{ fontSize: 11, color: 'var(--text3)' }}>
                              {formatFileSize(file.file_size)}
                              {file.uploaded_by ? ` · ${file.uploaded_by}` : ''}
                              {file.uploaded_at ? ` · ${formatChinaDateTime(file.uploaded_at)}` : ''}
                            </div>
                          </div>
                        </div>
                        {file.url ? (
                          <a className="action-btn" href={file.url} target="_blank" rel="noreferrer" style={{ flexShrink: 0, textDecoration: 'none' }}>打开</a>
                        ) : (
                          <span style={{ fontSize: 11, color: 'var(--text3)', flexShrink: 0 }}>无下载链接</span>
                        )}
                      </div>
                    </div>
                  ))}
                </div>
              ) : null}
            </div>
          </div>
        ) : null}
      </Modal>
      {dialog}
      <ToastContainer toasts={toasts} onDismiss={dismissToast} />
    </div>
  );
}

import React, { useEffect, useState } from 'react';
import { api } from '../../lib/api.js';
import { useAuth } from '../../state/store.js';
import { formatBanDuration, formatBanSource, formatExpiresAt } from './banDisplay.js';
import { formatChinaDateTime } from '../../shared/time.js';
import { InternalNoteBadge } from '../../shared/InternalNote.jsx';

function formatFileSize(bytes) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

function fileCategoryLabel(category) {
  if (category === 'video') return '录像';
  if (category === 'image') return '截图';
  if (category === 'audio') return '录音';
  return '文件';
}

export function BanDetailModal({ open, item: initialItem, onClose, canManageAll }) {
  const { session } = useAuth();
  const token = session?.token ?? null;

  const [detailItem, setDetailItem] = useState(initialItem);
  const [detailLoading, setDetailLoading] = useState(false);
  const [detailFiles, setDetailFiles] = useState([]);
  const [detailFilesLoading, setDetailFilesLoading] = useState(false);
  const [detailFilesError, setDetailFilesError] = useState('');
  const [detailSyncStatus, setDetailSyncStatus] = useState([]);
  const [detailSyncLoading, setDetailSyncLoading] = useState(false);

  useEffect(() => {
    if (!open || !initialItem) return;

    let cancelled = false;
    (async () => {
      React.startTransition(() => {
        setDetailItem(initialItem);
        setDetailFiles([]);
        setDetailFilesError('');
        setDetailSyncStatus([]);
      });
      setDetailLoading(true);
      setDetailFilesLoading(true);
      setDetailSyncLoading(true);
      try {
        const [detailResult, fileResult, syncResult] = await Promise.all([
          api.getBan(token, initialItem.id),
          api.listBanFiles(token, initialItem.id),
          api.banSyncStatus(token, initialItem.id).catch(() => ({ items: [] })),
        ]);
        if (cancelled) return;
        setDetailItem(detailResult.item ?? initialItem);
        setDetailFiles(fileResult.files ?? []);
        setDetailSyncStatus(syncResult.items ?? []);
      } catch (requestError) {
        if (!cancelled) setDetailFilesError(requestError.message || '加载附件失败');
      } finally {
        if (!cancelled) {
          setDetailLoading(false);
          setDetailFilesLoading(false);
          setDetailSyncLoading(false);
        }
      }
    })();

    return () => { cancelled = true; };
  }, [open, initialItem, token]);

  if (!open || !detailItem) return null;

  return (
    <div className="modal-overlay active" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal-header">
          <h2 style={{ fontSize: 16, fontWeight: 600, margin: 0 }}>封禁详细</h2>
          <span style={{ cursor: 'pointer', color: 'var(--text3)', fontSize: 18 }} onClick={onClose}>&#10005;</span>
        </div>
        <div className="modal-body" style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
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

          {canManageAll && (detailSyncStatus.length > 0 || detailSyncLoading) ? (
            <div className="form-group">
              <label className="mb-4">外部同步状态</label>
              {detailSyncLoading ? <div style={{ color: 'var(--text3)', fontSize: 13, padding: '8px 0' }}>正在加载同步状态...</div> : null}
              {!detailSyncLoading && detailSyncStatus.length > 0 ? (
                <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
                  {detailSyncStatus.map((sync) => {
                    const statusMap = {
                      synced: { label: '已同步', cls: 'pill-online' },
                      failed: { label: '失败', cls: 'pill-offline' },
                      unsynced: { label: '已撤销', cls: 'pill-away' },
                      pending: { label: '待同步', cls: 'pill-idle' },
                    };
                    const info = statusMap[sync.status] || { label: sync.status, cls: 'pill-idle' };
                    return (
                      <div key={sync.target_id} style={{ display: 'flex', alignItems: 'center', gap: 8, fontSize: 13 }}>
                        <span>{sync.target_name}</span>
                        <span className={`status-pill ${info.cls}`}>{info.label}</span>
                        {sync.last_error ? <span style={{ color: 'var(--danger)', fontSize: 12 }} title={sync.last_error}>（{sync.last_error.length > 40 ? sync.last_error.slice(0, 40) + '...' : sync.last_error}）</span> : null}
                      </div>
                    );
                  })}
                </div>
              ) : null}
            </div>
          ) : null}

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
          <InternalNoteBadge steamid64={detailItem?.steam_id} />
        </div>
        <div className="modal-footer">
          <button className="btn btn-outline" onClick={onClose}>关闭</button>
        </div>
      </div>
    </div>
  );
}

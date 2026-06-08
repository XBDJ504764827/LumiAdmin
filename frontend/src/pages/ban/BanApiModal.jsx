import React, { useCallback, useState } from 'react';
import { api } from '../../lib/api.js';
import { useToast } from '../../shared/Toast.jsx';
import { useAuth } from '../../state/auth.jsx';
import { formatChinaDateTime } from '../../shared/time.js';

export function BanApiModal({ open, onClose }) {
  const { session } = useAuth();
  const { toast } = useToast();
  const token = session?.token ?? null;

  const [apiKeys, setApiKeys] = useState([]);
  const [apiKeysLoading, setApiKeysLoading] = useState(false);
  const [apiKeyName, setApiKeyName] = useState('');
  const [apiKeyCreating, setApiKeyCreating] = useState(false);
  const [newApiToken, setNewApiToken] = useState('');

  const loadApiKeys = useCallback(async () => {
    try {
      setApiKeysLoading(true);
      const result = await api.banApiKeys(token);
      setApiKeys(result.items ?? []);
    } catch (requestError) {
      toast({ title: '加载失败', message: requestError.message || '加载 API Key 失败。', tone: 'danger' });
    } finally {
      setApiKeysLoading(false);
    }
  }, [token, toast]);

  React.useEffect(() => {
    if (open) {
      React.startTransition(() => {
        setNewApiToken('');
        setApiKeyName('');
        loadApiKeys();
      });
    }
  }, [open, loadApiKeys]);

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
    try {
      await api.deleteBanApiKey(token, item.id);
      loadApiKeys();
      toast({ title: '删除成功' });
    } catch (requestError) {
      toast({ title: '删除失败', message: requestError.message, tone: 'danger' });
    }
  }

  if (!open) return null;

  return (
    <div className="modal-overlay active" onClick={onClose}>
      <div className="modal" style={{ maxWidth: 780 }} onClick={(e) => e.stopPropagation()}>
        <div className="modal-header">
          <h2 style={{ fontSize: 16, fontWeight: 600, margin: 0 }}>封禁 API 接入</h2>
          <span style={{ cursor: 'pointer', color: 'var(--text3)', fontSize: 18 }} onClick={onClose}>&#10005;</span>
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
  );
}

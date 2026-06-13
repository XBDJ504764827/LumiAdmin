import { useState } from 'react';
import { api } from '../../lib/api.js';
import { useApiQuery } from '../../shared/useApiQuery.js';
import { useAuth } from '../../state/auth.jsx';
import { formatChinaDateTime } from '../../shared/time.js';
import { StatusPill } from '../../shared/StatusPill.jsx';
import { Modal } from '../../shared/Modal.jsx';
import { TableLoading, TableEmpty } from '../../shared/TableState.jsx';

const BAN_TYPE_MAP = {
  bhop_hack: '连跳作弊',
  cheat: '作弊',
  tool_assist: '辅助工具',
  other: '其他',
};

function banTypeLabel(t) {
  return BAN_TYPE_MAP[t] || t || '-';
}

function isPermaBan(expiresOn) {
  return expiresOn && expiresOn.startsWith('9999');
}

function expiresLabel(expiresOn) {
  if (!expiresOn) return '-';
  if (isPermaBan(expiresOn)) return '永久';
  return formatChinaDateTime(expiresOn, { seconds: false });
}

// 封禁详情弹窗
function BanDetailModal({ ban, localBanId, manualUnbanned, onClose }) {
  return (
    <Modal
      open={true}
      title="全球封禁详情"
      onClose={onClose}
      wide
      footer={<button className="btn btn-outline" onClick={onClose}>关闭</button>}
    >
      <div className="detail-grid">
        <span className="detail-label">KZTimer ID</span>
        <span className="detail-value mono">{ban.id}</span>

        <span className="detail-label">玩家</span>
        <span className="detail-value fw-600">{ban.player_name || '-'}</span>

        <span className="detail-label">SteamID64</span>
        <span className="detail-value mono">{ban.steamid64}</span>

        <span className="detail-label">SteamID</span>
        <span className="detail-value mono">{ban.steam_id || '-'}</span>

        <div className="detail-divider" />

        <span className="detail-label">封禁类型</span>
        <span className="detail-value"><StatusPill kind="danger">{banTypeLabel(ban.ban_type)}</StatusPill></span>

        <span className="detail-label">到期时间</span>
        <span className="detail-value">
          {isPermaBan(ban.expires_on)
            ? <span className="permanent-ban">永久封禁</span>
            : expiresLabel(ban.expires_on)}
        </span>

        <span className="detail-label">备注</span>
        <span className="detail-value">{ban.notes || '-'}</span>

        {ban.stats && (
          <>
            <span className="detail-label">统计数据</span>
            <span className="detail-value pre">{ban.stats}</span>
          </>
        )}

        <div className="detail-divider" />

        <span className="detail-label">封禁时间</span>
        <span className="detail-value">{ban.created_on ? formatChinaDateTime(ban.created_on) : '-'}</span>

        <span className="detail-label">更新时间</span>
        <span className="detail-value">{ban.updated_on ? formatChinaDateTime(ban.updated_on) : '-'}</span>

        {ban.server_id != null && (
          <>
            <span className="detail-label">服务器 ID</span>
            <span className="detail-value">{ban.server_id}</span>
          </>
        )}

        <div className="detail-divider" />

        <span className="detail-label">本地封禁</span>
        <span className="detail-value">
          {manualUnbanned
            ? <StatusPill kind="default">管理员已解封</StatusPill>
            : localBanId
              ? <StatusPill kind="danger">已同步封禁</StatusPill>
              : <StatusPill kind="success">未同步封禁</StatusPill>}
        </span>
      </div>
    </Modal>
  );
}

export function GlobalBanPage() {
  const { session } = useAuth();
  const token = session?.token ?? null;
  const role = session?.role ?? '';
  const isDeveloper = role === 'developer';
  const [page, setPage] = useState(1);
  const [syncing, setSyncing] = useState(false);
  const [syncResult, setSyncResult] = useState(null);
  const [detailItem, setDetailItem] = useState(null);
  const pageSize = 20;

  const { data, isLoading, error, refetch } = useApiQuery(
    ['globalBans', page],
    (token) => api.globalBans(token, { page, page_size: pageSize }),
    typeof session !== 'undefined',
  );

  const items = data?.items || [];
  const hasMore = data?.has_more || false;
  const canPrev = page > 1;
  const canNext = hasMore;

  async function handleSync() {
    if (syncing) return;
    setSyncing(true);
    setSyncResult(null);
    try {
      const result = await api.syncGlobalBans(token);
      setSyncResult(result.result || result);
      refetch();
    } catch (e) {
      setSyncResult({ error: e.message });
    } finally {
      setSyncing(false);
    }
  }

  return (
    <div className="content-section active">
      <div className="breadcrumb">
        <span>日志与审计</span><span className="sep">›</span>
        <span className="current">全球封禁</span>
      </div>
      <div className="page-header">
        <div>
          <div className="page-title">全球封禁</div>
          <div className="page-sub">实时显示 KZTimer GlobalAPI 中的全球封禁记录，被全球封禁的玩家将自动被 LumiAdmin 封禁并拒绝进服。</div>
        </div>
        {isDeveloper && (
          <button className="btn btn-primary" onClick={handleSync} disabled={syncing}>
            {syncing ? '同步中...' : '手动同步'}
          </button>
        )}
      </div>

      {syncResult && (
        <div className={`sync-result-bar ${syncResult.error ? 'error' : 'success'}`}>
          {syncResult.error ? (
            <span>同步失败: {syncResult.error}</span>
          ) : (
            <span>同步完成 — 获取 {syncResult.total_fetched ?? 0} 条，新增 {syncResult.new_bans ?? 0} 条</span>
          )}
          <button className="btn btn-sm" onClick={() => setSyncResult(null)}>关闭</button>
        </div>
      )}

      <div className="card">
        <div className="card-body p-0">
          <div className="table-responsive">
            <table className="data-table">
              <thead>
                <tr>
                  <th>玩家</th>
                  <th>SteamID64</th>
                  <th>封禁类型</th>
                  <th>到期时间</th>
                  <th>备注</th>
                  <th>本地封禁</th>
                  <th>封禁时间</th>
                  <th>操作</th>
                </tr>
              </thead>
              <tbody>
                {isLoading ? (
                  <TableLoading colSpan={8} text="正在加载全球封禁列表..." />
                ) : error ? (
                  <tr><td colSpan={8} className="table-state-cell">
                    <div className="table-state-inner table-state-inner--error">加载失败: {error.message}</div>
                  </td></tr>
                ) : items.length === 0 ? (
                  <TableEmpty colSpan={8} text="暂无全球封禁记录" />
                ) : (
                  items.map((item) => {
                    const ban = item.ban;
                    return (
                      <tr key={ban.id}>
                        <td className="fw-600">{ban.player_name || '-'}</td>
                        <td className="steam-id">{ban.steamid64}</td>
                        <td><StatusPill kind="danger">{banTypeLabel(ban.ban_type)}</StatusPill></td>
                        <td style={{ whiteSpace: 'nowrap' }}>
                          {isPermaBan(ban.expires_on)
                            ? <span className="permanent-ban">永久</span>
                            : expiresLabel(ban.expires_on)}
                        </td>
                        <td style={{ maxWidth: 200, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }} title={ban.notes || ''}>
                          {ban.notes || '-'}
                        </td>
                        <td>
                          {item.manual_unbanned
                            ? <StatusPill kind="default">已解封</StatusPill>
                            : item.local_ban_id
                              ? <StatusPill kind="danger">已封禁</StatusPill>
                              : <StatusPill kind="success">未封禁</StatusPill>}
                        </td>
                        <td style={{ whiteSpace: 'nowrap' }}>
                          {ban.created_on ? formatChinaDateTime(ban.created_on, { seconds: false }) : '-'}
                        </td>
                        <td>
                          <button className="action-btn" onClick={() => setDetailItem(item)}>
                            详细
                          </button>
                        </td>
                      </tr>
                    );
                  })
                )}
              </tbody>
            </table>
          </div>
        </div>
      </div>

      {/* 分页：使用 has_more 实现上一页/下一页（KZTimer API 不返回 total） */}
      <div className="pagination">
        <button
          className="pagination-btn"
          disabled={!canPrev}
          onClick={() => setPage((p) => Math.max(1, p - 1))}
        >
          上一页
        </button>
        <span className="pagination-info">第 {page} 页</span>
        <button
          className="pagination-btn"
          disabled={!canNext}
          onClick={() => setPage((p) => p + 1)}
        >
          下一页
        </button>
      </div>

      {detailItem && (
        <BanDetailModal
          ban={detailItem.ban}
          localBanId={detailItem.local_ban_id}
          manualUnbanned={detailItem.manual_unbanned}
          onClose={() => setDetailItem(null)}
        />
      )}
    </div>
  );
}

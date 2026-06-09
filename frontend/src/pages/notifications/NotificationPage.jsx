import { useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { api } from '../../lib/api.js';
import { useApiQuery, useApiMutation } from '../../shared/useApiQuery.js';
import { Pagination } from '../../shared/Pagination.jsx';
import { EmptyBlock } from '../../shared/TableState.jsx';
import { notifyNotificationsUpdated } from '../../hooks/useNotifications.js';
import { formatChinaDateTime } from '../../shared/time.js';

const TYPE_LABELS = {
  whitelist_apply: '白名单申请',
  ban_create: '封禁记录',
  plugin_ban: '插件封禁',
};

const TYPE_ICONS = {
  whitelist_apply: (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" width="20" height="20">
      <path d="M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2" /><circle cx="9" cy="7" r="4" /><line x1="19" y1="8" x2="19" y2="14" /><line x1="22" y1="11" x2="16" y2="11" />
    </svg>
  ),
  ban_create: (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" width="20" height="20">
      <circle cx="12" cy="12" r="10" /><line x1="4.93" y1="4.93" x2="19.07" y2="19.07" />
    </svg>
  ),
  plugin_ban: (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" width="20" height="20">
      <rect x="2" y="2" width="20" height="8" rx="2" ry="2" /><rect x="2" y="14" width="20" height="8" rx="2" ry="2" /><line x1="6" y1="6" x2="6.01" y2="6" /><line x1="6" y1="18" x2="6.01" y2="18" />
    </svg>
  ),
};

const TYPE_PILL_CLASS = {
  whitelist_apply: 'pill-online',
  ban_create: 'pill-accent',
  plugin_ban: 'pill-default',
};

export function NotificationPage() {
  const navigate = useNavigate();

  const [page, setPage] = useState(1);
  const [typeFilter, setTypeFilter] = useState('');
  const [readFilter, setReadFilter] = useState('');

  const { data, isLoading, error, refetch: _refetch } = useApiQuery(
    ['notifications', { page }],
    (token) => api.notifications(token, { page, page_size: 20 }),
  );

  const markReadMutation = useApiMutation(
    ({ token, id }) => api.markNotificationRead(token, id),
    { invalidateQueries: ['notifications'] },
  );

  const markAllReadMutation = useApiMutation(
    ({ token }) => api.markAllNotificationsRead(token),
    { invalidateQueries: ['notifications'] },
  );

  const items = data?.items ?? [];
  const total = data?.total ?? 0;

  const filtered = items.filter((n) => {
    if (typeFilter && n.type !== typeFilter) return false;
    if (readFilter === 'unread' && n.read) return false;
    if (readFilter === 'read' && !n.read) return false;
    return true;
  });

  const hasUnread = items.some(n => !n.read);

  async function handleMarkRead(id) {
    try {
      await markReadMutation.mutateAsync({ id });
      notifyNotificationsUpdated({ action: 'mark_read', id });
    } catch {}
  }

  async function handleMarkAllRead() {
    try {
      await markAllReadMutation.mutateAsync({});
      notifyNotificationsUpdated({ action: 'mark_all_read' });
    } catch {}
  }

  function handleClick(n) {
    if (!n.read) handleMarkRead(n.id);
    if (n.link) navigate(n.link);
  }

  return (
    <div id="notifications" className="content-section active">
      <div className="breadcrumb"><span>系统功能</span><span className="sep">›</span><span className="current">通知中心</span></div>
      <div className="page-header">
        <div>
          <div className="page-title">通知中心</div>
          <div className="page-sub">查看白名单申请、封禁记录等站内通知。</div>
        </div>
        {hasUnread && (
          <button className="btn btn-secondary" type="button" onClick={handleMarkAllRead}>
            全部标记已读
          </button>
        )}
      </div>

      <div className="card">
        <div className="card-header">
          <div>
            <div className="card-title">通知列表</div>
            <div className="card-sub">共 {total} 条通知</div>
          </div>
        </div>
        <div className="card-body p-0">
          <div className="filter-bar">
            <select
              className="filter-select"
              value={typeFilter}
              onChange={(e) => { setTypeFilter(e.target.value); setPage(1); }}
            >
              <option value="">全部类型</option>
              <option value="whitelist_apply">白名单申请</option>
              <option value="ban_create">封禁记录</option>
              <option value="plugin_ban">插件封禁</option>
            </select>
            <select
              className="filter-select"
              value={readFilter}
              onChange={(e) => { setReadFilter(e.target.value); setPage(1); }}
            >
              <option value="">全部状态</option>
              <option value="unread">未读</option>
              <option value="read">已读</option>
            </select>
          </div>

          {isLoading ? <div className="p-20">正在加载通知...</div> : null}
          {!isLoading && error ? <div style={{ padding: 20, color: 'var(--accent)' }}>{error.message}</div> : null}
          {!isLoading && !error ? (
            <div className="notification-page-list">
              {filtered.length === 0 ? (
                <EmptyBlock>暂无通知</EmptyBlock>
              ) : null}
              {filtered.map((n) => (
                <button
                  key={n.id}
                  type="button"
                  className={`notification-page-item ${n.read ? '' : 'unread'}`}
                  onClick={() => handleClick(n)}
                >
                  <div className="notification-page-item-icon">
                    {TYPE_ICONS[n.type] || TYPE_ICONS.ban_create}
                  </div>
                  <div className="notification-page-item-body">
                    <div className="notification-page-item-row">
                      <div className="notification-page-item-tags">
                        <span className={`status-pill ${TYPE_PILL_CLASS[n.type] || 'pill-default'}`}>
                          {TYPE_LABELS[n.type] || n.type}
                        </span>
                        <span className={`status-pill ${n.read ? 'pill-default' : 'pill-accent'}`}>
                          {n.read ? '已读' : '未读'}
                        </span>
                      </div>
                      <span className="notification-page-item-time">{formatChinaDateTime(n.created_at)}</span>
                    </div>
                    <div className="notification-page-item-title">{n.title}</div>
                    <div className="notification-page-item-message">{n.message}</div>
                  </div>
                </button>
              ))}
            </div>
          ) : null}
        </div>
      </div>

      <Pagination page={page} pageSize={20} total={total} onChange={setPage} />
    </div>
  );
}

import React, { useState, useRef, useEffect } from 'react';
import { useNavigate } from 'react-router-dom';
import { useNotifications } from '../hooks/useNotifications.js';
import { formatChinaDate } from '../shared/time.js';

const TYPE_ICONS = {
  whitelist_apply: (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" width="18" height="18">
      <path d="M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2" /><circle cx="9" cy="7" r="4" /><line x1="19" y1="8" x2="19" y2="14" /><line x1="22" y1="11" x2="16" y2="11" />
    </svg>
  ),
  ban_create: (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" width="18" height="18">
      <circle cx="12" cy="12" r="10" /><line x1="4.93" y1="4.93" x2="19.07" y2="19.07" />
    </svg>
  ),
  plugin_ban: (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" width="18" height="18">
      <rect x="2" y="2" width="20" height="8" rx="2" ry="2" /><rect x="2" y="14" width="20" height="8" rx="2" ry="2" /><line x1="6" y1="6" x2="6.01" y2="6" /><line x1="6" y1="18" x2="6.01" y2="18" />
    </svg>
  ),
};

function timeAgo(dateStr) {
  const now = Date.now();
  const then = new Date(dateStr).getTime();
  const diff = Math.max(0, now - then);
  const minutes = Math.floor(diff / 60000);
  if (minutes < 1) return '刚刚';
  if (minutes < 60) return `${minutes} 分钟前`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours} 小时前`;
  const days = Math.floor(hours / 24);
  if (days < 30) return `${days} 天前`;
  return formatChinaDate(dateStr);
}

export function NotificationBell() {
  const [open, setOpen] = useState(false);
  const panelRef = useRef(null);
  const navigate = useNavigate();
  const { unreadCount, notifications, loading, fetchNotifications, markRead, markAllRead } = useNotifications();

  useEffect(() => {
    function handleClickOutside(e) {
      if (panelRef.current && !panelRef.current.contains(e.target)) {
        setOpen(false);
      }
    }
    if (open) document.addEventListener('mousedown', handleClickOutside);
    return () => document.removeEventListener('mousedown', handleClickOutside);
  }, [open]);

  useEffect(() => {
    if (open && notifications.length === 0) {
      fetchNotifications();
    }
  }, [open, notifications.length, fetchNotifications]);

  function handleNotificationClick(n) {
    if (!n.read) markRead(n.id);
    setOpen(false);
    if (n.link) navigate(n.link);
  }

  return (
    <div className="notification-bell" ref={panelRef}>
      <button
        className="icon-btn notification-toggle"
        type="button"
        onClick={() => setOpen((v) => !v)}
        title="通知"
      >
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
          <path d="M18 8A6 6 0 0 0 6 8c0 7-3 9-3 9h18s-3-2-3-9" />
          <path d="M13.73 21a2 2 0 0 1-3.46 0" />
        </svg>
        {unreadCount > 0 && (
          <span className="notification-badge">
            {unreadCount > 99 ? '99+' : unreadCount}
          </span>
        )}
      </button>

      {open && (
        <div className="notification-panel">
          <div className="notification-panel-header">
            <span>通知</span>
            {unreadCount > 0 && (
              <button type="button" className="notification-mark-all" onClick={markAllRead}>
                全部已读
              </button>
            )}
          </div>
          <div className="notification-panel-body">
            {loading && notifications.length === 0 ? (
              <div className="notification-empty">加载中...</div>
            ) : notifications.length === 0 ? (
              <div className="notification-empty">暂无通知</div>
            ) : (
              notifications.map((n) => (
                <button
                  key={n.id}
                  type="button"
                  className={`notification-item ${n.read ? '' : 'unread'}`}
                  onClick={() => handleNotificationClick(n)}
                >
                  <div className="notification-item-icon">
                    {TYPE_ICONS[n.type] || TYPE_ICONS.ban_create}
                  </div>
                  <div className="notification-item-content">
                    <div className="notification-item-title">{n.title}</div>
                    <div className="notification-item-message">{n.message}</div>
                    <div className="notification-item-time">{timeAgo(n.created_at)}</div>
                  </div>
                  {!n.read && <div className="notification-item-dot" />}
                </button>
              ))
            )}
          </div>
          <div className="notification-panel-footer">
            <button type="button" className="notification-view-all" onClick={() => { setOpen(false); navigate('/notifications'); }}>
              查看全部通知
            </button>
          </div>
        </div>
      )}
    </div>
  );
}

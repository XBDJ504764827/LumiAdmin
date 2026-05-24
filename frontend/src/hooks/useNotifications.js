import { useState, useEffect, useCallback, useRef } from 'react';
import { api } from '../lib/api.js';
import { useAuth } from '../state/auth.jsx';

const WS_BASE = (import.meta.env.VITE_WS_BASE ?? '').replace(/^http/, 'ws') ||
  `${window.location.protocol === 'https:' ? 'wss:' : 'ws:'}//${window.location.host}`;

export function useNotifications() {
  const { session } = useAuth();
  const [unreadCount, setUnreadCount] = useState(0);
  const [notifications, setNotifications] = useState([]);
  const [loading, setLoading] = useState(false);
  const wsRef = useRef(null);
  const reconnectTimer = useRef(null);
  const reconnectAttempts = useRef(0);

  const token = session?.token ?? null;

  const fetchUnreadCount = useCallback(async () => {
    if (!token) return;
    try {
      const data = await api.notificationUnreadCount(token);
      setUnreadCount(data.count);
    } catch {}
  }, [token]);

  const fetchNotifications = useCallback(async (page = 1) => {
    if (!token) return;
    setLoading(true);
    try {
      const data = await api.notifications(token, { page, page_size: 20 });
      setNotifications(data.items);
    } catch {} finally {
      setLoading(false);
    }
  }, [token]);

  const markRead = useCallback(async (id) => {
    if (!token) return;
    try {
      await api.markNotificationRead(token, id);
      setNotifications(prev => prev.map(n => n.id === id ? { ...n, read: true } : n));
      setUnreadCount(prev => Math.max(0, prev - 1));
    } catch {}
  }, [token]);

  const markAllRead = useCallback(async () => {
    if (!token) return;
    try {
      await api.markAllNotificationsRead(token);
      setNotifications(prev => prev.map(n => ({ ...n, read: true })));
      setUnreadCount(0);
    } catch {}
  }, [token]);

  useEffect(() => {
    if (!token) return;

    let alive = true;
    reconnectAttempts.current = 0;

    function connect() {
      if (!alive) return;
      const ws = new WebSocket(`${WS_BASE}/ws/notifications?token=${token}`);
      wsRef.current = ws;

      ws.onopen = () => {
        reconnectAttempts.current = 0;
      };

      ws.onmessage = (event) => {
        try {
          const msg = JSON.parse(event.data);
          if (msg.type === 'notification' && msg.data) {
            setNotifications(prev => [msg.data, ...prev].slice(0, 50));
            setUnreadCount(prev => prev + 1);
          }
        } catch {}
      };

      ws.onclose = () => {
        if (!alive) return;
        const delay = Math.min(1000 * Math.pow(2, reconnectAttempts.current), 30000);
        reconnectAttempts.current++;
        reconnectTimer.current = setTimeout(connect, delay);
      };

      ws.onerror = () => {
        ws.close();
      };
    }

    connect();
    fetchUnreadCount();

    return () => {
      alive = false;
      if (wsRef.current) wsRef.current.close();
      if (reconnectTimer.current) clearTimeout(reconnectTimer.current);
    };
  }, [token, fetchUnreadCount]);

  return { unreadCount, notifications, loading, fetchNotifications, markRead, markAllRead, fetchUnreadCount };
}

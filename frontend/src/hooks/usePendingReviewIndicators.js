import React, { createContext, createElement, useCallback, useContext, useEffect, useMemo, useRef, useState } from 'react';
import { api } from '../lib/api.js';
import { useAuth } from '../state/auth.jsx';

const WS_BASE = (import.meta.env.VITE_WS_BASE ?? '').replace(/^http/, 'ws') ||
  `${window.location.protocol === 'https:' ? 'wss:' : 'ws:'}//${window.location.host}`;

export const PENDING_REVIEWS_UPDATED_EVENT = 'manger:pending-reviews-updated';

const EMPTY_COUNTS = {
  whitelist: 0,
  banAppeal: 0,
  playerReport: 0,
};

const PendingReviewContext = createContext({
  counts: EMPTY_COUNTS,
  loading: false,
  refresh: () => {},
});

export function notifyPendingReviewsUpdated(detail = {}) {
  if (typeof window === 'undefined') return;
  window.dispatchEvent(new CustomEvent(PENDING_REVIEWS_UPDATED_EVENT, { detail }));
}

export function usePendingReviewData() {
  const { session } = useAuth();
  const token = session?.token ?? null;
  const role = session?.role;
  const [counts, setCounts] = useState(EMPTY_COUNTS);
  const [loading, setLoading] = useState(false);
  const wsRef = useRef(null);
  const reconnectTimer = useRef(null);
  const reconnectAttempts = useRef(0);

  const canReviewWhitelist = ['developer', 'admin', 'normal'].includes(role);
  const canReviewReports = ['developer', 'admin'].includes(role);

  const refresh = useCallback(async () => {
    if (!token) {
      setCounts(EMPTY_COUNTS);
      return;
    }

    setLoading(true);
    try {
      const result = await api.reviewCounts(token);
      setCounts({
        whitelist: canReviewWhitelist ? Number(result?.whitelist ?? 0) : 0,
        banAppeal: canReviewReports ? Number(result?.ban_appeal ?? 0) : 0,
        playerReport: canReviewReports ? Number(result?.player_report ?? 0) : 0,
      });
    } catch {
      setCounts(EMPTY_COUNTS);
    } finally {
      setLoading(false);
    }
  }, [token, canReviewWhitelist, canReviewReports]);

  useEffect(() => {
    React.startTransition(() => { refresh(); });
  }, [refresh]);

  useEffect(() => {
    if (!token) return undefined;

    const handleUpdated = () => refresh();
    const handleFocus = () => refresh();

    window.addEventListener(PENDING_REVIEWS_UPDATED_EVENT, handleUpdated);
    window.addEventListener('focus', handleFocus);
    return () => {
      window.removeEventListener(PENDING_REVIEWS_UPDATED_EVENT, handleUpdated);
      window.removeEventListener('focus', handleFocus);
    };
  }, [token, refresh]);

  useEffect(() => {
    if (!token) return undefined;

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
          const notificationType = msg?.data?.type ?? msg?.data?.notification_type;
          if (msg.type === 'notification' && ['whitelist_apply', 'ban_appeal', 'player_report'].includes(notificationType)) {
            refresh();
          }
        } catch {}
      };

      ws.onclose = () => {
        if (!alive) return;
        const delay = Math.min(1000 * Math.pow(2, reconnectAttempts.current), 30000);
        reconnectAttempts.current += 1;
        reconnectTimer.current = setTimeout(connect, delay);
      };

      ws.onerror = () => {
        ws.close();
      };
    }

    connect();
    const interval = window.setInterval(refresh, 60000);

    return () => {
      alive = false;
      window.clearInterval(interval);
      if (wsRef.current) wsRef.current.close();
      if (reconnectTimer.current) window.clearTimeout(reconnectTimer.current);
    };
  }, [token, refresh]);

  return useMemo(() => ({ counts, loading, refresh }), [counts, loading, refresh]);
}

export function PendingReviewProvider({ value, children }) {
  return createElement(PendingReviewContext.Provider, { value }, children);
}

export function usePendingReviewIndicators() {
  return useContext(PendingReviewContext);
}

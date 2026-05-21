import React, { useCallback, useEffect, useRef, useState } from 'react';

let toastId = 0;

export function useToast() {
  const [toasts, setToasts] = useState([]);

  const show = useCallback((options) => {
    const id = ++toastId;
    const toast = {
      id,
      title: options.title ?? '提示',
      message: options.message ?? '',
      tone: options.tone ?? 'success',
      duration: options.duration ?? 3000,
    };
    setToasts((prev) => [...prev, toast]);
    return id;
  }, []);

  const dismiss = useCallback((id) => {
    setToasts((prev) => prev.map((t) => (t.id === id ? { ...t, exiting: true } : t)));
    setTimeout(() => {
      setToasts((prev) => prev.filter((t) => t.id !== id));
    }, 300);
  }, []);

  const toast = useCallback((options) => {
    const id = show(options);
    const duration = options.duration ?? 3000;
    setTimeout(() => dismiss(id), duration);
    return id;
  }, [show, dismiss]);

  return { toast, toasts, dismiss };
}

function ToastItem({ data, onDismiss }) {
  const timerRef = useRef(null);
  const pausedRef = useRef(false);
  const remainingRef = useRef(data.duration);
  const startRef = useRef(Date.now());

  useEffect(() => {
    timerRef.current = setTimeout(() => onDismiss(data.id), data.duration);
    return () => {
      if (timerRef.current) clearTimeout(timerRef.current);
    };
  }, [data.duration, data.id, onDismiss]);

  function handleMouseEnter() {
    if (timerRef.current) clearTimeout(timerRef.current);
    pausedRef.current = true;
    remainingRef.current -= Date.now() - startRef.current;
  }

  function handleMouseLeave() {
    pausedRef.current = false;
    startRef.current = Date.now();
    timerRef.current = setTimeout(() => onDismiss(data.id), remainingRef.current);
  }

  const toneClass = data.tone === 'danger' ? 'toast-danger' : 'toast-success';
  const icon = data.tone === 'danger' ? '✕' : '✓';

  return (
    <div
      className={`toast-item ${toneClass}${data.exiting ? ' toast-exiting' : ''}`}
      onMouseEnter={handleMouseEnter}
      onMouseLeave={handleMouseLeave}
    >
      <span className="toast-icon">{icon}</span>
      <div className="toast-content">
        <div className="toast-title">{data.title}</div>
        {data.message ? <div className="toast-message">{data.message}</div> : null}
      </div>
      <button className="toast-close" onClick={() => onDismiss(data.id)}>✕</button>
    </div>
  );
}

export function ToastContainer({ toasts, onDismiss }) {
  if (!toasts.length) return null;
  return (
    <div className="toast-container">
      {toasts.map((t) => <ToastItem key={t.id} data={t} onDismiss={onDismiss} />)}
    </div>
  );
}

import { createContext, useCallback, useContext, useRef, useState } from 'react';

let toastId = 0;

const ToastContext = createContext(null);

/**
 * Toast Provider：在应用根节点渲染一次，持有唯一的 toast 队列。
 * 子组件通过 useToast() 消费，调用 toast() 即可全局弹出通知。
 */
export function ToastProvider({ children }) {
  const [toasts, setToasts] = useState([]);

  const dismiss = useCallback((id) => {
    setToasts((prev) => prev.map((t) => (t.id === id ? { ...t, exiting: true } : t)));
    setTimeout(() => {
      setToasts((prev) => prev.filter((t) => t.id !== id));
    }, 300);
  }, []);

  const toast = useCallback((options) => {
    const id = ++toastId;
    const item = {
      id,
      title: options.title ?? '提示',
      message: options.message ?? '',
      tone: options.tone ?? 'success',
      duration: options.duration ?? 3000,
    };
    setToasts((prev) => [...prev, item]);
    setTimeout(() => dismiss(id), item.duration);
    return id;
  }, [dismiss]);

  const value = { toast, dismiss };

  return (
    <ToastContext.Provider value={value}>
      {children}
      <ToastContainer toasts={toasts} onDismiss={dismiss} />
    </ToastContext.Provider>
  );
}

/**
 * 消费全局 toast。返回 { toast, dismiss }。
 * toast(options) 弹出通知；dismiss(id) 手动关闭。
 */
export function useToast() {
  const ctx = useContext(ToastContext);
  if (!ctx) {
    throw new Error('useToast 必须在 <ToastProvider> 内部使用');
  }
  return { toast: ctx.toast, dismiss: ctx.dismiss };
}

function ToastItem({ data, onDismiss }) {
  const timerRef = useRef(null);
  const pausedRef = useRef(false);
  const remainingRef = useRef(data.duration);
  const startRef = useRef(0);

  const onMouseEnter = () => {
    if (timerRef.current) clearTimeout(timerRef.current);
    pausedRef.current = true;
    remainingRef.current -= Date.now() - startRef.current;
  };

  const onMouseLeave = () => {
    pausedRef.current = false;
    startRef.current = Date.now();
    timerRef.current = setTimeout(() => onDismiss(data.id), remainingRef.current);
  };

  const toneClass = data.tone === 'danger' ? 'toast-danger' : data.tone === 'warning' ? 'toast-warning' : 'toast-success';
  const icon = data.tone === 'danger' ? '✕' : data.tone === 'warning' ? '!' : '✓';

  return (
    <div
      className={`toast-item ${toneClass}${data.exiting ? ' toast-exiting' : ''}`}
      onMouseEnter={onMouseEnter}
      onMouseLeave={onMouseLeave}
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

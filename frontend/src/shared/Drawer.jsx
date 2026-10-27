import { useEffect, useRef } from 'react';

/**
 * 右侧抽屉。适合承载信息量大、需要保留页面上下文的详情/审核面板。
 */
export function Drawer({
  open,
  title,
  subtitle,
  children,
  footer,
  onClose,
  width,
  className = '',
}) {
  const panelRef = useRef(null);

  useEffect(() => {
    if (!open) return undefined;
    const onKeyDown = (event) => {
      if (event.key === 'Escape') onClose?.();
    };
    window.addEventListener('keydown', onKeyDown);
    panelRef.current?.focus();
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [open, onClose]);

  if (!open) return null;

  return (
    <div className="ui-drawer-overlay" onClick={onClose}>
      <div
        ref={panelRef}
        className={`ui-drawer ${className}`.trim()}
        style={width ? { width: `min(${width}px, 96vw)` } : undefined}
        role="dialog"
        aria-modal="true"
        aria-labelledby="ui-drawer-title"
        tabIndex={-1}
        onClick={(event) => event.stopPropagation()}
      >
        <div className="ui-drawer-header">
          <div>
            <h2 id="ui-drawer-title">{title}</h2>
            {subtitle ? <div className="card-sub">{subtitle}</div> : null}
          </div>
          <button type="button" className="modal-close-btn" onClick={onClose} aria-label="关闭抽屉">
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <line x1="18" y1="6" x2="6" y2="18" />
              <line x1="6" y1="6" x2="18" y2="18" />
            </svg>
          </button>
        </div>
        <div className="ui-drawer-body">{children}</div>
        {footer ? <div className="ui-drawer-footer">{footer}</div> : null}
      </div>
    </div>
  );
}

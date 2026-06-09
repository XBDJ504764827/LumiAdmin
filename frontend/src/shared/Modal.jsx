
export function Modal({ open, title, subtitle, children, footer, onClose, wide = false, extraWide = false }) {
  if (!open) return null;
  const widthStyle = extraWide ? { maxWidth: 920 } : wide ? { maxWidth: 640 } : undefined;
  return (
    <div className="modal-overlay active" onClick={onClose}>
      <div className="modal" style={widthStyle} onClick={(e) => e.stopPropagation()}>
        <div className="modal-header">
          <div>
            <h2 style={{ fontSize: 16, fontWeight: 600, margin: 0 }}>{title}</h2>
            {subtitle ? <div className="card-sub">{subtitle}</div> : null}
          </div>
          <button className="modal-close-btn" type="button" onClick={onClose} aria-label="关闭">
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <line x1="18" y1="6" x2="6" y2="18" />
              <line x1="6" y1="6" x2="18" y2="18" />
            </svg>
          </button>
        </div>
        <div className="modal-body">{children}</div>
        {footer ? <div className="modal-footer">{footer}</div> : null}
      </div>
    </div>
  );
}

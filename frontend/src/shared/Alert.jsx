/**
 * 统一提示条。复用现有 alert 样式，提供一致的语义、图标与可选关闭按钮。
 *
 * tone: info | success | warning | danger
 */
const ICONS = {
  info: 'ℹ',
  success: '✓',
  warning: '⚠',
  danger: '✕',
};

export function Alert({
  tone = 'info',
  title,
  children,
  icon,
  closable = false,
  onClose,
  className = '',
  role,
}) {
  const resolvedTone = tone === 'error' ? 'danger' : tone;
  // 现有样式类名为 alert-error（没有 alert-danger），这里映射到已有样式
  const toneClass = resolvedTone === 'danger' ? 'error' : resolvedTone;
  const level = resolvedTone === 'danger' || resolvedTone === 'warning' ? 'alert' : 'status';
  return (
    <div
      className={`alert alert-${toneClass} ${className}`.trim()}
      role={role || level}
    >
      <span className="alert-icon" aria-hidden="true">{icon ?? ICONS[resolvedTone] ?? ICONS.info}</span>
      <div className="alert-content">
        {title ? <div className="alert-title">{title}</div> : null}
        {children ? <div className="alert-text">{children}</div> : null}
      </div>
      {closable ? (
        <button type="button" className="alert-close" onClick={onClose} aria-label="关闭提示">✕</button>
      ) : null}
    </div>
  );
}

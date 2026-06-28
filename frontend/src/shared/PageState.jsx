import { IconAlertTriangle, IconRefresh } from './Icons.jsx';

export function PageState({
  tone = 'muted',
  title,
  message,
  action,
  actionText = '重试',
  icon,
  className = '',
}) {
  const Icon = icon || (tone === 'danger' ? IconAlertTriangle : null);

  return (
    <div className={`page-state page-state-${tone} ${className}`.trim()}>
      {Icon ? (
        <div className="page-state-icon">
          <Icon size={24} />
        </div>
      ) : null}
      {title ? <div className="page-state-title">{title}</div> : null}
      {message ? <div className="page-state-message">{message}</div> : null}
      {action ? (
        <button className="btn btn-outline" type="button" onClick={action}>
          <IconRefresh size={14} />
          {actionText}
        </button>
      ) : null}
    </div>
  );
}

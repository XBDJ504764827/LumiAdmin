import { Children, useEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';

/**
 * 行内二次确认气泡。适用于表格行、卡片操作区等「轻量但不可逆」的操作，
 * 避免为每次删除/撤销都打开一个大 Modal。
 *
 * 用法：
 *   <Popconfirm title="删除记录" description="删除后不可恢复" onConfirm={...}>
 *     <button>删除</button>
 *   </Popconfirm>
 */
export function Popconfirm({
  title,
  description,
  children,
  onConfirm,
  confirmText = '确认',
  cancelText = '取消',
  tone = 'danger',
  placement = 'bottom',
  width = 300,
  disabled = false,
}) {
  const anchorRef = useRef(null);
  const popRef = useRef(null);
  const [open, setOpen] = useState(false);
  const [position, setPosition] = useState({ top: 0, left: 0, width });
  const [working, setWorking] = useState(false);
  const child = Children.only(children);

  useEffect(() => {
    if (!open) return undefined;

    function updatePosition() {
      const anchor = anchorRef.current;
      if (!anchor) return;
      const rect = anchor.getBoundingClientRect();
      const gap = 8;
      const left = Math.min(
        Math.max(8, rect.right - width),
        Math.max(8, window.innerWidth - width - 8),
      );
      const top = placement === 'top'
        ? Math.max(8, rect.top - gap)
        : Math.min(window.innerHeight - 8, rect.bottom + gap);
      setPosition({ top, left, width });
    }

    updatePosition();
    const close = (event) => {
      if (
        anchorRef.current?.contains(event.target)
        || popRef.current?.contains(event.target)
      ) return;
      setOpen(false);
    };
    const onKeyDown = (event) => {
      if (event.key === 'Escape') setOpen(false);
    };
    window.addEventListener('resize', updatePosition);
    window.addEventListener('scroll', updatePosition, true);
    window.addEventListener('mousedown', close);
    window.addEventListener('keydown', onKeyDown);
    return () => {
      window.removeEventListener('resize', updatePosition);
      window.removeEventListener('scroll', updatePosition, true);
      window.removeEventListener('mousedown', close);
      window.removeEventListener('keydown', onKeyDown);
    };
  }, [open, placement, width]);

  async function handleConfirm() {
    if (working) return;
    try {
      setWorking(true);
      await onConfirm?.();
      setOpen(false);
    } finally {
      setWorking(false);
    }
  }

  function handleAnchorClick(event) {
    if (disabled) return;
    event.preventDefault();
    event.stopPropagation();
    setOpen((value) => !value);
  }

  return (
    <>
      <span
        ref={anchorRef}
        className={`ui-popconfirm-anchor${disabled ? ' is-disabled' : ''}`}
        onClick={handleAnchorClick}
        aria-haspopup="dialog"
        aria-expanded={open}
      >
        {child}
      </span>
      {open ? createPortal(
        <div
          ref={popRef}
          className={`ui-popconfirm ui-popconfirm-${tone}`}
          style={{
            top: position.top,
            left: position.left,
            width: position.width,
            transform: placement === 'top' ? 'translateY(-100%)' : undefined,
          }}
          role="dialog"
          aria-label={title || '确认操作'}
          onClick={(event) => event.stopPropagation()}
        >
          <div className="ui-popconfirm-title">{title || '确认操作'}</div>
          {description ? <div className="ui-popconfirm-desc">{description}</div> : null}
          <div className="ui-popconfirm-actions">
            <button
              type="button"
              className="btn btn-outline btn-sm"
              onClick={() => setOpen(false)}
              disabled={working}
            >
              {cancelText}
            </button>
            <button
              type="button"
              className={`btn btn-sm ${tone === 'danger' ? 'btn-danger' : 'btn-primary'}`}
              onClick={handleConfirm}
              disabled={working}
            >
              {working ? '处理中...' : confirmText}
            </button>
          </div>
        </div>,
        document.body,
      ) : null}
    </>
  );
}

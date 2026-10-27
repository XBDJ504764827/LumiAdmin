import { useEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';

/**
 * 轻量 Tooltip：包裹单个元素，悬停或键盘聚焦时显示说明。
 *
 * 使用 Portal + fixed 定位，避免被表格滚动容器、卡片或 Modal 的
 * overflow 裁剪。长内容请使用 Drawer / Modal。
 */
export function Tooltip({
  content,
  placement = 'top',
  children,
  className = '',
  disabled = false,
}) {
  const anchorRef = useRef(null);
  const [open, setOpen] = useState(false);
  const [position, setPosition] = useState({ top: 0, left: 0 });

  useEffect(() => {
    if (!open || disabled || !content) return undefined;

    function updatePosition() {
      const anchor = anchorRef.current;
      if (!anchor) return;
      const rect = anchor.getBoundingClientRect();
      const maxWidth = Math.min(280, window.innerWidth - 16);
      const left = Math.min(
        Math.max(maxWidth / 2 + 8, rect.left + rect.width / 2),
        window.innerWidth - maxWidth / 2 - 8,
      );
      let side = placement;
      if (side === 'top' && rect.top < 56) side = 'bottom';
      if (side === 'bottom' && window.innerHeight - rect.bottom < 56) side = 'top';
      const top = side === 'top' ? Math.max(8, rect.top - 8) : Math.min(window.innerHeight - 8, rect.bottom + 8);
      setPosition({ top, left, side, maxWidth });
    }

    updatePosition();
    window.addEventListener('resize', updatePosition);
    window.addEventListener('scroll', updatePosition, true);
    return () => {
      window.removeEventListener('resize', updatePosition);
      window.removeEventListener('scroll', updatePosition, true);
    };
  }, [open, placement, disabled, content]);

  if (!content || disabled) return children;

  return (
    <>
      <span
        ref={anchorRef}
        className={`ui-tooltip-anchor ${className}`.trim()}
        onMouseEnter={() => setOpen(true)}
        onMouseLeave={() => setOpen(false)}
        onFocus={() => setOpen(true)}
        onBlur={() => setOpen(false)}
      >
        {children}
      </span>
      {open ? createPortal(
        <div
          className={`ui-tooltip-content ui-tooltip-content-${position.side || placement}`}
          style={{
            top: position.top,
            left: position.left,
            maxWidth: position.maxWidth,
            transform: position.side === 'bottom' ? 'translateX(-50%)' : 'translate(-50%, -100%)',
          }}
          role="tooltip"
        >
          {content}
        </div>,
        document.body,
      ) : null}
    </>
  );
}

import { Tooltip } from './Tooltip.jsx';

/**
 * 字段说明图标：鼠标悬停 / 键盘聚焦时显示说明，移开后自动消失。
 * 用于替代输入框下方常驻的小字提示。
 */
export function FieldHint({ text, label = '查看字段说明', placement = 'top' }) {
  if (!text) return null;
  return (
    <Tooltip content={text} placement={placement}>
      <button type="button" className="form-help-icon" aria-label={label}>
        ?
      </button>
    </Tooltip>
  );
}

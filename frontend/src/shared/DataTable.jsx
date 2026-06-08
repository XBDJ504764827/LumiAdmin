import { useMemo } from 'react';

/**
 * 增强型 DataTable 组件
 *
 * 支持：
 * - columns 配置数组，含 key / label / render / align / width
 * - emptyText 自定义空状态文案
 * - loading / error 状态展示
 * - keyField 自定义行 key 字段名
 * - onRowClick 行点击回调
 * - compact 紧凑模式
 */
export function DataTable({
  columns = [],
  rows = [],
  loading = false,
  error = null,
  emptyText = '暂无数据',
  keyField = 'id',
  onRowClick,
  compact = false,
  className = '',
}) {
  const colSpan = columns.length || 1;

  const keyedRows = useMemo(() => rows.map((row, index) => ({
    row,
    key: row[keyField] ?? `__row_${index}`,
  })), [rows, keyField]);

  return (
    <div className={`table-wrap ${className}`.trim()}>
      <table className={`data-table ${compact ? 'data-table--compact' : ''}`.trim()} role="table">
        <thead>
          <tr role="row">
            {columns.map((col) => (
              <th
                key={col.key}
                scope="col"
                style={{
                  textAlign: col.align || 'left',
                  width: col.width || undefined,
                }}
              >
                {col.label}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {loading && (
            <tr><td colSpan={colSpan} className="table-status-cell">正在加载数据...</td></tr>
          )}
          {error && (
            <tr><td colSpan={colSpan} className="table-status-cell" style={{ color: 'var(--danger-text)' }}>{error}</td></tr>
          )}
          {!loading && keyedRows.length === 0 && (
            <tr><td colSpan={colSpan} className="table-status-cell">{emptyText}</td></tr>
          )}
          {!loading && keyedRows.map(({ row, key }) => {
            return (
              <tr
                key={key}
                role="row"
                onClick={onRowClick ? () => onRowClick(row) : undefined}
                className={onRowClick ? 'table-row-clickable' : undefined}
              >
                {columns.map((col) => (
                  <td
                    key={col.key}
                    style={{
                      textAlign: col.align || 'left',
                      width: col.width || undefined,
                    }}
                  >
                    {col.render ? col.render(row[col.key], row) : row[col.key]}
                  </td>
                ))}
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}

import { PageState } from '../../shared/PageState.jsx';

/**
 * 图表统一 Card 容器：
 * ┌─────────────────────────────────────┐
 * │ 标题                     时间范围 ▼ │
 * │ 描述                                 │
 * │        Loading / Empty / Error / 图表│
 * └─────────────────────────────────────┘
 * 所有图表共用该容器，保证视觉统一；
 * Loading / Empty / Error 三态由卡片统一处理，图表失败不影响整页。
 */

/** 时间范围分段切换控件 */
export function RangeSwitcher({ ranges, value, onChange }) {
  return (
    <div className="seg-control" role="group" aria-label="时间范围">
      {ranges.map((range) => (
        <button
          key={range.value}
          type="button"
          className={`seg-control-item${range.value === value ? ' active' : ''}`}
          onClick={() => onChange(range.value)}
        >
          {range.label}
        </button>
      ))}
    </div>
  );
}

/** 图表 Loading 骨架屏（保持容器尺寸稳定，避免页面跳动） */
export function ChartSkeleton() {
  return (
    <div className="chart-skeleton" role="status" aria-label="图表加载中">
      <div className="chart-skeleton-bar" style={{ width: '96%' }} />
      <div className="chart-skeleton-bar" style={{ width: '78%' }} />
      <div className="chart-skeleton-bar" style={{ width: '88%' }} />
      <div className="chart-skeleton-bar" style={{ width: '60%' }} />
    </div>
  );
}

export function DashboardChartCard({
  title,
  subtitle,
  ranges,
  range,
  onRangeChange,
  loading = false,
  error = false,
  errorMessage = '请稍后重试或刷新页面',
  onRetry,
  empty = false,
  emptyText = '当前时间范围内暂无统计数据',
  children,
  className = '',
}) {
  let body;
  if (loading) {
    body = <ChartSkeleton />;
  } else if (error) {
    body = (
      <PageState
        tone="danger"
        title="数据加载失败"
        message={errorMessage}
        action={onRetry}
        actionText="重试"
      />
    );
  } else if (empty) {
    body = <PageState title="暂无统计数据" message={emptyText} />;
  } else {
    body = children;
  }

  return (
    <article className={`card dash-chart-card ${className}`.trim()}>
      <div className="card-header dash-chart-header">
        <div className="dash-chart-heading">
          <div className="card-title">{title}</div>
          {subtitle ? <div className="card-sub">{subtitle}</div> : null}
        </div>
        {ranges ? (
          <RangeSwitcher ranges={ranges} value={range} onChange={onRangeChange} />
        ) : null}
      </div>
      <div className="card-body dash-chart-body">{body}</div>
    </article>
  );
}


/**
 * 页面加载骨架屏
 * 在 Suspense fallback 中使用，提供更好的加载体验
 */
export function PageSkeleton({ rows = 5 }) {
  return (
    <div className="skeleton-page">
      <div className="skeleton-line skeleton-title" />

      <div className="skeleton-card">
        {Array.from({ length: rows }, (_, i) => (
          <div key={i} className="skeleton-row">
            <div className={`skeleton-cell skeleton-cell-a skeleton-delay-${i % 5}`} />
            <div className={`skeleton-cell skeleton-cell-b skeleton-delay-${(i + 2) % 5}`} />
          </div>
        ))}
      </div>
    </div>
  );
}

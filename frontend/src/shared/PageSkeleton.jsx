import React from 'react';

/**
 * 页面加载骨架屏
 * 在 Suspense fallback 中使用，提供更好的加载体验
 */
export function PageSkeleton({ rows = 5 }) {
  return (
    <div className="skeleton-page" style={{ padding: 24 }}>
      {/* 页面标题骨架 */}
      <div className="skeleton-line skeleton-title" style={{
        width: '30%',
        height: 28,
        marginBottom: 24,
        borderRadius: 6,
        background: 'var(--skeleton-bg, #e8e8e8)',
        animation: 'skeleton-pulse 1.5s ease-in-out infinite',
      }} />

      {/* 卡片/内容骨架 */}
      <div className="skeleton-card" style={{
        background: 'var(--card-bg, #fff)',
        borderRadius: 12,
        padding: 24,
        boxShadow: 'var(--card-shadow, 0 1px 3px rgba(0,0,0,0.08))',
      }}>
        {Array.from({ length: rows }, (_, i) => (
          <div key={i} className="skeleton-row" style={{
            display: 'flex',
            gap: 16,
            marginBottom: i < rows - 1 ? 16 : 0,
          }}>
            <div className="skeleton-cell" style={{
              flex: `${30 + (i % 3) * 15}%`,
              height: 16,
              borderRadius: 4,
              background: 'var(--skeleton-bg, #e8e8e8)',
              animation: `skeleton-pulse 1.5s ease-in-out ${i * 0.1}s infinite`,
            }} />
            <div className="skeleton-cell" style={{
              flex: `${20 + (i % 2) * 10}%`,
              height: 16,
              borderRadius: 4,
              background: 'var(--skeleton-bg, #e8e8e8)',
              animation: `skeleton-pulse 1.5s ease-in-out ${i * 0.15}s infinite`,
            }} />
          </div>
        ))}
      </div>
    </div>
  );
}

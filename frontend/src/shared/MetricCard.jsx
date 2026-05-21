import React from 'react';

export function MetricCard({ label, value, badge, accent = false }) {
  return (
    <article className="metric-card">
      <div className="metric-label">
        <span>{label}</span>
        <span className={`metric-badge ${accent ? 'danger' : ''}`}>{badge}</span>
      </div>
      <div className="metric-value">{value}</div>
      <div className="sparkline" />
    </article>
  );
}

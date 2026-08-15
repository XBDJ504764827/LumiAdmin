import { useMemo, useState } from 'react';
import { keepPreviousData } from '@tanstack/react-query';
import { useApiQuery } from '../../shared/useApiQuery.js';
import { api } from '../../lib/api.js';
import { fillWhitelistTrendWindow } from '../../pages/dashboard/dashboardData.js';
import { DashboardChartCard } from './DashboardChartCard.jsx';
import { ChartCanvas, useChartThemeColors, hexToRgba } from './ChartCanvas.jsx';

const RANGES = [
  { value: 7, label: '7天' },
  { value: 30, label: '30天' },
  { value: 90, label: '90天' },
];

/** 白名单增长趋势 Line Chart：按通过时间统计每日新增白名单 */
export function WhitelistTrendChart() {
  const [days, setDays] = useState(30);
  const query = useApiQuery(
    ['dashboardAnalytics', 'whitelistTrend', days],
    (token) => api.whitelistTrend(token, days),
    { placeholderData: keepPreviousData }, // 切换时间范围时保留旧数据，避免闪烁跳动
  );

  const items = query.data?.data?.items ?? null;
  const window = useMemo(
    () => (items ? fillWhitelistTrendWindow(items, days) : null),
    [items, days],
  );

  // 图例补充统计：窗口累计 / 日均 / 单日峰值
  const summary = useMemo(() => {
    if (!window) return null;
    const total = window.counts.reduce((sum, count) => sum + count, 0);
    const peak = window.counts.length > 0 ? Math.max(...window.counts) : 0;
    const avg = window.counts.length > 0 ? total / window.counts.length : 0;
    return { total, peak, avg: Math.round(avg * 10) / 10 };
  }, [window]);

  const colors = useChartThemeColors();
  const data = useMemo(() => {
    if (!window) return null;
    return {
      labels: window.labels,
      datasets: [
        {
          label: '新增白名单',
          data: window.counts,
          borderColor: colors.accent,
          backgroundColor: hexToRgba(colors.accent, 0.1),
          fill: true,
          tension: 0.35,
          borderWidth: 2,
          pointRadius: window.labels.length > 31 ? 0 : 2.5,
          pointHoverRadius: 5,
          pointBackgroundColor: colors.surface,
          pointBorderColor: colors.accent,
        },
      ],
    };
  }, [window, colors]);

  const options = useMemo(
    () => ({
      scales: {
        x: { ticks: { maxTicksLimit: days === 90 ? 12 : 10 } },
        y: { beginAtZero: true },
      },
    }),
    [days],
  );

  return (
    <DashboardChartCard
      title="白名单增长趋势"
      subtitle="按通过时间统计每日新增白名单"
      ranges={RANGES}
      range={days}
      onRangeChange={setDays}
      loading={query.isLoading}
      error={query.isError}
      onRetry={() => query.refetch()}
      empty={!!window && window.counts.every((count) => count === 0)}
    >
      {summary ? (
        <div className="dash-chart-legend" aria-hidden="true">
          <span className="dash-chart-legend-item">
            <i className="dash-chart-dot dash-chart-dot-accent" />新增白名单
          </span>
          <span className="dash-chart-legend-meta">
            <span>累计 <b>{summary.total}</b></span>
            <span>日均 <b>{summary.avg}</b></span>
            <span>峰值 <b>{summary.peak}</b>/天</span>
          </span>
        </div>
      ) : null}
      <ChartCanvas
        type="line"
        data={data}
        options={options}
        ariaLabel="白名单增长趋势折线图"
      />
    </DashboardChartCard>
  );
}

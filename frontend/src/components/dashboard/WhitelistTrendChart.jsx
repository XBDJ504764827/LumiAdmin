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
      <ChartCanvas
        type="line"
        data={data}
        options={options}
        ariaLabel="白名单增长趋势折线图"
      />
    </DashboardChartCard>
  );
}

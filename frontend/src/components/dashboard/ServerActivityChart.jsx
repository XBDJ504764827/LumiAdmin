import { useMemo, useState } from 'react';
import { keepPreviousData } from '@tanstack/react-query';
import { useApiQuery } from '../../shared/useApiQuery.js';
import { api } from '../../lib/api.js';
import { buildServerActivityData } from '../../pages/dashboard/dashboardData.js';
import { DashboardChartCard } from './DashboardChartCard.jsx';
import { ChartCanvas, useChartThemeColors, hexToRgba } from './ChartCanvas.jsx';

const RANGES = [
  { value: 'today', label: '今日' },
  { value: '1d', label: '24小时' },
  { value: '7d', label: '7天' },
  { value: '30d', label: '30天' },
  { value: '90d', label: '90天' },
];

/**
 * 服务器活跃度趋势 Line Chart：
 * - 今日 / 24小时：按小时分桶；7/30/90 天：按天分桶
 * - 双 Series：活跃玩家（去重）+ 会话数
 */
export function ServerActivityChart() {
  const [range, setRange] = useState('today');
  const query = useApiQuery(
    ['dashboardAnalytics', 'serverActivity', range],
    (token) => api.serverActivity(token, range),
    {
      placeholderData: keepPreviousData,
      refetchInterval: 60_000, // 今日活跃度实时变化，每分钟刷新
    },
  );

  const payload = query.data?.data ?? null;
  const unit = payload?.unit ?? 'hour';
  const items = payload?.items ?? null;
  const chartData = useMemo(
    () => (items ? buildServerActivityData(items, unit) : null),
    [items, unit],
  );

  const colors = useChartThemeColors();
  const data = useMemo(() => {
    if (!chartData) return null;
    return {
      labels: chartData.labels,
      datasets: [
        {
          label: '活跃玩家',
          data: chartData.activePlayers,
          borderColor: colors.accent,
          backgroundColor: hexToRgba(colors.accent, 0.1),
          fill: true,
          tension: 0.35,
          borderWidth: 2,
          pointRadius: 0,
          pointHoverRadius: 4,
          pointBackgroundColor: colors.accent,
        },
        {
          label: '会话数',
          data: chartData.sessions,
          borderColor: colors.accent2,
          backgroundColor: 'transparent',
          borderDash: [5, 4],
          tension: 0.35,
          borderWidth: 1.5,
          pointRadius: 0,
          pointHoverRadius: 4,
          pointBackgroundColor: colors.accent2,
        },
      ],
    };
  }, [chartData, colors]);

  const options = useMemo(
    () => ({
      scales: {
        x: { ticks: { maxTicksLimit: range === 'today' || range === '1d' ? 12 : 10 } },
        y: { beginAtZero: true },
      },
    }),
    [range],
  );

  const isEmpty = !!chartData && chartData.activePlayers.every((count) => count === 0);

  return (
    <DashboardChartCard
      title="服务器活跃度"
      subtitle="活跃玩家与会话数趋势"
      ranges={RANGES}
      range={range}
      onRangeChange={setRange}
      loading={query.isLoading}
      error={query.isError}
      onRetry={() => query.refetch()}
      empty={isEmpty}
    >
      <div className="dash-chart-legend" aria-hidden="true">
        <span className="dash-chart-legend-item">
          <i className="dash-chart-dot dash-chart-dot-accent" />活跃玩家
        </span>
        <span className="dash-chart-legend-item">
          <i className="dash-chart-dot dash-chart-dot-accent2" />会话数
        </span>
      </div>
      <ChartCanvas type="line" data={data} options={options} ariaLabel="服务器活跃度趋势折线图" />
    </DashboardChartCard>
  );
}

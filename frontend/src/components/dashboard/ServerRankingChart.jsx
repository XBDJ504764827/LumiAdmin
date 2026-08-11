import { useMemo, useState } from 'react';
import { keepPreviousData } from '@tanstack/react-query';
import { useApiQuery } from '../../shared/useApiQuery.js';
import { api } from '../../lib/api.js';
import {
  buildServerRankingData,
  formatPlaytime,
} from '../../pages/dashboard/dashboardData.js';
import { DashboardChartCard } from './DashboardChartCard.jsx';
import { ChartCanvas, useChartThemeColors, hexToRgba } from './ChartCanvas.jsx';

const RANGES = [
  { value: '1d', label: '24小时' },
  { value: '7d', label: '7天' },
  { value: '30d', label: '30天' },
  { value: '90d', label: '90天' },
];

const RANKING_LIMIT = 10;

/**
 * 服务器活跃度排行 Bar Chart（横向）：
 * 按窗口内去重活跃玩家数排序，Tooltip 展示会话数与在线时长。
 * 超过 6 根柱子时使用横向布局，默认 Top 10。
 */
export function ServerRankingChart() {
  const [range, setRange] = useState('7d');
  const query = useApiQuery(
    ['dashboardAnalytics', 'serverRanking', range],
    (token) => api.serverRanking(token, range, RANKING_LIMIT),
    { placeholderData: keepPreviousData },
  );

  const items = query.data?.data ?? null;
  const ranking = useMemo(() => (items ? buildServerRankingData(items) : null), [items]);
  const colors = useChartThemeColors();

  const data = useMemo(() => {
    if (!ranking || ranking.names.length === 0) return null;
    return {
      labels: ranking.names,
      datasets: [
        {
          label: '活跃玩家',
          data: ranking.activePlayers,
          backgroundColor: hexToRgba(colors.accent, 0.78),
          hoverBackgroundColor: colors.accent,
          borderRadius: 6,
          barThickness: 22,
        },
      ],
    };
  }, [ranking, colors]);

  const options = useMemo(
    () => ({
      indexAxis: 'y',
      scales: {
        x: { beginAtZero: true },
        y: {
          ticks: {
            autoSkip: false,
            font: { size: 12 },
            callback(value) {
              const name = ranking?.names[value] ?? '';
              return name.length > 14 ? `${name.slice(0, 14)}…` : name;
            },
          },
        },
      },
      plugins: {
        tooltip: {
          callbacks: {
            label: (context) => `活跃玩家：${context.parsed.x}`,
            afterBody: (tooltipItems) => {
              const index = tooltipItems[0]?.dataIndex ?? 0;
              return [
                `会话数：${ranking?.sessions[index] ?? 0}`,
                `在线时长：${formatPlaytime(ranking?.playtimeSeconds[index])}`,
              ];
            },
          },
        },
      },
    }),
    [ranking],
  );

  return (
    <DashboardChartCard
      title="服务器活跃度排行"
      subtitle={`按活跃玩家数排序 · Top ${RANKING_LIMIT}`}
      ranges={RANGES}
      range={range}
      onRangeChange={setRange}
      loading={query.isLoading}
      error={query.isError}
      onRetry={() => query.refetch()}
      empty={!!ranking && ranking.names.length === 0}
      className="dash-chart-card--ranking"
    >
      <ChartCanvas
        type="bar"
        data={data}
        options={options}
        ariaLabel="服务器活跃度排行条形图"
      />
    </DashboardChartCard>
  );
}

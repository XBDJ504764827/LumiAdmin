import { useMemo } from 'react';
import { useApiQuery } from '../../shared/useApiQuery.js';
import { api } from '../../lib/api.js';
import { buildServerStatusData } from '../../pages/dashboard/dashboardData.js';
import { DashboardChartCard } from './DashboardChartCard.jsx';
import { ChartCanvas, useChartThemeColors } from './ChartCanvas.jsx';

/** 状态 → 主题色（与后端 status 字段一一对应） */
const STATUS_COLOR_KEY = {
  online: 'teal',
  hibernating: 'accent2',
  untested: 'text3',
  offline: 'danger',
};

/**
 * 服务器状态分布 Donut Chart：
 * 仅在状态类别确实构成整体占比时使用环形图；
 * 右侧图例展示数量与百分比，单类别时环形图仍可正常表达。
 */
export function ServerStatusChart() {
  const query = useApiQuery(
    ['dashboardAnalytics', 'serverStatus'],
    (token) => api.serverStatus(token),
    { refetchInterval: 60_000 }, // 状态实时变化，每分钟刷新（后端另有 30s 聚合缓存）
  );

  const items = query.data?.data ?? null;
  const statusData = useMemo(() => (items ? buildServerStatusData(items) : null), [items]);
  const colors = useChartThemeColors();

  const data = useMemo(() => {
    if (!statusData) return null;
    return {
      labels: statusData.items.map((item) => item.label),
      datasets: [
        {
          data: statusData.items.map((item) => item.count),
          backgroundColor: statusData.items.map(
            (item) => colors[STATUS_COLOR_KEY[item.status]] ?? colors.text3,
          ),
          borderColor: colors.surface,
          borderWidth: 3,
          borderRadius: 4,
          hoverOffset: 4,
        },
      ],
    };
  }, [statusData, colors]);

  const options = useMemo(
    () => ({
      cutout: '68%',
      plugins: {
        tooltip: {
          callbacks: {
            label: (context) => {
              const total = statusData?.total ?? 0;
              const percent = total > 0 ? Math.round((context.parsed / total) * 100) : 0;
              return ` ${context.parsed} 台（${percent}%）`;
            },
          },
        },
      },
    }),
    [statusData],
  );

  return (
    <DashboardChartCard
      title="服务器状态分布"
      subtitle="在线 / 休眠 / 离线 / 未测试"
      loading={query.isLoading}
      error={query.isError}
      onRetry={() => query.refetch()}
      empty={!!statusData && statusData.total === 0}
    >
      {statusData ? (
        <div className="dash-donut-layout">
          <div className="dash-donut-chart">
            <ChartCanvas type="doughnut" data={data} options={options} ariaLabel="服务器状态分布环形图" />
          </div>
          <ul className="dash-donut-legend" aria-label="服务器状态分布图例">
            {statusData.items.map((item) => (
              <li key={item.status} className="dash-donut-legend-item">
                <span
                  className="dash-donut-legend-dot"
                  style={{ background: colors[STATUS_COLOR_KEY[item.status]] ?? colors.text3 }}
                />
                <span className="dash-donut-legend-name">{item.label}</span>
                <span className="dash-donut-legend-count">{item.count}</span>
                <span className="dash-donut-legend-pct">
                  {statusData.total > 0 ? `${Math.round((item.count / statusData.total) * 100)}%` : '0%'}
                </span>
              </li>
            ))}
          </ul>
        </div>
      ) : null}
    </DashboardChartCard>
  );
}

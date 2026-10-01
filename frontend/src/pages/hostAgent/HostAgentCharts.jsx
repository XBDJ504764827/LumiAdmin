import { useMemo, useState } from 'react';
import { keepPreviousData } from '@tanstack/react-query';
import { useApiQuery } from '../../shared/useApiQuery.js';
import { api } from '../../lib/api.js';
import { DashboardChartCard } from '../../components/dashboard/DashboardChartCard.jsx';
import { ChartCanvas, useChartThemeColors, hexToRgba } from '../../components/dashboard/ChartCanvas.jsx';
import {
  HEARTBEAT_RANGES,
  POWER_TASK_RANGES,
  allZero,
  buildHeartbeatChartData,
  buildPowerTaskChartData,
  summarize,
} from './hostAgentTrend.js';

// Agent 心跳趋势：在线宿主机数（面积）+ 上报心跳总数（右轴虚线）。
export function HeartbeatTrendChart() {
  const [range, setRange] = useState('24h');
  const query = useApiQuery(
    ['hostAgent', 'heartbeatTrend', range],
    (token) => api.hostAgentHeartbeatTrend(token, range),
    { placeholderData: keepPreviousData },
  );
  const chartData = useMemo(
    () => buildHeartbeatChartData(query.data?.items),
    [query.data],
  );
  const colors = useChartThemeColors();

  const data = useMemo(() => ({
    labels: chartData.labels,
    datasets: [
      {
        label: '在线宿主机',
        data: chartData.online,
        borderColor: colors.accent,
        backgroundColor: hexToRgba(colors.accent, 0.12),
        fill: true,
        tension: 0.35,
        borderWidth: 2,
        pointRadius: 0,
        pointHoverRadius: 4,
        pointBackgroundColor: colors.accent,
        yAxisID: 'y',
      },
      {
        label: '心跳总数',
        data: chartData.heartbeats,
        borderColor: colors.teal,
        backgroundColor: 'transparent',
        borderDash: [5, 4],
        tension: 0.35,
        borderWidth: 1.5,
        pointRadius: 0,
        pointHoverRadius: 4,
        pointBackgroundColor: colors.teal,
        yAxisID: 'y1',
      },
    ],
  }), [chartData, colors]);

  const options = useMemo(() => ({
    scales: {
      x: { ticks: { maxTicksLimit: range === '24h' ? 12 : 10 } },
      y: { beginAtZero: true, title: { display: true, text: '台' } },
      y1: { beginAtZero: true, position: 'right', grid: { drawOnChartArea: false }, title: { display: true, text: '次' } },
    },
  }), [range]);

  const summary = useMemo(() => ({
    avgOnline: chartData.online.length ? (summarize(chartData.online).total / chartData.online.length).toFixed(1) : '0.0',
    heartbeats: summarize(chartData.heartbeats).total,
  }), [chartData]);

  return (
    <DashboardChartCard
      title="Agent 心跳"
      subtitle="在线宿主机与上报心跳总数趋势"
      ranges={HEARTBEAT_RANGES}
      range={range}
      onRangeChange={setRange}
      loading={query.isLoading}
      error={query.isError}
      onRetry={() => query.refetch()}
      empty={!query.isLoading && !query.isError && allZero(chartData.heartbeats)}
      emptyText="所选范围内暂无心跳采样"
    >
      <div className="dash-chart-legend" aria-hidden="true">
        <span className="dash-chart-legend-item">
          <i className="dash-chart-dot dash-chart-dot-accent" />在线宿主机
        </span>
        <span className="dash-chart-legend-item">
          <i className="dash-chart-dot dash-chart-dot-teal" />心跳总数
        </span>
        <span className="dash-chart-legend-meta">
          <span>平均在线 <b>{summary.avgOnline} 台</b></span>
          <span>心跳合计 <b>{summary.heartbeats}</b></span>
        </span>
      </div>
      <ChartCanvas type="line" data={data} options={options} ariaLabel="Agent 心跳趋势折线图" />
    </DashboardChartCard>
  );
}

// 电源任务趋势：重启 / 开机 / 关机次数。
export function PowerTaskTrendChart() {
  const [range, setRange] = useState('7d');
  const query = useApiQuery(
    ['hostAgent', 'powerTrend', range],
    (token) => api.hostAgentPowerTrend(token, range),
    { placeholderData: keepPreviousData },
  );
  const chartData = useMemo(
    () => buildPowerTaskChartData(query.data?.items),
    [query.data],
  );
  const colors = useChartThemeColors();

  const data = useMemo(() => ({
    labels: chartData.labels,
    datasets: [
      {
        label: '重启',
        data: chartData.restart,
        borderColor: colors.accent,
        backgroundColor: hexToRgba(colors.accent, 0.12),
        fill: true,
        tension: 0.35,
        borderWidth: 2,
        pointRadius: 0,
        pointHoverRadius: 4,
        pointBackgroundColor: colors.accent,
      },
      {
        label: '开机',
        data: chartData.start,
        borderColor: colors.teal,
        backgroundColor: 'transparent',
        tension: 0.35,
        borderWidth: 1.5,
        pointRadius: 0,
        pointHoverRadius: 4,
        pointBackgroundColor: colors.teal,
      },
      {
        label: '关机',
        data: chartData.stop,
        borderColor: colors.danger,
        backgroundColor: 'transparent',
        borderDash: [5, 4],
        tension: 0.35,
        borderWidth: 1.5,
        pointRadius: 0,
        pointHoverRadius: 4,
        pointBackgroundColor: colors.danger,
      },
    ],
  }), [chartData, colors]);

  const options = useMemo(() => ({
    scales: {
      x: { ticks: { maxTicksLimit: 10 } },
      y: { beginAtZero: true, ticks: { stepSize: 1 } },
    },
  }), []);

  const totals = useMemo(() => ({
    restart: summarize(chartData.restart).total,
    start: summarize(chartData.start).total,
    stop: summarize(chartData.stop).total,
  }), [chartData]);
  const empty = allZero(chartData.restart) && allZero(chartData.start) && allZero(chartData.stop);

  return (
    <DashboardChartCard
      title="电源任务"
      subtitle="重启 / 开机 / 关机执行次数趋势"
      ranges={POWER_TASK_RANGES}
      range={range}
      onRangeChange={setRange}
      loading={query.isLoading}
      error={query.isError}
      onRetry={() => query.refetch()}
      empty={!query.isLoading && !query.isError && empty}
      emptyText="所选范围内暂无电源任务"
    >
      <div className="dash-chart-legend" aria-hidden="true">
        <span className="dash-chart-legend-item">
          <i className="dash-chart-dot dash-chart-dot-accent" />重启
        </span>
        <span className="dash-chart-legend-item">
          <i className="dash-chart-dot dash-chart-dot-teal" />开机
        </span>
        <span className="dash-chart-legend-item">
          <i className="dash-chart-dot dash-chart-dot-danger" />关机
        </span>
        <span className="dash-chart-legend-meta">
          <span>重启 <b>{totals.restart}</b></span>
          <span>开机 <b>{totals.start}</b></span>
          <span>关机 <b>{totals.stop}</b></span>
        </span>
      </div>
      <ChartCanvas type="line" data={data} options={options} ariaLabel="电源任务趋势折线图" />
    </DashboardChartCard>
  );
}

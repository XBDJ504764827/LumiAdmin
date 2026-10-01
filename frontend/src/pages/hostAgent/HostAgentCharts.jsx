import { useMemo, useState } from 'react';
import { DashboardChartCard } from '../../components/dashboard/DashboardChartCard.jsx';
import { ChartCanvas, useChartThemeColors, hexToRgba } from '../../components/dashboard/ChartCanvas.jsx';
import {
  HOST_AGENT_RANGES,
  mockHeartbeatPoints,
  mockPowerTaskPoints,
} from './hostAgentCharts.js';

function sum(values) {
  return values.reduce((total, value) => total + value, 0);
}

function average(values) {
  return values.length === 0 ? 0 : sum(values) / values.length;
}

// Agent 心跳趋势：在线宿主机数（面积）+ 上报心跳总数（右轴虚线）。
export function HeartbeatTrendChart() {
  const [range, setRange] = useState('24h');
  const points = useMemo(() => mockHeartbeatPoints(range), [range]);
  const colors = useChartThemeColors();

  const data = useMemo(() => ({
    labels: points.labels,
    datasets: [
      {
        label: '在线宿主机',
        data: points.online,
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
        data: points.heartbeats,
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
  }), [points, colors]);

  const options = useMemo(() => ({
    scales: {
      x: { ticks: { maxTicksLimit: range === '24h' ? 12 : 10 } },
      y: { beginAtZero: true, suggestedMax: 3, title: { display: true, text: '台' } },
      y1: { beginAtZero: true, position: 'right', grid: { drawOnChartArea: false }, title: { display: true, text: '次' } },
    },
  }), [range]);

  return (
    <DashboardChartCard
      title="Agent 心跳"
      subtitle="在线宿主机与上报心跳总数趋势（演示数据）"
      ranges={HOST_AGENT_RANGES}
      range={range}
      onRangeChange={setRange}
    >
      <div className="dash-chart-legend" aria-hidden="true">
        <span className="dash-chart-legend-item">
          <i className="dash-chart-dot dash-chart-dot-accent" />在线宿主机
        </span>
        <span className="dash-chart-legend-item">
          <i className="dash-chart-dot dash-chart-dot-teal" />心跳总数
        </span>
        <span className="dash-chart-legend-meta">
          <span>平均在线 <b>{average(points.online).toFixed(1)} 台</b></span>
          <span>心跳合计 <b>{sum(points.heartbeats)}</b></span>
        </span>
      </div>
      <ChartCanvas type="line" data={data} options={options} ariaLabel="Agent 心跳趋势折线图" />
    </DashboardChartCard>
  );
}

// 电源任务趋势：重启 / 开机 / 关机次数。
export function PowerTaskTrendChart() {
  const [range, setRange] = useState('7d');
  const points = useMemo(() => mockPowerTaskPoints(range), [range]);
  const colors = useChartThemeColors();

  const data = useMemo(() => ({
    labels: points.labels,
    datasets: [
      {
        label: '重启',
        data: points.restart,
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
        data: points.start,
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
        data: points.stop,
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
  }), [points, colors]);

  const options = useMemo(() => ({
    scales: {
      x: { ticks: { maxTicksLimit: range === '24h' ? 12 : 10 } },
      y: { beginAtZero: true, ticks: { stepSize: 1 } },
    },
  }), [range]);

  return (
    <DashboardChartCard
      title="电源任务"
      subtitle="重启 / 开机 / 关机执行次数趋势（演示数据）"
      ranges={HOST_AGENT_RANGES}
      range={range}
      onRangeChange={setRange}
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
          <span>重启 <b>{sum(points.restart)}</b></span>
          <span>开机 <b>{sum(points.start)}</b></span>
          <span>关机 <b>{sum(points.stop)}</b></span>
        </span>
      </div>
      <ChartCanvas type="line" data={data} options={options} ariaLabel="电源任务趋势折线图" />
    </DashboardChartCard>
  );
}

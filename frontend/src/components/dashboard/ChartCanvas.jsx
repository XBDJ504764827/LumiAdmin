import { useEffect, useMemo, useRef } from 'react';
import Chart from 'chart.js/auto';
import { useTheme } from '../../state/store.js';

/**
 * Chart.js 生命周期封装：
 * - 组件卸载时销毁 Chart 实例
 * - 主题切换（light/dark）时读取最新 Design Token 并重建图表
 * - 图表颜色一律来自 CSS 变量，不在组件中硬编码
 *
 * `data` / `options` 需由父组件 useMemo 缓存，避免每次 Render 重建实例。
 */

function cssVar(name, fallback) {
  if (typeof window === 'undefined' || typeof getComputedStyle !== 'function') return fallback;
  const value = getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  return value || fallback;
}

/** 从当前主题解析 Design Token（图表绘制需要具体颜色值） */
export function chartThemeColors() {
  return {
    accent: cssVar('--accent', '#e8735a'),
    accent2: cssVar('--accent2', '#5b8dd9'),
    teal: cssVar('--teal', '#4ab5a0'),
    pink: cssVar('--pink', '#e87b9e'),
    text: cssVar('--text', '#1a1917'),
    text2: cssVar('--text2', '#6b6860'),
    text3: cssVar('--text3', '#9e9b93'),
    border: cssVar('--border', '#e8e5df'),
    surface: cssVar('--surface', '#ffffff'),
    danger: cssVar('--danger-text', '#c62828'),
  };
}

/**
 * 主题感知的颜色 Hook：主题切换时重新解析 Design Token。
 * 返回稳定引用，供 useMemo 依赖使用，避免图表数据每次 Render 重建。
 */
export function useChartThemeColors() {
  const { theme } = useTheme();
  return useMemo(() => {
    void theme; // theme 作为依赖信号：切换主题时重新解析 Design Token
    return chartThemeColors();
  }, [theme]);
}

/** hex 颜色转 rgba，用于图表半透明填充 */
export function hexToRgba(hex, alpha) {
  const value = String(hex ?? '').replace('#', '');
  if (value.length !== 6) return hex;
  const r = parseInt(value.slice(0, 2), 16);
  const g = parseInt(value.slice(2, 4), 16);
  const b = parseInt(value.slice(4, 6), 16);
  return `rgba(${r}, ${g}, ${b}, ${alpha})`;
}

/** 简单的深合并（chart.js options 默认值 + 调用方覆盖） */
function mergeDeep(base, extra) {
  if (!extra) return base;
  const result = { ...base };
  for (const [key, value] of Object.entries(extra)) {
    if (
      value &&
      typeof value === 'object' &&
      !Array.isArray(value) &&
      base[key] &&
      typeof base[key] === 'object'
    ) {
      result[key] = mergeDeep(base[key], value);
    } else {
      result[key] = value;
    }
  }
  return result;
}

/** 默认 options：网格 / 刻度 / Tooltip 颜色全部取自当前主题 */
function baseOptions() {
  const colors = chartThemeColors();
  return {
    responsive: true,
    maintainAspectRatio: false,
    interaction: { mode: 'index', intersect: false },
    animation: { duration: 400 },
    plugins: {
      legend: { display: false },
      tooltip: {
        backgroundColor: colors.surface,
        borderColor: colors.border,
        borderWidth: 1,
        titleColor: colors.text,
        bodyColor: colors.text2,
        titleFont: { size: 12, weight: '600' },
        bodyFont: { size: 12 },
        padding: 10,
        cornerRadius: 8,
        boxPadding: 4,
        displayColors: true,
      },
    },
    scales: {
      x: {
        grid: { color: hexToRgba(colors.border, 0.55), drawTicks: false },
        border: { display: false },
        ticks: { color: colors.text3, font: { size: 11 }, maxRotation: 0, autoSkip: true },
      },
      y: {
        grid: { color: hexToRgba(colors.border, 0.55) },
        border: { display: false },
        ticks: { color: colors.text3, font: { size: 11 }, precision: 0 },
      },
    },
  };
}

export function ChartCanvas({ type, data, options = {}, height, ariaLabel }) {
  const canvasRef = useRef(null);
  const { theme } = useTheme(); // 主题切换时重建图表

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || !data) return undefined;

    const chart = new Chart(canvas.getContext('2d'), {
      type,
      data,
      options: mergeDeep(baseOptions(), options),
    });
    return () => {
      chart.destroy();
    };
  }, [type, data, options, theme]);

  return (
    <div className="dash-chart-fill" style={height ? { height } : undefined}>
      <canvas ref={canvasRef} role="img" aria-label={ariaLabel ?? ''} />
    </div>
  );
}

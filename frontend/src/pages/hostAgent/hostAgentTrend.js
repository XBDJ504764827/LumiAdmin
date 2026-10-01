// Agent 趋势图数据构造器：把后端聚合接口的 items 转成图表数据集。
// 后端按桶返回 { bucket, online, heartbeats } / { bucket, restart, start, stop }。

export const HEARTBEAT_RANGES = [
  { value: '24h', label: '24小时' },
  { value: '7d', label: '7天' },
  { value: '30d', label: '30天' },
];

export const POWER_TASK_RANGES = [
  { value: '7d', label: '7天' },
  { value: '30d', label: '30天' },
];

function numbers(items, key) {
  return (Array.isArray(items) ? items : []).map((item) => Number(item?.[key]) || 0);
}

export function buildHeartbeatChartData(items) {
  const list = Array.isArray(items) ? items : [];
  return {
    labels: list.map((item) => item?.bucket ?? ''),
    online: numbers(list, 'online'),
    heartbeats: numbers(list, 'heartbeats'),
  };
}

export function buildPowerTaskChartData(items) {
  const list = Array.isArray(items) ? items : [];
  return {
    labels: list.map((item) => item?.bucket ?? ''),
    restart: numbers(list, 'restart'),
    start: numbers(list, 'start'),
    stop: numbers(list, 'stop'),
  };
}

export function allZero(values) {
  return values.length === 0 || values.every((value) => value === 0);
}

export function summarize(values) {
  const total = values.reduce((sum, value) => sum + value, 0);
  const peak = values.reduce((max, value) => Math.max(max, value), 0);
  return { total, peak };
}

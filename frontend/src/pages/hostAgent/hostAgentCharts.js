// Agent 监控折线图演示数据：监控接口未上线前用于预览设计，接入真实上报后删除。
// 生成器按 range 做确定性伪随机，同 range 多次调用结果一致。

export const HOST_AGENT_RANGES = [
  { value: '24h', label: '24小时' },
  { value: '7d', label: '7天' },
  { value: '30d', label: '30天' },
];

function mulberry32(seed) {
  let state = seed >>> 0;
  return () => {
    state |= 0;
    state = (state + 0x6d2b79f5) | 0;
    let mixed = Math.imul(state ^ (state >>> 15), 1 | state);
    mixed = (mixed + Math.imul(mixed ^ (mixed >>> 7), 61 | mixed)) ^ mixed;
    return ((mixed ^ (mixed >>> 14)) >>> 0) / 4294967296;
  };
}

function rangeSeed(range) {
  return { '24h': 101, '7d': 707, '30d': 3003 }[range] ?? 1;
}

function pointCount(range) {
  return range === '24h' ? 24 : range === '7d' ? 7 : 30;
}

function buildLabels(range, now = new Date()) {
  const labels = [];
  const count = pointCount(range);
  for (let i = count - 1; i >= 0; i -= 1) {
    const point = new Date(now.getTime());
    if (range === '24h') {
      point.setHours(point.getHours() - i, 0, 0, 0);
      labels.push(`${String(point.getHours()).padStart(2, '0')}:00`);
    } else {
      point.setDate(point.getDate() - i);
      labels.push(`${point.getMonth() + 1}-${point.getDate()}`);
    }
  }
  return labels;
}

// 心跳趋势：在线宿主机数（面积）+ 上报心跳总数（虚线）。
export function mockHeartbeatPoints(range) {
  const random = mulberry32(rangeSeed(range));
  const labels = buildLabels(range);
  const online = [];
  const heartbeats = [];
  for (let i = 0; i < labels.length; i += 1) {
    // game-02 偶发离线：演示在线率波动
    const secondOnline = random() > 0.18;
    online.push(secondOnline ? 2 : 1);
    const perHost = range === '24h' ? 350 + Math.floor(random() * 30) : 8300 + Math.floor(random() * 500);
    heartbeats.push(perHost * (secondOnline ? 2 : 1));
  }
  return { labels, online, heartbeats };
}

// 电源任务趋势：重启 / 开机 / 关机次数（稀疏事件）。
export function mockPowerTaskPoints(range) {
  const random = mulberry32(rangeSeed(range) + 7);
  const labels = buildLabels(range);
  const restart = [];
  const start = [];
  const stop = [];
  const density = range === '24h' ? 0.12 : 0.3;
  for (let i = 0; i < labels.length; i += 1) {
    restart.push(random() < density ? 1 + Math.floor(random() * 2) : 0);
    start.push(random() < density / 3 ? 1 : 0);
    stop.push(random() < density / 3 ? 1 : 0);
  }
  return { labels, restart, start, stop };
}

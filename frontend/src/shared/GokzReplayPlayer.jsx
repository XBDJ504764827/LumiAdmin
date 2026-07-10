import { useEffect, useMemo, useRef, useState } from 'react';
import { parseGokzReplay } from './gokzReplay.js';

function formatSeconds(value) {
  if (!Number.isFinite(value)) return '00:00.00';
  const safe = Math.max(0, value);
  const minutes = Math.floor(safe / 60);
  const seconds = safe - minutes * 60;
  return `${String(minutes).padStart(2, '0')}:${seconds.toFixed(2).padStart(5, '0')}`;
}

function drawReplay(canvas, replay, tickIndex) {
  const context = canvas.getContext('2d');
  const rect = canvas.getBoundingClientRect();
  const ratio = window.devicePixelRatio || 1;
  const width = Math.max(320, rect.width);
  const height = Math.max(260, rect.height);
  if (canvas.width !== Math.round(width * ratio) || canvas.height !== Math.round(height * ratio)) {
    canvas.width = Math.round(width * ratio);
    canvas.height = Math.round(height * ratio);
  }
  context.setTransform(ratio, 0, 0, ratio, 0, 0);
  context.clearRect(0, 0, width, height);
  context.fillStyle = '#08101d';
  context.fillRect(0, 0, width, height);

  const { ticks } = replay;
  let minX = Infinity; let maxX = -Infinity; let minY = Infinity; let maxY = -Infinity;
  for (const tick of ticks) {
    minX = Math.min(minX, tick.origin[0]); maxX = Math.max(maxX, tick.origin[0]);
    minY = Math.min(minY, tick.origin[1]); maxY = Math.max(maxY, tick.origin[1]);
  }
  const padding = 28;
  const rangeX = Math.max(1, maxX - minX);
  const rangeY = Math.max(1, maxY - minY);
  const scale = Math.min((width - padding * 2) / rangeX, (height - padding * 2) / rangeY);
  const offsetX = (width - rangeX * scale) / 2;
  const offsetY = (height - rangeY * scale) / 2;
  const point = (tick) => [
    offsetX + (tick.origin[0] - minX) * scale,
    height - offsetY - (tick.origin[1] - minY) * scale,
  ];

  context.strokeStyle = 'rgba(148, 163, 184, .13)';
  context.lineWidth = 1;
  for (let i = 1; i < 5; i += 1) {
    const x = (width / 5) * i;
    const y = (height / 5) * i;
    context.beginPath(); context.moveTo(x, 0); context.lineTo(x, height); context.stroke();
    context.beginPath(); context.moveTo(0, y); context.lineTo(width, y); context.stroke();
  }

  context.strokeStyle = 'rgba(56, 189, 248, .38)';
  context.lineWidth = 2;
  context.beginPath();
  const step = Math.max(1, Math.floor(ticks.length / 6000));
  for (let i = 0; i < ticks.length; i += step) {
    const [x, y] = point(ticks[i]);
    if (i === 0) context.moveTo(x, y); else context.lineTo(x, y);
  }
  context.stroke();

  context.strokeStyle = '#38bdf8';
  context.lineWidth = 3;
  context.beginPath();
  for (let i = 0; i <= tickIndex; i += step) {
    const [x, y] = point(ticks[i]);
    if (i === 0) context.moveTo(x, y); else context.lineTo(x, y);
  }
  context.stroke();

  const current = ticks[Math.min(tickIndex, ticks.length - 1)];
  const [x, y] = point(current);
  context.fillStyle = '#f8fafc';
  context.beginPath(); context.arc(x, y, 6, 0, Math.PI * 2); context.fill();
  const yaw = (current.angles[1] * Math.PI) / 180;
  context.strokeStyle = '#facc15'; context.lineWidth = 2;
  context.beginPath(); context.moveTo(x, y); context.lineTo(x + Math.cos(yaw) * 22, y - Math.sin(yaw) * 22); context.stroke();

  context.fillStyle = 'rgba(226, 232, 240, .75)';
  context.font = '12px system-ui, sans-serif';
  context.fillText('俯视轨迹（不含地图模型）', 12, 20);
}

export function GokzReplayPlayer({ file }) {
  const canvasRef = useRef(null);
  const frameRef = useRef(null);
  const startedAtRef = useRef(0);
  const startTickRef = useRef(0);
  const currentTickRef = useRef(0);
  const [replay, setReplay] = useState(null);
  const [error, setError] = useState('');
  const [loading, setLoading] = useState(true);
  const [playing, setPlaying] = useState(false);
  const [tick, setTick] = useState(0);
  const [speed, setSpeed] = useState(1);

  useEffect(() => {
    const controller = new AbortController();
    fetch(file.url, { signal: controller.signal })
      .then((response) => {
        if (!response.ok) throw new Error(`录像读取失败（HTTP ${response.status}）`);
        return response.arrayBuffer();
      })
      .then((buffer) => setReplay(parseGokzReplay(buffer)))
      .catch((reason) => {
        if (reason.name !== 'AbortError') setError(reason.message || '录像解析失败');
      })
      .finally(() => setLoading(false));
    return () => controller.abort();
  }, [file.url]);

  useEffect(() => {
    if (!replay || !canvasRef.current) return undefined;
    drawReplay(canvasRef.current, replay, tick);
    const onResize = () => drawReplay(canvasRef.current, replay, tick);
    window.addEventListener('resize', onResize);
    return () => window.removeEventListener('resize', onResize);
  }, [replay, tick]);

  useEffect(() => {
    if (!playing || !replay) return undefined;
    startedAtRef.current = performance.now();
    startTickRef.current = currentTickRef.current;
    const update = (now) => {
      const elapsed = ((now - startedAtRef.current) / 1000) * replay.header.tickrate * speed;
      const next = Math.min(replay.ticks.length - 1, Math.floor(startTickRef.current + elapsed));
      currentTickRef.current = next;
      setTick(next);
      if (next >= replay.ticks.length - 1) setPlaying(false);
      else frameRef.current = requestAnimationFrame(update);
    };
    frameRef.current = requestAnimationFrame(update);
    return () => cancelAnimationFrame(frameRef.current);
  }, [playing, replay, speed]);

  const current = replay?.ticks[tick];
  const speedUnits = useMemo(() => {
    if (!current) return 0;
    return Math.hypot(current.velocity[0], current.velocity[1]);
  }, [current]);

  if (loading) return <div className="replay-player-state">正在解析 Replay 轨迹…</div>;
  if (error) return <div className="replay-player-state replay-player-error">{error}，仍可下载原文件在游戏服务器中播放。</div>;
  if (!replay) return null;

  const currentSeconds = tick / replay.header.tickrate;
  const totalSeconds = (replay.ticks.length - 1) / replay.header.tickrate;
  return (
    <div className="gokz-replay-player">
      <div className="gokz-replay-heading">
        <div><strong>网页轨迹播放器</strong><span>{replay.header.mapName} · {replay.header.playerAlias}</span></div>
        <div>{replay.header.tickrate.toFixed(0)} tick · TP {replay.header.teleports}</div>
      </div>
      <canvas ref={canvasRef} className="gokz-replay-canvas" />
      <input
        className="gokz-replay-range"
        type="range"
        min="0"
        max={replay.ticks.length - 1}
        value={tick}
        onChange={(event) => {
          const next = Number(event.target.value);
          currentTickRef.current = next;
          setPlaying(false);
          setTick(next);
        }}
        aria-label="Replay 播放进度"
      />
      <div className="gokz-replay-controls">
        <button type="button" className="action-btn action-btn-accent" onClick={() => {
          if (tick >= replay.ticks.length - 1) {
            currentTickRef.current = 0;
            setTick(0);
          }
          setPlaying((value) => !value);
        }}>{playing ? '暂停' : '播放'}</button>
        <span>{formatSeconds(currentSeconds)} / {formatSeconds(totalSeconds)}</span>
        <span>速度 {speedUnits.toFixed(0)} u/s</span>
        <span>高度 {current?.origin[2].toFixed(0) ?? '-'} u</span>
        <select value={speed} onChange={(event) => setSpeed(Number(event.target.value))} aria-label="播放倍速">
          <option value={0.5}>0.5×</option><option value={1}>1×</option><option value={2}>2×</option><option value={4}>4×</option>
        </select>
      </div>
      <div className="gokz-replay-note">Replay 仅保存位置、视角和按键轨迹，不包含游戏画面、地图模型或声音。</div>
    </div>
  );
}

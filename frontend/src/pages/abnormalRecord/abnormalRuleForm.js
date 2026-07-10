export const ALL_MAPS_RULE = '*';

export function createEmptyRuleForm(scope = 'single_map') {
  return {
    scope,
    map_name: '',
    course: 0,
    mode: '',
    time_type: '',
    threshold_minutes: 0,
    threshold_seconds: 5,
    enabled: true,
    note: '',
  };
}

export function splitThreshold(totalSeconds) {
  const value = Number(totalSeconds);
  if (!Number.isFinite(value) || value < 0) return { minutes: 0, seconds: 0 };
  const centiseconds = Math.round(value * 100);
  const minutes = Math.floor(centiseconds / 6000);
  const seconds = (centiseconds - minutes * 6000) / 100;
  return { minutes, seconds };
}

export function ruleToForm(rule) {
  if (!rule) return createEmptyRuleForm();
  const threshold = splitThreshold(rule.threshold_seconds);
  return {
    scope: rule.map_name === ALL_MAPS_RULE ? 'all_maps' : 'single_map',
    map_name: rule.map_name === ALL_MAPS_RULE ? '' : (rule.map_name || ''),
    course: rule.course ?? 0,
    mode: rule.mode || '',
    time_type: rule.time_type || '',
    threshold_minutes: threshold.minutes,
    threshold_seconds: threshold.seconds,
    enabled: Boolean(rule.enabled),
    note: rule.note || '',
  };
}

export function buildRulePayload(form) {
  const mapName = form.scope === 'all_maps' ? ALL_MAPS_RULE : String(form.map_name ?? '').trim();
  if (!mapName) return { error: '请输入地图名称。', payload: null };

  const minutes = Number(form.threshold_minutes);
  const seconds = Number(form.threshold_seconds);
  if (!Number.isInteger(minutes) || minutes < 0) {
    return { error: '阈值分钟必须是大于或等于 0 的整数。', payload: null };
  }
  if (!Number.isFinite(seconds) || seconds < 0 || seconds >= 60) {
    return { error: '阈值秒数必须在 0（含）到 60（不含）之间。', payload: null };
  }

  const thresholdSeconds = minutes * 60 + seconds;
  if (thresholdSeconds <= 0) return { error: '异常阈值必须大于 0 秒。', payload: null };
  if (thresholdSeconds > 86_400) return { error: '异常阈值不能超过 24 小时。', payload: null };

  return {
    error: '',
    payload: {
      map_name: mapName,
      course: Math.max(0, Number(form.course) || 0),
      mode: form.mode || null,
      time_type: form.time_type || null,
      threshold_seconds: thresholdSeconds,
      enabled: Boolean(form.enabled),
      note: String(form.note ?? '').trim() || null,
    },
  };
}

export function ruleMapLabel(mapName) {
  return mapName === ALL_MAPS_RULE ? '全部地图（默认）' : mapName;
}

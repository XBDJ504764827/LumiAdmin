export const CHINA_TIME_ZONE = 'Asia/Shanghai';

function dateParts(value, options) {
  const date = value instanceof Date ? value : new Date(value);
  if (Number.isNaN(date.getTime())) return null;

  const parts = new Intl.DateTimeFormat('zh-CN', {
    timeZone: CHINA_TIME_ZONE,
    hour12: false,
    hourCycle: 'h23',
    ...options,
  }).formatToParts(date);

  return Object.fromEntries(parts.map((part) => [part.type, part.value]));
}

export function formatChinaDateTime(value, { seconds = true } = {}) {
  if (!value) return '-';
  const parts = dateParts(value, {
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
    ...(seconds ? { second: '2-digit' } : {}),
  });
  if (!parts) return value;

  const time = seconds
    ? `${parts.hour}:${parts.minute}:${parts.second}`
    : `${parts.hour}:${parts.minute}`;
  return `${parts.year}-${parts.month}-${parts.day} ${time}`;
}

export function formatChinaDate(value) {
  if (!value) return '-';
  const parts = dateParts(value, {
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
  });
  if (!parts) return value;
  return `${parts.year}-${parts.month}-${parts.day}`;
}

export function formatChinaMonthDayTime(value) {
  if (!value) return '-';
  const parts = dateParts(value, {
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
  });
  if (!parts) return value;
  return `${parts.month}-${parts.day} ${parts.hour}:${parts.minute}`;
}

export function formatChinaToday(value = new Date()) {
  const parts = dateParts(value, {
    year: 'numeric',
    month: 'numeric',
    day: 'numeric',
  });
  if (!parts) return '';
  return `${parts.year}年${parts.month}月${parts.day}日`;
}

export function getChinaHour(value = new Date()) {
  const parts = dateParts(value, { hour: '2-digit' });
  if (!parts) return new Date(value).getHours();
  return Number(parts.hour);
}

export const BAN_TYPE_OPTIONS = [
  { value: 'steam', label: 'Steam 账号封禁' },
  { value: 'ip', label: 'IP 封禁' },
];

export const BAN_DURATION_OPTIONS = [
  { value: 0, label: '永久封禁' },
  { value: 30, label: '30 分钟' },
  { value: 60, label: '1 小时' },
  { value: 360, label: '6 小时' },
  { value: 720, label: '12 小时' },
  { value: 1440, label: '1 天' },
  { value: 4320, label: '3 天' },
  { value: 10080, label: '1 周' },
  { value: 43200, label: '1 个月' },
  { value: -1, label: '自定义到期时间' },
];

export const emptyBanForm = {
  player: '',
  steam_id: '',
  ban_type: '',
  ip_address: '',
  reason: '',
  duration_minutes: 0,
  expires_at: null,
};

function optionalText(value) {
  const trimmed = value.trim();
  return trimmed ? trimmed : null;
}

export function validateBanForm(form) {
  if (!form.steam_id.trim()) {
    return '请输入 SteamID64。';
  }

  if (!form.ban_type) {
    return '请选择封禁属性。';
  }

  if (!BAN_TYPE_OPTIONS.some((option) => option.value === form.ban_type)) {
    return '封禁属性无效。';
  }

  if (form.duration_minutes === -1 && !form.expires_at) {
    return '请选择封禁到期时间。';
  }

  if (form.duration_minutes === -1 && form.expires_at && new Date(form.expires_at) <= new Date()) {
    return '封禁到期时间必须晚于当前时间。';
  }

  if (!form.reason.trim()) {
    return '请输入封禁理由。';
  }

  return '';
}

/**
 * 根据表单数据计算提交给后端的 duration_minutes 和 expires_at。
 * - 预设时长：duration_minutes > 0，后端自行计算 expires_at
 * - 永久封禁：duration_minutes = 0
 * - 自定义到期时间：duration_minutes = -1，前端传 expires_at ISO 字符串，后端使用该值
 */
export function buildCreateBanPayload(form) {
  const payload = {
    player: optionalText(form.player),
    steam_id: form.steam_id.trim(),
    ban_type: form.ban_type,
    ip_address: optionalText(form.ip_address),
    reason: form.reason.trim(),
  };

  if (form.duration_minutes === -1) {
    payload.duration_minutes = 0;
    payload.expires_at = form.expires_at;
  } else {
    payload.duration_minutes = form.duration_minutes;
  }

  return payload;
}

export function banRecordAction(record) {
  return record.status === 'active' ? 'unban' : 'reban';
}

export function buildBanFormFromRecord(record) {
  let duration_minutes = record.duration_minutes ?? 0;
  let expires_at = null;

  // 如果记录有时长但不在预设选项中，标记为自定义
  if (duration_minutes > 0 && !BAN_DURATION_OPTIONS.some((opt) => opt.value === duration_minutes)) {
    duration_minutes = -1;
    expires_at = record.expires_at ?? null;
  }

  return {
    player: record.player ?? '',
    steam_id: record.steam_id ?? '',
    ban_type: record.ban_type ?? '',
    ip_address: record.ip_address ?? '',
    reason: record.reason ?? '',
    duration_minutes,
    expires_at,
  };
}

export function banModalTitle(mode) {
  if (mode === 'edit') return '编辑封禁记录';
  if (mode === 'reban') return '重新封禁玩家';
  return '手动添加封禁';
}

export function banModalSubmitText(mode, submitting) {
  if (submitting) return '提交中...';
  if (mode === 'edit') return '保存修改';
  if (mode === 'reban') return '确认重新封禁';
  return '确认封禁';
}

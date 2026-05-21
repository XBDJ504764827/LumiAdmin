export const BAN_TYPE_OPTIONS = [
  { value: 'steam', label: 'Steam 账号封禁' },
  { value: 'ip', label: 'IP 封禁' },
];

export const emptyBanForm = {
  player: '',
  steam_id: '',
  ban_type: '',
  ip_address: '',
  reason: '',
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

  if (!form.reason.trim()) {
    return '请输入封禁理由。';
  }

  return '';
}

export function buildCreateBanPayload(form) {
  return {
    player: optionalText(form.player),
    steam_id: form.steam_id.trim(),
    ban_type: form.ban_type,
    ip_address: optionalText(form.ip_address),
    reason: form.reason.trim(),
  };
}

export function banRecordAction(record) {
  return record.status === 'active' ? 'unban' : 'reban';
}

export function buildBanFormFromRecord(record) {
  return {
    player: record.player ?? '',
    steam_id: record.steam_id ?? '',
    ban_type: record.ban_type ?? '',
    ip_address: record.ip_address ?? '',
    reason: record.reason ?? '',
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

export function validateCreateUserForm(form) {
  if (!form.username?.trim()) return '请输入用户名。';
  if (!form.password?.trim()) return '请输入密码。';
  return '';
}

export function buildCreateUserPayload(form) {
  return {
    username: form.username.trim(),
    password: form.password.trim(),
    role: form.role,
    steam_id: form.steam_id?.trim() ? form.steam_id.trim() : null,
    remark: form.remark?.trim() ? form.remark.trim() : null,
  };
}

export function buildUpdateUserPayload(form, includeRole) {
  return {
    username: form.username.trim(),
    role: includeRole ? form.role : undefined,
    steam_id: form.steam_id?.trim() ? form.steam_id.trim() : null,
    remark: form.remark?.trim() ? form.remark.trim() : null,
  };
}

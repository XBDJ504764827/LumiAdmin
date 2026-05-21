export const emptyAccessConfig = {
  access_restriction_enabled: false,
  min_rating: '0',
  min_steam_level: '0',
  whitelist_mode_enabled: false,
  use_custom_access: false,
};

export const emptyCommunityAccessConfig = {
  whitelist_mode_enabled: false,
  min_rating: '0',
  min_steam_level: '0',
};

export function validateAccessConfig(form) {
  const minRating = Number(form.min_rating);
  const minSteamLevel = Number(form.min_steam_level);

  if (!Number.isInteger(minRating) || minRating < 0) return '最低进入 rating 不能为负数。';
  if (!Number.isInteger(minSteamLevel) || minSteamLevel < 0) return '最低 Steam 等级不能为负数。';
  return '';
}

export function validateCommunityAccessConfig(form) {
  const minRating = Number(form.min_rating);
  const minSteamLevel = Number(form.min_steam_level);

  if (!Number.isInteger(minRating) || minRating < 0) return '社区最低进入 rating 不能为负数。';
  if (!Number.isInteger(minSteamLevel) || minSteamLevel < 0) return '社区最低 Steam 等级不能为负数。';
  return '';
}

export function buildServerPayloadWithAccess(form) {
  const validationError = validateAccessConfig(form);
  if (validationError) throw new Error(validationError);

  const maxPlayers = Number(form.max_players);
  if (!Number.isInteger(maxPlayers) || maxPlayers < 0) throw new Error('最大玩家数不能为负数。');

  return {
    name: form.name.trim(),
    ip: form.ip.trim(),
    port: Number(form.port),
    rcon_password: form.rcon_password.trim(),
    report_token: form.report_token.trim() || null,
    note: form.note.trim() || null,
    access_restriction_enabled: Boolean(form.access_restriction_enabled),
    min_rating: Number(form.min_rating),
    min_steam_level: Number(form.min_steam_level),
    whitelist_mode_enabled: Boolean(form.whitelist_mode_enabled),
    max_players: maxPlayers,
    use_custom_access: Boolean(form.use_custom_access),
  };
}

export function buildCommunityAccessPayload(form) {
  const validationError = validateCommunityAccessConfig(form);
  if (validationError) throw new Error(validationError);

  return {
    whitelist_mode_enabled: Boolean(form.whitelist_mode_enabled),
    min_rating: Number(form.min_rating),
    min_steam_level: Number(form.min_steam_level),
  };
}

export function fillAccessConfigFromServer(server) {
  return {
    access_restriction_enabled: Boolean(server.access_restriction_enabled),
    min_rating: String(server.min_rating ?? 0),
    min_steam_level: String(server.min_steam_level ?? 0),
    whitelist_mode_enabled: Boolean(server.whitelist_mode_enabled),
    use_custom_access: Boolean(server.use_custom_access),
  };
}

export function fillCommunityAccessConfig(group) {
  return {
    whitelist_mode_enabled: Boolean(group.whitelist_mode_enabled),
    min_rating: String(group.min_rating ?? 0),
    min_steam_level: String(group.min_steam_level ?? 0),
  };
}

export function buildAccessSummary(server, group) {
  const custom = Boolean(server.use_custom_access);
  const hasRestriction = custom ? Boolean(server.access_restriction_enabled) : (group?.min_rating > 0 || group?.min_steam_level > 0);
  const hasWhitelist = custom ? Boolean(server.whitelist_mode_enabled) : Boolean(group?.whitelist_mode_enabled);
  const minRating = custom ? (server.min_rating ?? 0) : (group?.min_rating ?? 0);
  const minSteamLevel = custom ? (server.min_steam_level ?? 0) : (group?.min_steam_level ?? 0);
  const source = custom ? '' : '（社区）';

  if (hasWhitelist && hasRestriction) {
    return `满足限制即可进；不满足需通过白名单（rating ≥ ${minRating}，Steam 等级 ≥ ${minSteamLevel}${source}）`;
  }
  if (hasWhitelist) return '白名单模式';
  if (hasRestriction) return `限制：rating ≥ ${minRating}，Steam 等级 ≥ ${minSteamLevel}${source}`;
  return '无限制';
}

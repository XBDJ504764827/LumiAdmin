export function onlinePlayerKey(player) {
  return `${player.steam_id64}-${player.ip}`;
}

export function onlinePlayerFields(player) {
  return [
    { label: 'SteamID64', value: player.steam_id64 },
    { label: 'IP 地址', value: player.ip },
    { label: '延迟', value: `${player.ping}ms` },
    { label: '服务器端口', value: player.server_port },
  ];
}

/** SteamID64 → SteamID3 ([U:1:ACCOUNTID])，不依赖 STEAM_0/STEAM_1 universe */
export function steamId64ToSteam3(steamId64) {
  const accountId = BigInt(steamId64) - 76561197960265728n;
  return `[U:1:${accountId}]`;
}

export function buildKickCommand(steamId64, reason = '') {
  const steam3 = steamId64ToSteam3(steamId64);
  const escapedReason = reason ? ` "${reason}"` : '';
  return `sm_kick "${steam3}"${escapedReason}`;
}

export function buildBanCommand(steamId64, duration, reason) {
  return `sm_ban "${steamId64}" ${duration} "${reason}"`;
}

export function formatBanDuration(durationMinutes) {
  if (durationMinutes === 0) return '永久';
  if (durationMinutes >= 1440) {
    const days = Math.floor(durationMinutes / 1440);
    return `${days} 天`;
  }
  if (durationMinutes >= 60) {
    const hours = Math.floor(durationMinutes / 60);
    return `${hours} 小时`;
  }
  return `${durationMinutes} 分钟`;
}

export const BAN_DURATION_OPTIONS = [
  { value: 0, label: '永久' },
  { value: 30, label: '30 分钟' },
  { value: 60, label: '1 小时' },
  { value: 1440, label: '1 天' },
  { value: 10080, label: '1 周' },
];

export const BAN_REASON_OPTIONS = [
  '作弊',
  '恶意行为',
  '辱骂玩家',
  '干扰游戏',
];

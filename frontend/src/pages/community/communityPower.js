// 服务器电源操作（界面预览阶段）：变体判定、可用性、文案。
// 下发执行尚未接入，确认后仅关闭弹窗；接入时替换 CommunityPage 中的占位 toast。

export const POWER_COUNTDOWN_SECS = 5;

export const POWER_ACTION = Object.freeze({
  restart: 'restart',
  start: 'start',
  stop: 'stop',
});

export const RESTART_HAS_PLAYERS_TEXT = '当前服务器内存在玩家无法进行重启服务器操作。';
export const RESTART_EMPTY_TEXT =
  '当前指令将重启服务器，可能因为服务器延迟问题没有正确的显示出服务器内是否存在玩家，请您确认服务器内没有玩家后再继续下一步。';

export function serverPlayerCount(server) {
  return server?.online_player_count ?? server?.players?.length ?? 0;
}

// 开机状态：进程在跑（在线或空服休眠）；其余视为关机/未启动。
export function isPoweredOn(server) {
  return server?.status === 'online' || server?.status === 'hibernating';
}

export function restartVariant(server) {
  return serverPlayerCount(server) > 0 ? 'has-players' : 'empty';
}

export function powerAvailability(server) {
  const poweredOn = isPoweredOn(server);
  return {
    restart: { enabled: true },
    start: { enabled: !poweredOn, disabledReason: poweredOn ? '服务器为开机状态，无需开启' : '' },
    stop: { enabled: poweredOn, disabledReason: poweredOn ? '' : '服务器为关机状态，无需关闭' },
  };
}

export function buildStartConfirmText(serverName) {
  return `即将向服务器「${serverName}」发送开机指令，开机后需要一定时间完成地图加载与插件初始化，确定继续？`;
}

export function buildStopConfirmText(serverName) {
  return `即将向服务器「${serverName}」发送关机指令，在线玩家将被强制断开，确定继续？`;
}

export function buildStopFinalText() {
  return '请再次确认：关机后该服务器将停止接受玩家进入，重启需要重新开机，强制阅读 5 秒后才可执行。';
}

export function countdownLabel(base, seconds) {
  return seconds > 0 ? `${base}（${seconds}s）` : base;
}

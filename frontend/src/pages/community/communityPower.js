// 服务器电源操作：确认框 + 倒计时，下发后轮询任务终态并回显执行输出。

export const POWER_COUNTDOWN_SECS = 5;

export const POWER_ACTION = Object.freeze({
  restart: 'restart',
  forceRestart: 'force-restart',
  start: 'start',
  forceStart: 'force-start',
  stop: 'stop',
});

export const RESTART_HAS_PLAYERS_TEXT = '当前服务器内存在玩家无法进行重启服务器操作。';
export const RESTART_EMPTY_TEXT =
  '当前指令将重启服务器，可能因为服务器延迟问题没有正确的显示出服务器内是否存在玩家，请您确认服务器内没有玩家后再继续下一步。';
export const RESTART_FINAL_TEXT =
  '请再次确认是否重启该服务器：重启将断开服内连接并重载地图与插件，强制阅读 5 秒后才可执行。';
export const FORCE_RESTART_CONFIRM_TEXT =
  '强制重启将无视服内玩家直接重启服务器，在线玩家将被强制断开；可能因为服务器延迟问题没有正确的显示出服务器内是否存在玩家，请谨慎操作。';
export const FORCE_RESTART_FINAL_TEXT =
  '请再次确认是否强制重启该服务器：执行后全部玩家立即断开并重载服务器，强制阅读 5 秒后才可执行。';
export const FORCE_START_CONFIRM_TEXT =
  '即将向服务器发送强制开机指令：即使状态显示异常也会执行，适用于进程已关但状态未刷新的情况，确定继续？';
export const FORCE_START_FINAL_TEXT =
  '请再次确认是否强制开机该服务器，强制阅读 5 秒后才可执行。';

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
  // 开启：仅在线（明确在跑）时禁用；休眠/离线/未测试都可点——
  // 休眠可能是僵尸状态（进程已死但上报残留），点开启无害（LGSM 会提示已在运行），
  // 真正拿不准时还有强制开启兜底。
  const startEnabled = server?.status !== 'online';
  return {
    restart: { enabled: true },
    forceRestart: { enabled: true },
    start: { enabled: startEnabled, disabledReason: startEnabled ? '' : '服务器在线，无需开启' },
    forceStart: { enabled: true },
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

export const POWER_ACTION_LABEL = Object.freeze({
  restart: '重启服务器',
  'force-restart': '强制重启服务器',
  start: '开启服务器',
  'force-start': '强制开启服务器',
  stop: '关闭服务器',
});

// 任务终态：成功 / 失败 / 超时；pending / running 继续轮询。
export function isPowerJobTerminal(status) {
  return status === 'success' || status === 'failed' || status === 'timeout';
}

export const POWER_JOB_STATUS_TEXT = Object.freeze({
  success: '执行成功',
  failed: '执行失败',
  timeout: '执行超时',
});

// 轮询配置：LGSM 重启常需数十秒，2s 间隔、150s 上限。
export const POWER_JOB_POLL_INTERVAL_MS = 2000;
export const POWER_JOB_POLL_TIMEOUT_MS = 150000;

// 去 ANSI 转义（与后端 sanitize_job_output 同规则，兜底历史脏数据展示）。
export function stripAnsi(text) {
  return String(text ?? '').replace(/\[[0-9;?]*[ -/]*[@-~]/g, '');
}

// 清洗执行回显：去 ANSI + 折叠 \r 进度行 + 去空行。
export function cleanPowerOutput(text) {
  return String(text ?? '')
    .split('\n')
    .map((line) => stripAnsi(line.split('\r').pop() ?? '').trimEnd())
    .filter((line) => line.length > 0)
    .join('\n');
}

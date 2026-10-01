#!/usr/bin/env bash
# LumiServerAgent 一键安装脚本（游戏用户身份运行）。
#
# 必须以 LGSM 属主用户（如 steam）执行，不要用 root：
#   su - steam
#   bash install.sh
#
# 安装包由网站 `Agent控制` 页直接下发，本脚本不从 GitHub 拉取。
# LGSM 用户通常没有 systemctl 权限，因此默认用户态运行：
# 二进制与配置放在用户目录，后台进程 + cron 实现开机自启与掉线拉起。
# 如有 root 权限，可另行安装 deploy/lumi-host-agent.service 走 systemd。
#
# 前置：在网站 Agent控制 页签发安装口令（15 分钟有效，一次性）。
#
# 交互式（推荐）：直接运行，按提示填写。
# 非交互：  bash install.sh --url ... --install-token ... --lgsm-dir ... --instances ... [--yes]
set -euo pipefail

BACKEND_URL=""
INSTALL_TOKEN=""
PREFIX=""
LGSM_DIR=""
INSTANCES=""
POLL_INTERVAL="10"
ASSUME_YES=0
ALLOW_ROOT=0

usage() {
  cat <<'USAGE'
用法: bash install.sh [--url <后端地址> --install-token <安装口令> --lgsm-dir <LGSM目录> --instances <实例清单>] [--prefix <安装目录>] [--poll-interval <秒>] [--yes] [--allow-root]

  --url            网站后端地址，如 https://admin.example.com
  --install-token  网站 Agent控制 页签发的安装口令（15 分钟有效，一次性）
  --lgsm-dir       LGSM 目录，如 /home/steam（内含 csgoserver 等实例脚本）
  --instances      实例清单（逗号分隔），如 csgoserver,csgoserver-2
  --prefix         Agent 存放目录，默认 $HOME/lumi-agent
  --poll-interval  轮询间隔秒数，默认 10
  --yes            非交互：跳过二次确认（cron 自启仍会询问，除非同时给 --cron/--no-cron）
  --allow-root     允许以 root 运行（默认拒绝，请用 LGSM 属主用户执行）
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --url) BACKEND_URL="${2:-}"; shift 2 ;;
    --install-token) INSTALL_TOKEN="${2:-}"; shift 2 ;;
    --prefix) PREFIX="${2:-}"; shift 2 ;;
    --lgsm-dir) LGSM_DIR="${2:-}"; shift 2 ;;
    --instances) INSTANCES="${2:-}"; shift 2 ;;
    --poll-interval) POLL_INTERVAL="${2:-}"; shift 2 ;;
    --yes) ASSUME_YES=1; shift ;;
    --allow-root) ALLOW_ROOT=1; shift ;;
    -h|--help) usage; exit 0 ;;
    *) echo "未知参数：$1" >&2; usage; exit 1 ;;
  esac
done

if [[ "$(id -u)" -eq 0 && "$ALLOW_ROOT" -ne 1 ]]; then
  echo "请不要用 root 运行：Agent 必须以 LGSM 属主用户身份执行 ./<实例> 脚本。" >&2
  echo "请切换用户后重试，例如：su - steam" >&2
  exit 1
fi

command -v curl >/dev/null || { echo "需要 curl，请先安装" >&2; exit 1; }
command -v python3 >/dev/null || { echo "需要 python3（仅用于解析注册返回），请先安装" >&2; exit 1; }
# pgrep/pkill 用于启动检查与掉线拉起；缺失时仍可安装，只是没有自动拉起
HAVE_PGREP=1
command -v pgrep >/dev/null && command -v pkill >/dev/null || { echo "提示：未找到 pgrep/pkill，将跳过进程存活检查与自动拉起。" >&2; HAVE_PGREP=0; }

[[ -z "$PREFIX" ]] && PREFIX="$HOME/lumi-agent"
[[ -z "$LGSM_DIR" ]] && LGSM_DIR="$HOME"

prompt() {
  local tip="$1" def="$2" var="$3" input=""
  if [[ -n "$def" ]]; then
    printf '%s [%s]: ' "$tip" "$def"
  else
    printf '%s: ' "$tip"
  fi
  IFS= read -r input || true
  input="$(printf '%s' "$input" | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')"
  printf -v "$var" '%s' "${input:-$def}"
}

# 缺失的必填项走交互式补齐
[[ -z "$BACKEND_URL" ]] && prompt "网站后端地址（如 https://admin.example.com）" "" BACKEND_URL
[[ -z "$INSTALL_TOKEN" ]] && prompt "网站签发的安装口令（15 分钟有效）" "" INSTALL_TOKEN
[[ -z "$LGSM_DIR" ]] && prompt "LGSM 目录" "$HOME" LGSM_DIR
if [[ -z "$INSTANCES" ]]; then
  # 自动探测候选：LGSM 实例脚本本身不带后缀，只认无点号的可执行文件名
  # （rcon_restart.py、linuxgsm.sh 这类带后缀的一律排除）
  candidates="$(find "$LGSM_DIR" -maxdepth 1 -type f -executable -printf '%f\n' 2>/dev/null | grep -E '^[A-Za-z0-9_-]+$' | sort | tr '\n' ',' | sed 's/,$//')"
  if [[ -n "$candidates" ]]; then
    echo "探测到疑似实例：$candidates"
    prompt "实例清单（逗号分隔，回车沿用探测结果）" "$candidates" INSTANCES
  else
    prompt "实例清单（逗号分隔，如 csgoserver,csgoserver-2）" "" INSTANCES
  fi
fi

# 校验
[[ "$BACKEND_URL" =~ ^https?://[^[:space:]]+$ ]] || { echo "后端地址格式无效：$BACKEND_URL" >&2; exit 1; }
[[ -z "$INSTALL_TOKEN" ]] && { echo "安装口令不能为空" >&2; exit 1; }
[[ -d "$LGSM_DIR" ]] || { echo "LGSM 目录不存在：$LGSM_DIR" >&2; exit 1; }
[[ "$POLL_INTERVAL" =~ ^[0-9]+$ ]] && [[ "$POLL_INTERVAL" -ge 1 ]] || { echo "轮询间隔必须为正整数秒" >&2; exit 1; }
missing=""
for raw in ${INSTANCES//,/ }; do
  inst="$(printf '%s' "$raw" | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')"
  [[ -z "$inst" ]] && continue
  [[ "$inst" =~ ^[A-Za-z0-9_-]{1,32}$ ]] || { echo "实例名格式无效：$inst（仅允许字母数字下划线中划线，1~32 位）" >&2; exit 1; }
  [[ -x "$LGSM_DIR/$inst" ]] || missing="$missing $inst"
done
[[ -z "${missing// }" ]] || { echo "以下实例在 LGSM 目录中不是可执行文件：$missing" >&2; echo "请确认实例名与脚本文件名一字不差。" >&2; exit 1; }
# 归一化清单（去空格）
INSTANCES="$(printf '%s' "$INSTANCES" | tr -d '[:space:]')"

BACKEND_URL="${BACKEND_URL%/}"
BIN_DIR="$PREFIX/bin"
ENV_FILE="$PREFIX/agent.env"
LOG_FILE="$PREFIX/agent.log"

echo "----------------------------------------"
echo "安装目录：  $PREFIX"
echo "后端地址：  $BACKEND_URL"
echo "LGSM 目录： $LGSM_DIR"
echo "实例清单：  $INSTANCES"
echo "轮询间隔：  ${POLL_INTERVAL}s"
echo "运行身份：  $(id -un)（LGSM 属主）"
echo "----------------------------------------"
if [[ "$ASSUME_YES" -ne 1 ]]; then
  printf '确认安装？[Y/n]: '
  IFS= read -r confirm || true
  case "$confirm" in
    ""|[Yy]*) ;;
    *) echo "已取消。" >&2; exit 1 ;;
  esac
fi

HOSTNAME="$(hostname 2>/dev/null || echo unknown)"

echo "==> 向网站注册本机（安装口令换长期口令）"
REGISTER_RESP="$(curl -fsSL -X POST "${BACKEND_URL}/api/host-agent/register" \
  -H 'Content-Type: application/json' \
  -d "$(python3 -c 'import json,sys; print(json.dumps({"install_token": sys.argv[1], "hostname": sys.argv[2], "lgsm_dir": sys.argv[3], "instances": [i.strip() for i in sys.argv[4].split(",") if i.strip()]}))' "$INSTALL_TOKEN" "$HOSTNAME" "$LGSM_DIR" "$INSTANCES")")"
AGENT_TOKEN="$(printf '%s' "$REGISTER_RESP" | python3 -c 'import json,sys; print(json.load(sys.stdin)["agent_token"])')"
[[ -z "$AGENT_TOKEN" ]] && { echo "注册失败：后端未返回 Agent 口令" >&2; exit 1; }
echo "    注册成功，长期口令已签发（仅保存到配置文件，不再显示）。"
INSTALL_TOKEN=""

mkdir -p "$BIN_DIR"
AUTH_HEADER="Authorization: Bearer ${AGENT_TOKEN}"

echo "==> 下载 Agent 二进制与 service 文件（来源：${BACKEND_URL}）"
curl -fsSL -H "$AUTH_HEADER" \
  -o "${BIN_DIR}/lumi-server-agent" \
  "${BACKEND_URL}/api/host-agent/download/lumi-server-agent-x86_64"
chmod 755 "${BIN_DIR}/lumi-server-agent"
curl -fsSL -H "$AUTH_HEADER" \
  -o "${PREFIX}/lumi-host-agent.service" \
  "${BACKEND_URL}/api/host-agent/download/lumi-host-agent.service"

echo "==> 写入 ${ENV_FILE}（权限 600）"
cat > "$ENV_FILE" <<EOF
LUMI_BACKEND_URL=${BACKEND_URL}
LUMI_AGENT_TOKEN=${AGENT_TOKEN}
LUMI_LGSM_DIR=${LGSM_DIR}
LUMI_INSTANCES=${INSTANCES}
LUMI_POLL_INTERVAL_SECS=${POLL_INTERVAL}
EOF
chmod 600 "$ENV_FILE"
AGENT_TOKEN=""

start_agent() {
  # 已在运行则先停掉，避免双实例抢任务
  if [[ "$HAVE_PGREP" -eq 1 ]]; then
    pkill -f "lumi-server-agent --config ${ENV_FILE}" 2>/dev/null || true
    sleep 1
  fi
  setsid "${BIN_DIR}/lumi-server-agent" --config "$ENV_FILE" >>"$LOG_FILE" 2>&1 < /dev/null &
  sleep 2
  if [[ "$HAVE_PGREP" -eq 1 ]] && ! pgrep -f "lumi-server-agent --config ${ENV_FILE}" >/dev/null; then
    echo "    启动失败，请查看日志：$LOG_FILE" >&2
    tail -20 "$LOG_FILE" >&2 || true
    exit 1
  fi
  echo "    Agent 已在后台启动，日志：$LOG_FILE"
}

echo "==> 后台启动 Agent（用户态，无需 systemctl）"
start_agent

# cron 自启 + 掉线拉起（LGSM 用户无 systemctl 时的替代方案）
if [[ "$HAVE_PGREP" -eq 1 ]] && command -v crontab >/dev/null; then
  printf '设置开机自启与掉线拉起（cron）？[Y/n]: '
  IFS= read -r want_cron || true
  case "$want_cron" in
    ""|[Yy]*)
      (crontab -l 2>/dev/null | grep -v "lumi-server-agent --config ${ENV_FILE}" || true; cat <<EOF
@reboot ${BIN_DIR}/lumi-server-agent --config ${ENV_FILE} >>${LOG_FILE} 2>&1
* * * * * pgrep -f "lumi-server-agent --config ${ENV_FILE}" >/dev/null || ${BIN_DIR}/lumi-server-agent --config ${ENV_FILE} >>${LOG_FILE} 2>&1
EOF
      ) | crontab -
      echo "    已写入 crontab：@reboot 自启 + 每分钟掉线拉起。"
      ;;
    *) echo "    已跳过 cron 设置；机器重启后需手动重新后台启动。" ;;
  esac
else
  echo "    跳过 cron 设置（缺 pgrep 或 crontab）；机器重启后需手动重新后台启动。"
fi

echo "----------------------------------------"
echo "安装完成。常用命令："
echo "  查看日志：  tail -f $LOG_FILE"
echo "  重启 Agent：pkill -f \"lumi-server-agent --config ${ENV_FILE}\" 后重新后台启动"
echo "  停止 Agent：pkill -f \"lumi-server-agent --config ${ENV_FILE}\""
echo "如有 root 权限，可改用 ${PREFIX}/lumi-host-agent.service 走 systemd（User 填当前用户）。"

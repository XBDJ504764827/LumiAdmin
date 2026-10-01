#!/usr/bin/env bash
# LumiServerAgent 一键安装脚本。
# 安装包由网站 `Agent控制` 页直接下发，本脚本不从 GitHub 拉取。
#
# 前置：在网站 Agent控制 页签发安装口令（15 分钟有效，一次性）。
#
# 用法示例：
#   sudo bash install.sh \
#     --url https://admin.example.com \
#     --install-token <网站签发的安装口令> \
#     --lgsm-dir /home/steam/lgsm \
#     --instances csgoserver,csgo2server
#
# 流程：注册（安装口令换长期 Agent 口令）→ 下载二进制与 service 文件
#       （持长期口令）→ 写配置 → systemd 拉起。
set -euo pipefail

BACKEND_URL=""
INSTALL_TOKEN=""
PREFIX="/usr/local"
LGSM_DIR=""
INSTANCES=""

usage() {
  cat <<'USAGE'
用法: install.sh --url <后端地址> --install-token <安装口令> --lgsm-dir <LGSM目录> --instances <实例清单> [--prefix <安装前缀>]

  --url            网站后端地址，如 https://admin.example.com
  --install-token  网站 Agent控制 页签发的安装口令（15 分钟有效，一次性）
  --lgsm-dir       LGSM 目录，如 /home/steam/lgsm
  --instances      实例清单（逗号分隔），如 csgoserver,csgo2server
  --prefix         安装前缀，默认 /usr/local
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --url) BACKEND_URL="${2:-}"; shift 2 ;;
    --install-token) INSTALL_TOKEN="${2:-}"; shift 2 ;;
    --prefix) PREFIX="${2:-}"; shift 2 ;;
    --lgsm-dir) LGSM_DIR="${2:-}"; shift 2 ;;
    --instances) INSTANCES="${2:-}"; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "未知参数：$1" >&2; usage; exit 1 ;;
  esac
done

[[ -z "$BACKEND_URL" ]] && { echo "缺少 --url（网站后端地址）" >&2; exit 1; }
[[ -z "$INSTALL_TOKEN" ]] && { echo "缺少 --install-token（网站签发的安装口令）" >&2; exit 1; }
[[ -z "$LGSM_DIR" ]] && { echo "缺少 --lgsm-dir（LGSM 目录）" >&2; exit 1; }
[[ -z "$INSTANCES" ]] && { echo "缺少 --instances（实例清单）" >&2; exit 1; }
[[ -d "$LGSM_DIR" ]] || { echo "LGSM 目录不存在：$LGSM_DIR" >&2; exit 1; }

command -v curl >/dev/null || { echo "需要 curl，请先安装" >&2; exit 1; }
command -v python3 >/dev/null || { echo "需要 python3（仅用于解析注册返回），请先安装" >&2; exit 1; }
command -v systemctl >/dev/null || { echo "需要 systemd，本脚本只支持 systemd 系统" >&2; exit 1; }

BACKEND_URL="${BACKEND_URL%/}"
HOSTNAME="$(hostname 2>/dev/null || echo unknown)"

echo "==> 向网站注册本机（安装口令换长期口令）"
REGISTER_RESP="$(curl -fsSL -X POST "${BACKEND_URL}/api/host-agent/register" \
  -H 'Content-Type: application/json' \
  -d "$(python3 -c 'import json,sys; print(json.dumps({"install_token": sys.argv[1], "hostname": sys.argv[2], "lgsm_dir": sys.argv[3], "instances": [i.strip() for i in sys.argv[4].split(",") if i.strip()]}))' "$INSTALL_TOKEN" "$HOSTNAME" "$LGSM_DIR" "$INSTANCES")")"
AGENT_TOKEN="$(printf '%s' "$REGISTER_RESP" | python3 -c 'import json,sys; print(json.load(sys.stdin)["agent_token"])')"
[[ -z "$AGENT_TOKEN" ]] && { echo "注册失败：后端未返回 Agent 口令" >&2; exit 1; }
echo "    注册成功，长期口令已签发（不再显示）。"

AUTH_HEADER="Authorization: Bearer ${AGENT_TOKEN}"

echo "==> 下载 Agent 二进制与 service 文件（来源：${BACKEND_URL}）"
curl -fsSL -H "$AUTH_HEADER" \
  -o "${PREFIX}/bin/lumi-server-agent" \
  "${BACKEND_URL}/api/host-agent/download/lumi-server-agent-x86_64"
chmod 755 "${PREFIX}/bin/lumi-server-agent"
curl -fsSL -H "$AUTH_HEADER" \
  -o /etc/systemd/system/lumi-host-agent.service \
  "${BACKEND_URL}/api/host-agent/download/lumi-host-agent.service"

echo "==> 写入 /etc/lumi-agent.env（权限 600）"
cat > /etc/lumi-agent.env <<EOF
LUMI_BACKEND_URL=${BACKEND_URL}
LUMI_AGENT_TOKEN=${AGENT_TOKEN}
LUMI_LGSM_DIR=${LGSM_DIR}
LUMI_INSTANCES=${INSTANCES}
EOF
chmod 600 /etc/lumi-agent.env

echo "==> 启用并启动服务"
systemctl daemon-reload
systemctl enable --now lumi-host-agent
systemctl --no-pager status lumi-host-agent

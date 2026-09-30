# backend/agent（vendored）

本目录存放由网站后端直接托管下发的宿主机 Agent 文本文件，
通过 `include_str!` 编进后端二进制，经 `GET /api/host-agent/download/:filename` 下发。
安装不走 GitHub。

| 文件 | 来源 | 说明 |
|------|------|------|
| `install.sh` | `LumiServerAgent` 仓库 `scripts/install.sh` | 一键安装脚本 |
| `lumi-host-agent.service` | `LumiServerAgent` 仓库 `deploy/lumi-host-agent.service` | systemd 单元模板 |

同步方式：Agent 仓库变更后，复制对应文件到本目录并提交。
二进制包（`lumi-server-agent-x86_64`）体积大不入库，
待发布流程（CI 构建产物上传）落地后再由下载接口提供，当前返回 501。

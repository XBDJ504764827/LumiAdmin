# LumiAdmin

CS:GO / CS2 社区服务器综合管理系统，提供玩家封禁管理、社区组管理、在线玩家监控、白名单控制、玩家信息 API 分发、审计日志等功能，并通过 RCON / 插件 API 与游戏服务器实时联动。

> 游戏服务器 SourceMod 插件已独立为 [LumiAdmin-plugins](https://github.com/LumiAdmin/LumiAdmin-plugins) 仓库，本仓库不再包含插件源码与构建。


---

## 技术栈

| 模块 | 技术 |
|------|------|
| 前端 | React 18 + Vite + React Router |
| 后端 | Rust (Axum) + SQLx (PostgreSQL) + Tokio |
| 游戏插件 | SourceMod (SourcePawn)，独立仓库 [LumiAdmin-plugins](https://github.com/LumiAdmin/LumiAdmin-plugins) |
| CI/CD | GitHub Actions + SSH + rsync |

---

## 项目结构

```
LumiAdmin/
├── backend/                      # Rust 后端服务
│   ├── src/
│   │   ├── main.rs               # 入口：启动 HTTP 服务与后台任务
│   │   ├── config.rs             # 环境变量配置
│   │   ├── db.rs                 # 数据库连接与 Schema 迁移
│   │   ├── models.rs             # 公共数据模型
│   │   ├── rcon.rs               # RCON 协议实现
│   │   ├── a2s.rs                # A2S 查询实现
│   │   ├── http_client.rs        # 全局 HTTP 客户端
│   │   ├── password.rs           # 密码哈希工具
│   │   ├── auth/                 # 认证 & 会话
│   │   ├── routes/               # API 路由层
│   │   └── services/             # 业务逻辑层
│   ├── Cargo.toml
│   └── .env                      # 环境变量（不入库）
├── frontend/                     # React 前端
│   ├── src/
│   │   ├── App.jsx               # 根组件（路由、认证守卫）
│   │   ├── main.jsx              # 入口
│   │   ├── lib/api.js            # API 客户端封装
│   │   ├── components/           # 布局组件（AppShell、侧边栏）
│   │   ├── pages/                # 页面模块
│   │   │   ├── dashboard/        # 仪表盘
│   │   │   ├── community/        # 社区组管理
│   │   │   ├── whitelist/        # 白名单管理
│   │   │   ├── ban/              # 封禁管理
│   │   │   ├── users/            # 用户管理
│   │   │   ├── logs/             # 操作日志
│   │   │   ├── audit/            # 审计日志
│   │   │   ├── api/              # API 接口文档 & 玩家API配置
│   │   │   ├── external/         # 外部服务器管理
│   │   │   └── public/           # 公开页面（白名单申请、封禁公示等）
│   │   ├── shared/               # 通用 UI 组件（Modal、Toast、Pagination 等）
│   │   ├── state/                # 状态管理（auth、theme）
│   │   └── styles.css            # 全局样式
│   ├── index.html
│   ├── vite.config.js
│   └── package.json
├── workers/                       # Cloudflare Workers（R2 文件网关，供后端与游戏插件共用）
├── .github/workflows/deploy.yml  # CI/CD 自动部署
└── docs/                         # 文档
```

> SourceMod 游戏插件（`cngokz-*` 系列）源码与构建脚本已移至独立仓库 [LumiAdmin-plugins](https://github.com/LumiAdmin/LumiAdmin-plugins)。

---

## 功能模块

### 核心管理

| 模块 | 说明 |
|------|------|
| **仪表盘** | 服务器状态总览（在线/离线）、核心数据统计、服务器性能指标（FPS、CPU、Tickrate）、白名单统计、管理员预览 |
| **社区组管理** | 社区组 CRUD、服务器 CRUD（含 RCON 连接测试）、在线玩家实时查看、Token 管理、访问限制配置（白名单模式 / Rating / Steam 等级门槛 / 中高风险账号拦截）、RCON 远程命令执行 |
| **白名单管理** | 白名单审核大厅（待审核/已通过/未通过三个标签页）、全球封禁记录检测（KZTimerGlobal API）、手动添加白名单、Steam 名称刷新、全球封禁玩家审核强制填写理由、低风险自动通过开关（低风险玩家申请满 3 小时无人审核自动通过） |
| **封禁管理** | 玩家封禁/解封、Steam/IP 封禁属性、时长设置（临时/永久）、到期自动解封、封禁公示 |
| **用户管理** | 管理员账户 CRUD、权限组（developer/admin/normal）、密码管理、账号启用/禁用、会话管理 |

### 系统功能

| 模块 | 说明 |
|------|------|
| **玩家信息 API** | Webhook 分发在线玩家数据、自定义 API 端点（公开/密钥访问）、外部服务器数据聚合、地图等级查询 |
| **外部服务器** | 第三方服务器管理、A2S 查询（失败时 RCON fallback）采集玩家数据、服务器状态监控 |
| **操作日志** | 管理员操作追踪（按模块、操作人、时间范围检索） |
| **审计日志** | 详细审计记录 |
| **API 接口文档** | 后端所有 API 端点一览 |

### 公开页面

| 页面 | 说明 |
|------|------|
| 白名单申请 | 玩家自助提交白名单申请（SteamID 解析） |
| 白名单公示 | 已通过白名单公开展示 |
| 封禁公示 | 公开播封记录查看 |

### SourceMod 插件

游戏服务器侧 `cngokz-*` 系列插件（core / server / sync / recordguard / global）已独立为 [LumiAdmin-plugins](https://github.com/LumiAdmin/LumiAdmin-plugins) 仓库，请前往该仓库获取源码、构建与部署文档。

---

## 系统架构

```
┌──────────────────┐     ┌──────────────────┐     ┌──────────────┐
│   React 前端      │────▶│   Axum 后端       │────▶│  PostgreSQL  │
│   (Vite Dev)      │◀────│   (Rust)         │◀────│  数据库       │
└──────────────────┘     └────────┬─────────┘     └──────────────┘
                                  │
                    ┌─────────────┼─────────────┐
                    ▼             ▼             ▼
             ┌──────────┐  ┌──────────┐  ┌──────────┐
             │ RCON 轮询 │  │ Webhook  │  │ Steam    │
             │ 定时采集   │  │ 分发     │  │ API 代理 │
             └──────────┘  └──────────┘  └──────────┘
                    ▲
                    │
            SourceMod 插件（LumiAdmin-plugins）
            上报 / 封禁 / 异常记录
```

### 后台定时任务

| 任务 | 周期 | 说明 |
|------|------|------|
| Webhook 分发 | 可配置（默认 30s） | 将在线玩家数据推送到配置的 Webhook URL |
| 过期封禁检查 | 60s | 自动解封到期的封禁记录 |
| Steam 名称刷新 | 6h | 批量更新白名单玩家的 Steam 昵称 |
| Session 清理 | 10min | 清理过期的用户会话 |
| 外部服务器轮询 | 120s（可配置） | RCON 采集外部服务器玩家数据 |
| 休眠服务器兑底轮询 | 60s（可配置） | 空服休眠时插件无法上报，后端通过 RCON 执行 status/stats 保持服务器状态与在线数据持续刷新（默认开启，可用 `HIBERNATION_POLL_ENABLED=false` 关闭） |
| 过期服务器清理 | 300s | 清理超时未上报的在线玩家并标记服务器为休眠/待上报 |
| 地图等级同步 | 6h | 从 MySQL 同步地图难度等级数据 |
| 限流器清理 | 60s | 清理过期的限流计数器 |
| LumiBot 事件上报 | 1800s（可配置） | 白名单申请先写入持久化队列，再由后台异步发送、重试和死信处理 |
| 白名单低风险自动通过 | 60s | 低风险（无本地/全球封禁、无同 IP 高风险关联）玩家申请满 3 小时无人审核自动通过；中高风险（含全球封禁）等待管理员审核；通过后主动刷新白名单缓存 |

---

## 进服准入规则

游戏服务器插件在玩家进服时调用 `POST /api/plugin/access/check`，后端按以下顺序判定，
所有判定都以玩家账号（SteamID64）为准：

1. **封禁拦截**：账号自身或上报 IP 命中有效封禁 → 直接拒绝，白名单不可豁免；
2. **中高风险账号拦截**（服务器级开关 `servers.risk_block_enabled`，默认开启）：
   账号存在封禁类风险信号时视为中/高风险，没有白名单一律拒绝，
   判定信号与后台「玩家风险档案」完全一致：
   - 账号自身存在有效本地封禁，或由全球封禁同步生成的本地封禁，或账号自身存在有效全球封禁；
   - 同 IP 关联账号（本次上报 IP 与账号历史 IP）存在有效本地/全球封禁。
   持有已通过的白名单（`whitelist_requests.status = 'approved'`）即可豁免本项；
3. **服务器进服模式**：白名单模式 / 进入限制（Rating、Steam 等级）/ CS 优先账户，
   开启的模式之间为 OR，满足任意一项即可进入。

> 中高风险分级沿用风险档案的 `action`：`allow`=低风险、`warn`=中风险、
> `require_force` / `deny`=高风险；进服检查只统计封禁类信号，不统计白名单拒绝次数等
> 次要信号。命中明细写入进服日志的拒绝原因（玩家侧只看到统一提示）。
>
> 该判定需要实时查询数据库：后端降级到访问快照（`snapshot_fallback`）或快照内置的插件
> 本地判定时，只校验快照中的封禁 / 白名单 / 进服模式，不包含中高风险拦截。
> 关闭某台服务器的中高风险拦截后，该服务器行为与改造前一致。

---

## 权限体系

| 角色 | 权限范围 |
|------|---------|
| `developer` | 全部权限：用户管理、封禁管理、白名单管理、社区组管理、任意 RCON 命令、Steam 名称刷新、API 配置 |
| `admin` | 封禁管理、白名单管理（含手动添加）、社区组管理、RCON 白名单命令、API 配置 |
| `normal` | 白名单审核、封禁查看（不可增删改）、RCON 白名单命令 |

---

## 快速开始

### 环境要求

- Node.js >= 20
- Rust stable（推荐通过 rustup 安装）
- PostgreSQL >= 14
- CS:GO / CS2 服务器 + SourceMod（可选，游戏插件见 [LumiAdmin-plugins](https://github.com/LumiAdmin/LumiAdmin-plugins)）

### 前端

```bash
cd frontend
npm install
npm run dev      # 开发服务器 → http://localhost:5173
npm run build    # 生产构建 → dist/
```

### 后端

```bash
cd backend
cp .env.example .env   # 然后编辑 .env 填入实际配置
cargo run               # 开发运行（自动迁移数据库 Schema）
cargo build --release   # 生产构建 → target/release/manger-backend
```

首次启动会自动创建数据库表结构和初始管理员账户。

---

## 环境变量

### 必需配置

| 变量 | 说明 | 示例 |
|------|------|------|
| `DATABASE_URL` | PostgreSQL 连接字符串 | `postgres://user:pass@localhost:5432/manger` |

### 基础配置

| 变量 | 默认值 | 说明 |
|------|--------|------|
| `APP_ENV` | `development` | 运行环境；设置为 `production` 时会启用更严格的启动校验 |
| `BIND_ADDR` | `0.0.0.0:3001` | 服务监听地址 |
| `DEV_USERNAME` | `dev` | 初始管理员用户名 |
| `DEV_PASSWORD` | `change-me` | 初始管理员密码；生产环境必须设置为非默认强密码 |
| `SESSION_TTL_HOURS` | `24` | 会话有效期（小时） |
| `CORS_ORIGIN` | 允许所有来源 | 前端域名（生产环境务必设置） |

生产环境启动校验：

- `APP_ENV=production` 时必须设置 `CORS_ORIGIN`。
- `DEV_PASSWORD` 不能使用 `change-me` 或 `dev123`。
- Worker 网关模式必须同时配置 `R2_WORKER_URL`、`R2_WORKER_API_KEY`、`R2_WORKER_SIGNING_KEY`。
- 兼容 S3 模式如果填写任意 S3 字段，则 `R2_ENDPOINT`、`R2_BUCKET`、`R2_ACCESS_KEY_ID`、`R2_SECRET_ACCESS_KEY` 必须全部配置。
- `MAX_REQUEST_BODY_BYTES` 如果小于或等于 `APPEAL_FILE_MAX_SIZE_MB` 对应字节数，会自动修正为文件上限加 10MB。

### Steam API

| 变量 | 说明 |
|------|------|
| `STEAM_API_KEY` | Steam Web API Key（用于解析 SteamID） |
| `STEAM_WEB_KEY` | Steam Web Key（优先级高于 STEAM_API_KEY） |
| `STEAMCHINA_PROFILE_KEY` | SteamChina 个人资料 API Key |
| `STEAMCHINA_LEVEL_KEY` | SteamChina 等级 API Key |

### 数据库连接池

| 变量 | 默认值 | 说明 |
|------|--------|------|
| `DB_MAX_CONNECTIONS` | `20` | 最大连接数 |
| `DB_MIN_CONNECTIONS` | `2` | 最小空闲连接数 |
| `DB_ACQUIRE_TIMEOUT_SECS` | `10` | 获取连接超时 |
| `DB_IDLE_TIMEOUT_SECS` | `600` | 空闲连接超时 |

### HTTP 客户端

| 变量 | 默认值 | 说明 |
|------|--------|------|
| `HTTP_TIMEOUT_SECS` | `60` | 请求超时（全局 HTTP 客户端兜底） |
| `HTTP_CONNECT_TIMEOUT_SECS` | `5` | 连接超时 |
| `REQUEST_TIMEOUT_SECS` | `60` | 全局请求超时 |
| `MAX_REQUEST_BODY_BYTES` | `APPEAL_FILE_MAX_SIZE_MB + 10MB` | 请求体大小限制，需高于申诉文件大小上限 |

### R2 文件存储

| 变量 | 默认值 | 说明 |
|------|--------|------|
| `APPEAL_FILE_MAX_SIZE_MB` | `100` | 单个申诉辅助文件大小上限 |
| `R2_WORKER_URL` | 空 | 生产环境文件网关，例如 `https://cngokz.iquankz.cn` |
| `R2_WORKER_API_KEY` | 空 | Worker 内部上传密钥，必须与 Worker `API_KEY` 一致 |
| `R2_WORKER_SIGNING_KEY` | 空 | 短时下载地址签名密钥，必须与 Worker `DOWNLOAD_SIGNING_KEY` 一致 |
| `R2_ENDPOINT` | 空 | Cloudflare R2 S3 API endpoint，例如 `https://<account>.r2.cloudflarestorage.com` |
| `R2_BUCKET` | 空 | R2 存储桶名称 |
| `R2_ACCESS_KEY_ID` | 空 | R2 S3 访问密钥 ID |
| `R2_SECRET_ACCESS_KEY` | 空 | R2 S3 机密访问密钥 |
| `R2_TOKEN_VALUE` | 空 | 可选，保留给后续 R2 管理 API 使用 |

生产环境推荐仅配置 `R2_WORKER_*`，S3 配置用于兼容旧部署或开发环境。未配置完整
R2 信息时，业务记录本身仍可提交，只有证据文件上传不可用。

### 其他

| 变量 | 说明 |
|------|------|
| `MYSQL_DATABASE_URL` | MySQL 连接字符串（用于地图等级同步，可选） |

### LumiBot（QQ 机器人事件上报）

| 变量 | 默认值 | 说明 |
|------|--------|------|
| `QQ_INTEGRATION_TOKEN` | 空（禁用） | LumiBot 调用 QQ 审批/统计接口的令牌，与 LumiBot 的 `LUMIADMIN_QQ_TOKEN` 一致 |
| `LUMI_BOT_API_URL` | 空（禁用） | LumiBot 事件接收中心地址，如 `http://127.0.0.1:8080`；与 `LUMI_BOT_API_KEY` 同时配置后启用 |
| `LUMI_BOT_API_KEY` | 空（禁用） | LumiBot 分配的 API Key（`X-API-Key` 请求头，建议向 LumiBot 申请专属 `key-admin`） |
| `LUMI_BOT_SYNC_INTERVAL_SECS` | `1800` | 队列集中上报周期（秒），即每 30 分钟批量上报一次 |
| `LUMI_BOT_MAX_ATTEMPTS` | `5` | 单条事件最大重试次数，超过后标记 `failed`（死信），退避周期后自动复活重试 |
| `LUMI_BOT_BATCH_SIZE` | `100` | 每轮最多上报的事件条数 |
| `LUMI_BOT_FAILED_RETRY_SECS` | `86400` | 死信复活退避（秒）：`failed` 超过该时长后自动重置为 `pending` 再试 |
| `LUMI_BOT_FAILED_MAX_AGE_SECS` | `604800` | 死信最长保留（秒）：超过 7 天仍无法送达的标记为 `expired`，不再重试 |

启用后，玩家在公开页面提交的白名单申请会写入 `lumi_bot_event_queue` 队列，
后台任务按周期集中调用 `POST {LUMI_BOT_API_URL}/api/v1/events`
（`source: LumiAdmin`，`event_type: WHITELIST_REQUEST_CREATED`）上报，
由 LumiBot 再通知 QQ 管理员/用户。

**事件不会因 LumiBot 短暂停机而丢失**：上报失败按指数退避重试
（间隔最长 1 小时）；重试耗尽后进入死信，仍会在 24 小时后自动复活重试，
直到送达或超过 7 天标记为 `expired`。死信/过期数量可在
`GET /api/ops/lumi-bot` 的 `queue` 字段中监控。

LumiBot 点击审批调用 `POST /api/integration/qq/whitelist/:id/review`，请求体包含
`action`、审批人 `openid`、QQ `interaction_id`、可选 `reason` 和 `force`。

LumiBot 的 `/wl <steamid64/steamid2>` 指令调用
`GET /api/integration/qq/whitelist/status?steam_input=...` 查询白名单状态。
该接口支持 SteamID64、SteamID2 和 Steam 个人主页 URL，返回指定账号的全部历史
白名单记录（状态、时间和拒绝原因），不返回联系方式、审核人等敏感字段。
LumiAdmin 使用 `interaction_id` 生成唯一幂等键，在同一 PostgreSQL 事务中更新
白名单申请并写入 `audit_logs`；重复请求返回第一次审批的 `audit_id` 与结果。

### 数据库迁移

后端启动时会先执行旧版幂等 Schema 兼容迁移，再执行 `backend/migrations/` 下的 SQLx 正式迁移文件。后续新增或修改数据库结构时，优先添加带时间戳的 SQL 迁移文件，旧版代码迁移只作为兼容已有部署的过渡层保留。生产部署建议在切换应用前单独执行迁移，并在 CI 同时验证全新数据库和旧版数据库升级路径。

访问控制缓存通过 PostgreSQL `LISTEN/NOTIFY` 在封禁、白名单、服务器配置或玩家访问资料变化后立即刷新，固定周期刷新仅作为断线兜底。服务器状态和访问快照刷新失败时会记录观测指标，短时间内保留旧缓存，超过宽限期则重新读取数据库。

---

## API 端点概览

### 认证

| 方法 | 路径 | 说明 |
|------|------|------|
| POST | `/api/auth/login` | 登录 |
| POST | `/api/auth/logout` | 登出当前会话 |
| POST | `/api/auth/logout-all` | 登出所有设备 |
| GET | `/api/auth/me` | 获取当前用户信息 |

### 社区组管理

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/api/community/servers` | 获取所有服务器列表 |
| POST | `/api/community/groups` | 创建社区组 |
| DELETE | `/api/community/groups/:id` | 删除社区组 |
| PUT | `/api/community/groups/:id/access` | 更新社区组访问设置 |
| POST | `/api/community/groups/:id/servers` | 添加服务器 |
| PUT | `/api/community/servers/:id` | 更新服务器 |
| DELETE | `/api/community/servers/:id` | 删除服务器 |
| GET | `/api/community/servers/:id/players` | 获取在线玩家 |
| POST | `/api/community/servers/:id/rcon` | 执行 RCON 命令 |

### 白名单

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/api/whitelist` | 白名单列表（分页、搜索、状态过滤） |
| POST | `/api/whitelist/manual` | 手动添加白名单 |
| POST | `/api/whitelist/:id/approve` | 通过审核 |
| POST | `/api/whitelist/:id/reject` | 拒绝审核 |
| POST | `/api/whitelist/:id/restore` | 恢复通过 |
| POST | `/api/whitelist/:id/revoke` | 撤销白名单 |

### 封禁管理

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/api/bans` | 封禁列表 |
| POST | `/api/bans` | 添加封禁 |
| PUT | `/api/bans/:id` | 编辑封禁 |
| DELETE | `/api/bans/:id` | 删除封禁 |
| POST | `/api/bans/:id/unban` | 解封 |

### 插件 API（游戏服务器调用）

| 方法 | 路径 | 说明 |
|------|------|------|
| POST | `/api/plugin/online-players/report` | 上报在线玩家 |
| POST | `/api/plugin/bans` | 插件提交封禁 |
| POST | `/api/plugin/bans/poll` | 轮询活跃封禁 |
| POST | `/api/plugin/bans/check` | 检查玩家封禁状态 |
| POST | `/api/plugin/access/check` | 检查玩家进服权限 |
| POST | `/api/plugin/access/snapshot` | 获取权限快照 |

### 玩家信息 API

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/api/player-api/players` | 当前所有在线玩家 |
| GET | `/api/player-api/config` | 获取 Webhook 配置 |
| PUT | `/api/player-api/config` | 更新 Webhook 配置 |
| GET | `/webhook/:path` | 公开 API 端点（按配置的路径访问） |

### 公开页面

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/api/public/whitelist` | 白名单公示 |
| POST | `/api/public/whitelist` | 提交白名单申请 |
| GET | `/api/public/bans` | 封禁公示 |
| POST | `/api/public/steam/resolve` | SteamID 解析 |
| GET | `/api/public/global-bans/:steamid64` | 查询全球封禁记录 |
| POST | `/api/public/global-bans/batch` | 批量查询全球封禁 |

---

## 部署

4. 通过 SSH + rsync 部署到目标服务器
5. 自动重启后端服务（游戏插件部署见 [LumiAdmin-plugins](https://github.com/LumiAdmin/LumiAdmin-plugins)）

### 后端 systemd 服务配置

生产环境通过 systemd 管理后端进程（开机自启、崩溃自动拉起、优雅关闭）。
仓库提供标准服务单元模板：[`deploy/manger-backend.service`](deploy/manger-backend.service)。

目标服务器首次部署或检查配置时：

1. 复制模板到 `/etc/systemd/system/manger-backend.service`，按实际部署路径
   修改 `User` / `WorkingDirectory` / `ExecStart`（`WorkingDirectory` 必须指向
   `.env` 所在目录，后端通过 dotenvy 从工作目录加载配置）；
2. `systemctl daemon-reload && systemctl enable --now manger-backend` 启用
   开机自启与崩溃自动拉起（`Restart=on-failure`）；
3. 日志通过 `journalctl -u manger-backend` 查看（stdout 全部进 journald）。

注意：

- 后端已实现 SIGTERM 优雅关闭（停机时刷写最终访问快照），unit 中
  `TimeoutStopSec=30` 预留了刷写时间，请勿改回默认之外的过短值；
- `systemctl restart` 只重启当前进程，`enable` 才是开机自启；新服务器部署后
  请确认 `systemctl is-enabled manger-backend` 为 enabled；
- 应用内后台循环（缓存刷新、全球封禁同步、白名单自动通过等）由
  `task_runtime::spawn_persistent` 提供 panic 隔离与自动重启，与 systemd 形成
  任务级 + 进程级两层保障；若发现服务状态异常，可用
  `GET /api/ops/overview`（含后台任务运行指标）与 `journalctl` 结合排查。

---

## 安全说明

- 所有管理 API 需要Bearer Token 认证
- 前端 401 全局拦截，Token 过期自动跳转登录
- RCON 采用白名单模式：`developer` 可执行单条任意命令；其他管理员只允许 `status`、`stats`、`version`、`listplayers`、`sm_version`、`sm version`、`sm plugins list` 和 `sm plugins info`。命令长度、单条命令格式、超时和输出长度均有限制，审计日志不会记录完整敏感命令。
- 插件 API 通过 report_token + port 双重认证
- Webhook 密钥支持常量时间比较，防止时序攻击
- 公开 API 有 IP 级别的速率限制
- 全球封禁玩家审核时强制管理员填写通过/拒绝理由

---

## 许可证

私有项目，未授权禁止使用。

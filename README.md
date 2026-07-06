# LumiAdmin

CS:GO / CS2 社区服务器综合管理系统，提供玩家封禁管理、社区组管理、在线玩家监控、白名单控制、玩家信息 API 分发、审计日志等功能，并通过 SourceMod 插件与游戏服务器实时联动。

---

## 技术栈

| 模块 | 技术 |
|------|------|
| 前端 | React 18 + Vite + React Router |
| 后端 | Rust (Axum) + SQLx (PostgreSQL) + Tokio |
| 游戏插件 | SourceMod (SourcePawn) + RIPExt + SteamWorks + GlobalAPI |
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
├── csgo/                         # SourceMod 游戏插件
│   ├── addons/sourcemod/
│   │   ├── plugins/              # 编译后的 .smx 插件
│   │   ├── scripting/            # SourcePawn 源码 (.sp)
│   │   │   ├── cngokz-core.sp
│   │   │   ├── cngokz-server.sp
│   │   │   ├── cngokz-sync.sp
│   │   │   ├── cngokz-recordguard.sp
│   │   │   └── cngokz-global.sp
│   │   └── configs/              # 插件依赖
│   └── cfg/sourcemod/            # 插件配置文件
├── .github/workflows/deploy.yml  # CI/CD 自动部署
└── docs/                         # 文档
```

---

## 功能模块

### 核心管理

| 模块 | 说明 |
|------|------|
| **仪表盘** | 服务器状态总览（在线/离线）、核心数据统计、服务器性能指标（FPS、CPU、Tickrate）、白名单统计、管理员预览 |
| **社区组管理** | 社区组 CRUD、服务器 CRUD（含 RCON 连接测试）、在线玩家实时查看、Token 管理、访问限制配置（白名单模式 / Rating / Steam 等级门槛）、RCON 远程命令执行 |
| **白名单管理** | 白名单审核大厅（待审核/已通过/未通过三个标签页）、全球封禁记录检测（KZTimerGlobal API）、手动添加白名单、Steam 名称刷新、全球封禁玩家审核强制填写理由 |
| **封禁管理** | 玩家封禁/解封、封禁类型（Steam/IP/双重）、时长设置（临时/永久）、到期自动解封、封禁公示 |
| **用户管理** | 管理员账户 CRUD、权限组（developer/admin/normal）、密码管理、账号启用/禁用、会话管理 |

### 系统功能

| 模块 | 说明 |
|------|------|
| **玩家信息 API** | Webhook 分发在线玩家数据、自定义 API 端点（公开/密钥访问）、外部服务器数据聚合、地图等级查询 |
| **外部服务器** | 第三方服务器管理、RCON 自动轮询采集玩家数据、服务器状态监控 |
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

| 插件 | 说明 |
|------|------|
| `cngokz-core` | CNGOKZ 公共配置与端口 Token 映射，其他 CNGOKZ 插件优先读取它的配置 |
| `cngokz-server` | 在线玩家、服务器状态、玩家断开原因、封禁与进服权限校验上报 |
| `cngokz-sync` | 离线操作队列与边缘封禁同步 |
| `cngokz-recordguard` | 异常记录检测、录像捕获、R2 上传与审核后提交全球记录 |
| `cngokz-global` | 替代原版 `gokz-global`，保留 GOKZ Global 功能并接入异常记录审核 |

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
             ┌──────────────────────┐
             │  SourceMod 插件        │
             │  (上报/封禁/异常记录)   │
             └──────────────────────┘
```

### 后台定时任务

| 任务 | 周期 | 说明 |
|------|------|------|
| Webhook 分发 | 可配置（默认 30s） | 将在线玩家数据推送到配置的 Webhook URL |
| 过期封禁检查 | 60s | 自动解封到期的封禁记录 |
| Steam 名称刷新 | 6h | 批量更新白名单玩家的 Steam 昵称 |
| Session 清理 | 10min | 清理过期的用户会话 |
| 外部服务器轮询 | 120s（可配置） | RCON 采集外部服务器玩家数据 |
| 过期服务器清理 | 300s | 清理超时未上报的在线玩家并标记服务器为休眠/待上报 |
| 地图等级同步 | 6h | 从 MySQL 同步地图难度等级数据 |
| 限流器清理 | 60s | 清理过期的限流计数器 |

---

## 权限体系

| 角色 | 权限范围 |
|------|---------|
| `developer` | 全部权限：用户管理、封禁管理、白名单管理、社区组管理、RCON 执行、Steam 名称刷新、API 配置 |
| `admin` | 封禁管理、白名单管理（含手动添加）、社区组管理、RCON 执行、API 配置 |
| `normal` | 白名单审核、封禁查看（不可增删改） |

---

## 快速开始

### 环境要求

- Node.js >= 20
- Rust stable（推荐通过 rustup 安装）
- PostgreSQL >= 14
- CS:GO / CS2 服务器 + SourceMod（可选，用于插件联动）

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
- 如果配置任意 `R2_*` 字段，则 `R2_ENDPOINT`、`R2_BUCKET`、`R2_ACCESS_KEY_ID`、`R2_SECRET_ACCESS_KEY` 必须全部配置。
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
| `HTTP_TIMEOUT_SECS` | `300` | 请求超时 |
| `HTTP_CONNECT_TIMEOUT_SECS` | `5` | 连接超时 |
| `REQUEST_TIMEOUT_SECS` | `300` | 全局请求超时 |
| `MAX_REQUEST_BODY_BYTES` | `APPEAL_FILE_MAX_SIZE_MB + 10MB` | 请求体大小限制，需高于申诉文件大小上限 |

### 封禁申诉文件上传

| 变量 | 默认值 | 说明 |
|------|--------|------|
| `APPEAL_FILE_MAX_SIZE_MB` | `100` | 单个申诉辅助文件大小上限 |
| `R2_ENDPOINT` | 空 | Cloudflare R2 S3 API endpoint，例如 `https://<account>.r2.cloudflarestorage.com` |
| `R2_BUCKET` | 空 | R2 存储桶名称 |
| `R2_ACCESS_KEY_ID` | 空 | R2 S3 访问密钥 ID |
| `R2_SECRET_ACCESS_KEY` | 空 | R2 S3 机密访问密钥 |
| `R2_CUSTOM_DOMAIN` | 空 | 文件公开访问域名，可省略 `https://` |
| `R2_TOKEN_VALUE` | 空 | 可选，保留给后续 R2 管理 API 使用 |

未配置完整 R2 信息时，封禁申诉本身仍可提交，只有辅助文件上传不可用。

### 其他

| 变量 | 说明 |
|------|------|
| `MYSQL_DATABASE_URL` | MySQL 连接字符串（用于地图等级同步，可选） |

### 数据库迁移

后端启动时会先执行旧版幂等 Schema 兼容迁移，再执行 `backend/migrations/` 下的 SQLx 正式迁移文件。后续新增或修改数据库结构时，优先添加带时间戳的 SQL 迁移文件，旧版代码迁移只作为兼容已有部署的过渡层保留。

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

## SourceMod 插件部署

### 游戏服务器依赖

安装 `cngokz-*` 插件前，游戏服务器需要先具备以下运行时依赖。缺少依赖时，SourceMod 会出现 `Library not found`、`Native not bound` 或插件无法加载。

| 依赖 | 用途 | 缺失影响 |
|------|------|----------|
| MetaMod:Source | SourceMod 运行基础 | SourceMod 本身无法运行 |
| SourceMod 1.11+ | `.smx` 插件运行环境 | 所有插件无法加载 |
| GOKZ 3.6+ | KZ 核心、模式、计时、录像、玩家状态接口 | `cngokz-global`、`cngokz-recordguard` 无法正常工作 |
| GOKZ Replays | 读取玩家完成地图录像 | 异常记录无法关联玩家录像 |
| GOKZ Local DB / Local Ranks | 原版 GOKZ Global 兼容功能 | 全球封禁、本地排行相关功能不完整 |
| GlobalAPI 2.x | GOKZ.TOP 全球接口请求 | 全球记录、地图、模式、封禁查询失败 |
| RIPExt / REST in Pawn | 插件向 LumiAdmin 后端发送 HTTP/JSON 请求 | `cngokz-server`、`cngokz-sync`、`cngokz-recordguard` 无法访问后端 |
| SteamWorks SourceMod Extension | 游戏服直接上传异常录像到 R2；GlobalAPI 常见环境也依赖它 | 异常录像 R2 上传失败，部分 GlobalAPI 请求可能不可用 |

常见依赖文件位置：

```text
csgo/addons/sourcemod/extensions/ripext.ext.so
csgo/addons/sourcemod/extensions/SteamWorks.ext.so
csgo/addons/sourcemod/plugins/GlobalAPI.smx
csgo/addons/sourcemod/plugins/GlobalAPI-Retrying-Binary.smx
csgo/addons/sourcemod/plugins/GlobalAPI-Logging-Flatfile.smx
csgo/addons/sourcemod/plugins/gokz-core.smx
csgo/addons/sourcemod/plugins/gokz-replays.smx
csgo/addons/sourcemod/plugins/gokz-localdb.smx
csgo/addons/sourcemod/plugins/gokz-localranks.smx
```

在服务器控制台可用以下命令检查依赖：

```text
sm exts list
sm plugins list
```

`sm exts list` 中应能看到 RIPExt 和 SteamWorks；`sm plugins list` 中应能看到 GOKZ、GlobalAPI 和 CNGOKZ 插件。

### 插件包结构

GitHub Actions 可选择发布游戏服务器插件包到 Releases。压缩包按游戏服务器目录组织，解压到游戏服务器根目录后路径应类似：

```text
csgo/
├── addons/sourcemod/plugins/
│   ├── cngokz-core.smx
│   ├── cngokz-server.smx
│   ├── cngokz-sync.smx
│   ├── cngokz-recordguard.smx
│   └── cngokz-global.smx
└── cfg/sourcemod/cngokz-lumiadmin/
```

Release 包不会携带生成后的 `.cfg` 配置文件，避免覆盖服务器已有配置。插件首次加载时会自动生成配置文件；如果服务器上已有同名配置，则 SourceMod 不会重新生成覆盖。

### 本地编译

```bash
bash csgo/build_plugins.sh
```

编译产物位于：

```text
csgo/addons/sourcemod/plugins/
```

### 安装步骤

1. 先安装 MetaMod:Source、SourceMod、GOKZ、GlobalAPI、RIPExt、SteamWorks。
2. 将以下插件放入游戏服务器的 `csgo/addons/sourcemod/plugins/`：

   ```text
   cngokz-core.smx
   cngokz-server.smx
   cngokz-sync.smx
   cngokz-recordguard.smx
   cngokz-global.smx
   ```

3. 将旧插件移动到 `csgo/addons/sourcemod/plugins/disabled/`，避免重复上报或库冲突：

   ```text
   gokz-global.smx
   gokz-r2upload.smx
   manger_online_reporter.smx
   manger_edge_sync.smx
   ```

4. 加载插件或重启服务器。建议加载顺序：

   ```text
   sm plugins load cngokz-core
   sm plugins load cngokz-server
   sm plugins load cngokz-sync
   sm plugins load cngokz-recordguard
   sm plugins load cngokz-global
   ```

### 基础配置

配置文件目录：

```text
csgo/cfg/sourcemod/cngokz-lumiadmin/
```

核心配置文件：

```text
csgo/cfg/sourcemod/cngokz-lumiadmin/cngokz-core.cfg
```

至少需要配置后端插件 API 地址和当前服务器端口对应的 `report_token`：

```text
cngokz_api_base_url "https://你的后端域名/api/plugin"
cngokz_server "27015" "该服务器在网站后台生成的 report_token"
```

如果同一台机器运行多个端口，可以写多行：

```text
cngokz_server "27015" "token_for_27015"
cngokz_server "27016" "token_for_27016"
cngokz_server "27017" "token_for_27017"
```

### 异常录像 R2 上传配置

`cngokz-recordguard` 会优先读取旧 `gokz-r2upload` 配置，方便从旧插件迁移；也可以使用新的 CNGOKZ 配置项。

新配置文件：

```text
csgo/cfg/sourcemod/cngokz-lumiadmin/cngokz-recordguard.cfg
```

关键配置：

```text
cngokz_recordguard_r2upload_enabled "1"
cngokz_recordguard_r2upload_url "https://你的 Cloudflare Worker 上传地址"
cngokz_recordguard_r2upload_key "你的上传 API Key"
cngokz_recordguard_r2upload_verify_cert "0"
```

兼容旧配置：

```text
csgo/cfg/sourcemod/gokz/gokz-r2upload.cfg
```

```text
gokz_r2upload_enabled "1"
gokz_r2upload_url "https://你的 Cloudflare Worker 上传地址"
gokz_r2upload_key "你的上传 API Key"
gokz_r2upload_verify_cert "0"
```

### GOKZ Global 配置

`cngokz-global` 替代原版 `gokz-global`，但继续使用原版 GOKZ Global 的配置文件路径：

```text
csgo/cfg/sourcemod/gokz/gokz-global.cfg
```

如果服务器原来已经能正常运行原版 `gokz-global`，通常保留原配置即可。安装 `cngokz-global` 后，需要禁用原版 `gokz-global.smx`，否则会出现库冲突或重复提交。

### 快速排错

| 现象 | 检查项 |
|------|--------|
| `Library not found: ripext` | RIPExt 是否安装到 `addons/sourcemod/extensions/`，并在 `sm exts list` 中加载 |
| `Library not found: SteamWorks` | SteamWorks extension 是否安装并加载 |
| `Library not found: GlobalAPI` | GlobalAPI 插件是否安装并加载 |
| `Library not found: gokz-global` | `cngokz-global.smx` 是否加载；它会提供兼容的 `gokz-global` library |
| 异常记录有数据但没有录像 | 检查 GOKZ Replays、SteamWorks、R2 上传 URL/Key、`cngokz_recordguard_r2upload_enabled` |
| 后端收不到在线玩家/封禁数据 | 检查 `cngokz_api_base_url`、`cngokz_server "<port>" "<token>"`、服务器端口是否与后台配置一致 |
| 配置文件没有生成 | 检查 `csgo/cfg/sourcemod/cngokz-lumiadmin/` 是否可写 |

---

## 部署

项目使用 GitHub Actions 自动化部署（`.github/workflows/deploy.yml`）：

1. 推送到 `main` 分支自动触发
2. 检测 `frontend/`、`backend/`、`csgo/` 各模块变更
3. 仅构建有变更的模块
4. 通过 SSH + rsync 部署到目标服务器
5. 自动重启后端服务 / 提示重载游戏插件

---

## 安全说明

- 所有管理 API 需要Bearer Token 认证
- 前端 401 全局拦截，Token 过期自动跳转登录
- RCON 命令执行有黑名单保护（阻止 quit、exit、exec 等破坏性命令）
- 插件 API 通过 report_token + port 双重认证
- Webhook 密钥支持常量时间比较，防止时序攻击
- 公开 API 有 IP 级别的速率限制
- 全球封禁玩家审核时强制管理员填写通过/拒绝理由

---

## 许可证

私有项目，未授权禁止使用。

# LumiAdmin

CS:GO / CS2 社区服务器综合管理系统，提供玩家封禁管理、社区组管理、在线玩家监控、白名单控制、玩家访问规则、玩家信息 API 分发、审计日志等功能，并通过 SourceMod 插件与游戏服务器实时联动。

---

## 技术栈

| 模块 | 技术 |
|------|------|
| 前端 | React 18 + Vite + React Router |
| 后端 | Rust (Axum) + SQLx (PostgreSQL) + Tokio |
| 游戏插件 | SourceMod (SourcePawn) + REST in Pawn |
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
│   │   │   ├── playerAccess/     # 玩家访问规则
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
│   │   │   ├── manger_online_reporter.sp   # 在线玩家上报插件
│   │   │   └── manger_edge_sync.sp        # 封禁同步插件
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
| **玩家访问规则** | 基于玩家 SteamID 的访问控制（允许/禁止特定社区组或服务器） |
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
| `manger_online_reporter` | 定时上报在线玩家数据到后端（玩家名、SteamID、IP、Ping、当前地图） |
| `manger_edge_sync` | 封禁记录同步（从后端拉取封禁列表、推送本地封禁到后端） |

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
             │  (在线玩家上报/封禁同步) │
             └──────────────────────┘
```

### 后台定时任务

| 任务 | 周期 | 说明 |
|------|------|------|
| Webhook 分发 | 可配置（默认 30s） | 将在线玩家数据推送到配置的 Webhook URL |
| 过期封禁检查 | 60s | 自动解封到期的封禁记录 |
| Steam 名称刷新 | 6h | 批量更新白名单玩家的 Steam 昵称 |
| Session 清理 | 10min | 清理过期的用户会话 |
| 外部服务器轮询 | 5s（可配置） | RCON 采集外部服务器玩家数据 |
| 过期服务器清理 | 30s | 标记超时未上报的服务器为离线 |
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
| `BIND_ADDR` | `0.0.0.0:3001` | 服务监听地址 |
| `DEV_USERNAME` | `dev` | 初始管理员用户名 |
| `DEV_PASSWORD` | `dev123` | 初始管理员密码（生产环境务必修改） |
| `SESSION_TTL_HOURS` | `24` | 会话有效期（小时） |
| `CORS_ORIGIN` | 允许所有来源 | 前端域名（生产环境务必设置） |

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
| `HTTP_TIMEOUT_SECS` | `30` | 请求超时 |
| `HTTP_CONNECT_TIMEOUT_SECS` | `5` | 连接超时 |
| `REQUEST_TIMEOUT_SECS` | `60` | 全局请求超时 |
| `MAX_REQUEST_BODY_BYTES` | `1048576` | 请求体大小限制（默认 1MB） |

### 其他

| 变量 | 说明 |
|------|------|
| `MYSQL_DATABASE_URL` | MySQL 连接字符串（用于地图等级同步，可选） |

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

### 在线玩家上报插件

1. 将 `manger_online_reporter.smx` 放入服务器的 `addons/sourcemod/plugins/`
2. 编辑 `cfg/sourcemod/manger_online_reporter.cfg`：
   ```
   manger_report_url "http://你的后端地址:3001/api/plugin/online-players/report"
   manger_report_token "服务器的 report_token"
   ```
3. 重载插件或重启服务器

### 封禁同步插件

1. 将 `manger_edge_sync.smx` 放入服务器的 `addons/sourcemod/plugins/`
2. 配置后端地址和 Token
3. 插件会定时从后端拉取封禁列表并同步到本地

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

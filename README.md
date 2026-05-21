# LumiAdmin

CS:GO / CS2 社区服务器综合管理系统，提供玩家封禁管理、社区组管理、在线玩家监控、白名单控制、审计日志等功能，并通过 SourceMod 插件与游戏服务器实时联动。

## 技术栈

| 模块 | 技术 |
|------|------|
| 前端 | React 18 + Vite + React Router + Chart.js |
| 后端 | Rust (Axum) + SQLx (PostgreSQL) + Tokio |
| 游戏插件 | SourceMod (SourcePawn) |
| CI/CD | GitHub Actions |

## 项目结构

```
LumiAdmin/
├── backend/                  # Rust 后端服务
│   ├── src/
│   │   ├── auth/             # 认证 & 权限
│   │   ├── services/         # 业务逻辑
│   │   ├── main.rs
│   │   ├── routes.rs
│   │   └── ...
│   ├── Cargo.toml
│   └── Cargo.lock
├── frontend/                 # React 前端
│   ├── src/
│   │   ├── components/       # 通用组件
│   │   ├── pages/            # 页面（仪表盘、封禁、社区组、白名单等）
│   │   ├── routes/           # 路由配置
│   │   ├── shared/           # 共享 UI 组件
│   │   ├── state/            # 状态管理
│   │   ├── lib/              # 工具函数（API 客户端等）
│   │   └── styles.css
│   ├── index.html
│   ├── vite.config.js
│   └── package.json
├── csgo/                     # SourceMod 游戏插件
│   ├── addons/sourcemod/
│   │   ├── plugins/          # 编译后的 .smx 插件
│   │   └── scripting/        # SourcePawn 源码 (.sp)
│   └── cfg/sourcemod/        # 插件配置文件
├── .github/workflows/        # CI/CD 部署流程
└── .gitignore
```

## 功能模块

- **仪表盘** — 服务器状态总览、数据统计图表
- **封禁管理** — 玩家封禁/解封、封禁公示、到期自动解封
- **社区组管理** — 社区组 CRUD、访问限制（支持白名单模式）
- **白名单** — 玩家白名单管理
- **玩家访问规则** — 基于条件的玩家访问控制
- **在线玩家** — 实时在线玩家数据上报（通过 SourceMod 插件）
- **用户管理** — 管理员账户与权限
- **审计日志** — 操作记录追踪
- **日志查询** — 服务器日志检索

## 快速开始

### 环境要求

- Node.js >= 20
- Rust (stable)
- PostgreSQL
- CS:GO / CS2 服务器 + SourceMod

### 前端

```bash
cd frontend
npm install
npm run dev      # 开发服务器 (默认 http://localhost:5173)
npm run build    # 生产构建 → dist/
```

### 后端

```bash
cd backend
cp ../.env.example ../.env   # 然后编辑 .env 填入数据库等配置
cargo run                     # 开发运行
cargo build --release         # 生产构建 → target/release/manger-backend
```

### 环境变量

参考 `.env.example` 文件，主要配置项：

- 数据库连接地址
- 服务监听端口
- Steam API Key
- 管理员初始账户

### SourceMod 插件

插件源码位于 `csgo/addons/sourcemod/scripting/`，编译后部署到游戏服务器的 `addons/sourcemod/plugins/` 目录。

## 部署

项目使用 GitHub Actions 自动化部署（`.github/workflows/deploy.yml`）：

1. 推送到 `main` 分支自动触发
2. 检测 `frontend/`、`backend/`、`csgo/` 各模块变更
3. 仅构建有变更的模块
4. 通过 SSH + rsync 部署到目标服务器
5. 自动重启后端服务 / 提示重载游戏插件

## 许可证

私有项目，未授权禁止使用。

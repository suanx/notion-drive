# Notion Drive - 云盘服务

> 基于 Rust + Next.js 的高性能云盘服务，支持本地存储、S3、OneDrive 和 WebDAV

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/rust-1.75+-orange.svg)](https://www.rust-lang.org)
[![Next.js](https://img.shields.io/badge/next.js-14-black)](https://nextjs.org)

## 📋 目录

- [特性](#-特性)
- [项目结构](#-项目结构)
- [快速开始](#-快速开始)
- [Docker 部署](#-docker-部署)
- [API 文档](#-api-文档)
- [安全加固](#-安全加固)
- [开发说明](#-开发说明)
- [许可证](#-许可证)

---

## ✨ 特性

| 特性 | 说明 |
|------|------|
| 🚀 **高性能后端** | Rust + Axum 框架，异步非阻塞 I/O |
| 🎨 **现代化前端** | Next.js 14 App Router + TypeScript + Tailwind CSS |
| 📦 **多存储后端** | 本地存储、MinIO/S3、Microsoft OneDrive |
| 🔌 **WebDAV 支持** | 完整 RFC 4918 协议，支持所有主流客户端 |
| 🔐 **安全认证** | JWT + Argon2 密码哈希，分享密码哈希存储 |
| 📁 **文件管理** | 文件夹层级、版本控制、回收站、秒传 |
| 🌐 **分享功能** | 带密码保护的分享链接，速率限制 |
| 📥 **离线下载** | 支持 HTTP/FTP 链接离线下载 |
| 👥 **团队协作** | 用户组、权限管理、文件共享 |

---

## 📁 项目结构

```
notion-drive/
├── Cargo.toml                    # Workspace 配置
├── package.json                  # Root package (concurrently)
├── Makefile                      # 便捷命令
├── .gitignore
├── README.md
│
├── backend/                      # Rust 后端
│   ├── Cargo.toml                # Rust 依赖 (v0.2.0)
│   ├── Dockerfile                # 多阶段构建镜像
│   ├── config/
│   │   └── config.toml           # 应用配置
│   └── src/
│       ├── main.rs               # 应用入口、路由、CORS、健康检查
│       ├── auth.rs               # JWT 认证、注册、登录
│       ├── config.rs             # 配置管理 (含 CORS 白名单)
│       ├── db.rs                 # 数据库连接池
│       ├── file.rs               # 文件 CRUD、上传、搜索
│       ├── share.rs              # 分享链接 (Argon2 密码哈希)
│       ├── storage.rs            # 存储抽象层 (sanitize_filename)
│       ├── storage/
│       │   └── onedrive.rs       # OneDrive 存储驱动
│       ├── offline_download.rs   # 离线下载 (SSRF 防护)
│       ├── onedrive_auth.rs      # OneDrive OAuth2
│       ├── preview.rs            # 文件预览
│       ├── user.rs               # 用户、团队、配额管理
│       └── webdav.rs             # WebDAV 服务器 (使用 config JWT 密钥)
│
├── frontend/                     # Next.js 前端
│   ├── package.json              # Node.js 依赖
│   ├── next.config.js            # Next.js 配置
│   ├── tailwind.config.js        # Tailwind CSS 配置
│   ├── tsconfig.json             # TypeScript 配置
│   ├── app/                      # App Router 页面
│   │   ├── layout.tsx
│   │   ├── page.tsx              # 仪表盘
│   │   ├── login/page.tsx        # 登录页
│   │   └── register/page.tsx     # 注册页
│   ├── components/               # React 组件
│   │   ├── FilePreviewModal.tsx
│   │   └── icons.tsx
│   ├── lib/
│   │   └── api.ts                # API 客户端
│   └── types/
│       └── index.ts              # TypeScript 类型定义
│
├── docker/                       # Docker 配置
│   ├── docker-compose.yml        # 服务编排 (GHCR 镜像)
│   ├── .env.example              # 环境变量模板
│   └── scripts/
│       ├── init-db.sql           # 数据库初始化
│       ├── migrations-phase2.sql
│       ├── migrations-onedrive.sql
│       └── migrations-webdav.sql
│
└── .github/
    └── workflows/
        └── docker-publish.yml    # CI/CD (多架构、Trivy 扫描)
```

---

## 🚀 快速开始

### 环境要求

| 工具 | 版本 |
|------|------|
| Rust | ≥ 1.91 |
| Node.js | ≥ 20 |
| Docker | ≥ 24 |
| PostgreSQL | ≥ 16 |

### 开发环境

```bash
# 方式一：使用 Makefile
make backend-run    # 后端 (http://localhost:8080)
make frontend-run   # 前端 (http://localhost:3000)

# 方式二：手动启动
# 后端
cd backend
cargo run

# 前端（另开终端）
cd frontend
npm install
npm run dev
```

### 生产部署

```bash
# 1. 配置环境变量
cd docker
cp .env.example .env

# ⚠️ 必须修改以下配置：
# JWT_SECRET=<强随机密钥，至少 32 字符>
# NOTION_SERVER_BASE_URL=https://yourdomain.com
# NOTION_CORS_ALLOWED_ORIGINS=https://yourdomain.com

# 2. 启动服务
docker-compose up -d

# 3. 查看日志
docker-compose logs -f notion-drive
```

---

## 🐳 Docker 部署

### 镜像来源

本项目使用 **GitHub Container Registry (GHCR)** 提供预构建镜像：

```
ghcr.io/${owner}/notion-drive:latest
ghcr.io/${owner}/notion-drive:v0.2.0
```

### 多架构支持

| 架构 | 支持 |
|------|------|
| linux/amd64 | ✅ |
| linux/arm64 | ✅ |

### docker-compose.yml 配置

```yaml
version: '3.8'

services:
  postgres:
    image: postgres:16-alpine
    environment:
      POSTGRES_USER: ${DB_USER}
      POSTGRES_PASSWORD: ${DB_PASSWORD}
      POSTGRES_DB: ${DB_NAME}
    volumes:
      - postgres_data:/var/lib/postgresql/data
      - ./docker/scripts/init-db.sql:/docker-entrypoint-initdb.d/init-db.sql
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U ${DB_USER} -d ${DB_NAME}"]
      interval: 10s
      retries: 5

  notion-drive:
    image: ghcr.io/${owner}/notion-drive:latest
    environment:
      DATABASE_URL: postgres://${DB_USER}:${DB_PASSWORD}@postgres:5432/${DB_NAME}
      JWT_SECRET: ${JWT_SECRET}           # ⚠️ 必须设置
      NOTION__SERVER__BASE_URL: ${NOTION_SERVER_BASE_URL}
      NOTION__CORS__ALLOWED_ORIGINS: ${NOTION_CORS_ALLOWED_ORIGINS}
    volumes:
      - backend_storage:/app/storage
    depends_on:
      postgres:
        condition: service_healthy

volumes:
  postgres_data:
  backend_storage:
```

### 环境变量说明

| 变量 | 默认值 | 说明 |
|------|--------|------|
| `DB_USER` | notion_drive | 数据库用户名 |
| `DB_PASSWORD` | notion_drive_secret | 数据库密码 |
| `DB_NAME` | notion_drive | 数据库名 |
| `JWT_SECRET` | - | ⚠️ **必须设置**，强随机密钥 |
| `NOTION_SERVER_BASE_URL` | http://localhost:8080 | 服务器基础 URL |
| `NOTION_CORS_ALLOWED_ORIGINS` | http://localhost:3000,http://localhost:8080 | CORS 白名单 |
| `RUST_LOG` | info | 日志级别 |

---

## 🔌 API 文档

### 认证

| 方法 | 路径 | 描述 |
|------|------|------|
| POST | `/api/v1/auth/register` | 用户注册 |
| POST | `/api/v1/auth/login` | 用户登录 |
| POST | `/api/v1/auth/refresh` | 刷新令牌 |
| GET | `/api/v1/auth/me` | 当前用户信息 |

### 文件管理

| 方法 | 路径 | 描述 |
|------|------|------|
| GET | `/api/v1/files` | 列出文件 |
| POST | `/api/v1/files` | 创建文件夹 |
| POST | `/api/v1/files/upload/simple` | 上传文件（秒传） |
| POST | `/api/v1/files/upload/session` | 创建上传会话 |
| PUT | `/api/v1/files/upload/chunk` | 上传分块 |
| GET | `/api/v1/files/:file_id` | 获取文件信息 |
| DELETE | `/api/v1/files/:file_id` | 删除文件 |
| GET | `/api/v1/files/search?q=xxx` | 搜索文件 |

### 分享管理

| 方法 | 路径 | 描述 |
|------|------|------|
| GET | `/api/v1/shares` | 列出分享链接 |
| POST | `/api/v1/shares` | 创建分享链接 |
| DELETE | `/api/v1/shares/:share_id` | 删除分享链接 |
| GET | `/api/v1/shares/public/:token` | 公开分享信息 |
| GET | `/api/v1/shares/public/:token/download` | 公开下载 |

### WebDAV

```
# 使用 WebDAV 客户端连接
URL: http://localhost:8080/webdav
认证: Bearer <JWT 令牌> 或 Basic Auth
```

### 健康检查

```bash
curl http://localhost:8080/health
```

响应示例：
```json
{
  "status": "ok",
  "database": "healthy",
  "storage": "healthy",
  "timestamp": 1716134400
}
```

---

## 🔒 安全加固

### v0.2.0 - 全面安全审计修复

| 类别 | 问题 | 修复措施 |
|------|------|----------|
| 🔴 **CRITICAL** | WebDAV 硬编码密钥 `"secret"` | 使用 `state.config.jwt.secret`，移除回退机制 |
| 🔴 **CRITICAL** | CORS 允许任意来源 | 配置白名单 `allowed_origins` |
| 🔴 **CRITICAL** | 分享密码明文存储 | Argon2 密码哈希存储 |
| 🟠 **HIGH** | 文件名路径遍历 | `sanitize_filename()` 函数防护 |
| 🟠 **HIGH** | 分享链接无速率限制 | `governor` 限流器 (10 次/分钟) |
| 🟠 **HIGH** | SSRF - URL 验证不足 | 完整 URL 解析 + 私有 IP 过滤 |
| 🟡 **MEDIUM** | 分享 URL 硬编码 localhost | 从 `config.server.base_url` 读取 |
| 🟡 **MEDIUM** | 缺少请求体大小限制 | `BodyLimitLayer` 限制 100MB |
| 🟢 **LOW** | 健康检查过于简单 | 返回数据库、存储详细状态 |

### 生产环境安全检查清单

- [ ] `JWT_SECRET` 设置为至少 32 字符的强随机密钥
- [ ] `NOTION_CORS_ALLOWED_ORIGINS` 配置为实际域名
- [ ] `NOTION_SERVER_BASE_URL` 配置为 HTTPS URL
- [ ] 数据库密码已修改（默认 `notion_drive_secret`）
- [ ] 已禁用默认管理员账户或使用强密码
- [ ] 已启用防火墙限制端口访问
- [ ] 已配置 HTTPS 反向代理（Nginx/Caddy）

---

## 💻 开发说明

### Makefile 命令

```bash
make all           # 构建全部
make build         # 构建后端和前端
make backend-build # 构建后端
make frontend-build# 构建前端
make backend-run   # 运行后端（开发）
make frontend-run  # 运行前端（开发）
make docker-up     # 启动 Docker
make docker-down   # 停止 Docker
make test          # 运行测试
make clean         # 清理构建产物
make fmt           # 格式化代码
make lint          # 代码检查
```

### 项目配置

#### backend/config/config.toml

```toml
[database]
url = "postgres://..."
pool_size = 10

[jwt]
secret = "your_jwt_secret_key_change_in_production"
expiration_hours = 24

[storage]
default_policy_id = "00000000-0000-0000-0000-000000000001"
local_path = "./storage"

[server]
host = "0.0.0.0"
port = 8080
base_url = "http://localhost:8080"

[cors]
allowed_origins = ["http://localhost:3000", "http://localhost:8080"]
allowed_methods = ["GET", "POST", "PUT", "DELETE", "OPTIONS"]
allowed_headers = ["Authorization", "Content-Type", "X-Requested-With"]
```

### 依赖说明

#### Rust (backend/Cargo.toml)

| 依赖 | 版本 | 用途 |
|------|------|------|
| axum | 0.7 | Web 框架 |
| sqlx | 0.7 | 异步数据库 |
| jsonwebtoken | 9 | JWT 认证 |
| argon2 | 0.5 | 密码哈希 |
| governor | 0.6 | 速率限制 |
| url | 2.5 | URL 解析（SSRF 防护） |
| aws-sdk-s3 | 1 | S3 存储 |

#### Node.js (frontend/package.json)

| 依赖 | 版本 | 用途 |
|------|------|------|
| next | 14 | React 框架 |
| react | 18 | UI 库 |
| tailwindcss | 3.4 | CSS 框架 |
| lucide-react | latest | 图标库 |

---

## 📝 变更日志

### [0.2.0] - 2024-05-20

**安全加固**
- 移除 WebDAV 硬编码密钥
- CORS 白名单配置
- 分享密码 Argon2 哈希存储
- 文件名路径遍历防护
- 分享链接速率限制
- SSRF URL 验证
- 请求体大小限制（100MB）
- 详细健康检查端点

**项目结构**
- 添加 Workspace Cargo.toml
- 添加根 package.json（concurrently）
- 添加 Makefile
- 更新 docker-compose.yml（GHCR 镜像）

**依赖更新**
- 添加 `url` crate（SSRF 防护）
- 更新 `governor` 速率限制

### [0.1.0] - 2024-05-15

- 初始版本发布
- 基础文件管理功能
- WebDAV 支持
- OneDrive 集成
- 分享链接功能

---

## 📄 许可证

MIT License - 详见 [LICENSE](LICENSE) 文件

---

## 🤝 贡献

欢迎提交 Issue 和 Pull Request！

1. Fork 本仓库
2. 创建特性分支 (`git checkout -b feature/AmazingFeature`)
3. 提交更改 (`git commit -m 'Add some AmazingFeature'`)
4. 推送到分支 (`git push origin feature/AmazingFeature`)
5. 开启 Pull Request

---

## 📧 联系方式

- 项目主页: https://github.com/${owner}/notion-drive
- 问题反馈: https://github.com/${owner}/notion-drive/issues
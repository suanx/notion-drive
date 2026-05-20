# 多阶段构建 - Rust 后端 + Next.js 前端
# ==================== 阶段 1: 构建前端 ====================
FROM node:20-alpine AS frontend-builder

WORKDIR /app

# 复制 package 文件（优先使用 package-lock.json 确保依赖一致性）
COPY frontend/package.json frontend/package-lock.json ./
RUN npm ci

# 复制前端源代码
COPY frontend/. .

# 构建前端（生产模式）
ENV NEXT_PUBLIC_API_URL=/api
RUN npm run build

# ==================== 阶段 2: 构建后端 ====================
FROM rust:1.75-alpine AS backend-builder

WORKDIR /app

# 安装构建依赖（包括 PostgreSQL 客户端库、OpenSSL 和 pkg-config）
RUN apk add --no-cache musl-dev pkgconfig openssl-dev postgresql-dev

# 复制 Cargo 配置文件
COPY backend/Cargo.toml ./

# 复制源代码
COPY backend/src ./src

# 生成 Cargo.lock 并下载依赖（合并步骤，避免缓存失效）
# 注意：cargo 会自动生成 Cargo.lock 如果不存在
RUN cargo build --release

# 编译产物已在 target/release/notion-drive

# ==================== 阶段 3: 运行 ====================
FROM alpine:3.19

WORKDIR /app

# 安装运行时依赖
RUN apk add --no-cache ca-certificates

# 从构建阶段复制二进制文件
COPY --from=backend-builder /app/target/release/notion-drive /app/notion-drive

# 从构建阶段复制前端构建产物
COPY --from=frontend-builder /app/out /app/frontend

# 复制配置文件
COPY backend/config /app/config

# 创建存储目录
RUN mkdir -p /app/storage

# 暴露端口
EXPOSE 8080

# 运行
CMD ["./notion-drive"]

# 多阶段构建 - Rust 后端 + Next.js 前端
# ==================== 阶段 1: 构建前端 ====================
FROM node:20-alpine AS frontend-builder

WORKDIR /app

# 安装依赖
COPY frontend/package.json frontend/package-lock.json* ./
RUN npm ci

# 复制前端源代码
COPY frontend/. .

# 构建前端（生产模式）
ENV NEXT_PUBLIC_API_URL=/api
RUN npm run build

# ==================== 阶段 2: 构建后端 ====================
FROM rust:1.75-alpine AS backend-builder

WORKDIR /app

# 安装构建依赖
RUN apk add --no-cache musl-dev pkgconfig openssl-dev

# 复制 Cargo 配置文件
COPY backend/Cargo.toml backend/Cargo.lock ./

# 创建虚拟的空项目以利用 Docker 缓存
RUN cargo build --release && rm -rf src target/release/*

# 复制源代码
COPY backend/src ./src

# 编译
RUN cargo build --release

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

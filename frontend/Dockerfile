# 生产环境 Dockerfile
FROM node:20-alpine AS builder

WORKDIR /app

# 安装依赖
COPY package.json package-lock.json* ./
RUN npm ci

# 复制源代码
COPY . .

# 构建
RUN npm run build

# 第二阶段：生产环境
FROM node:20-alpine

WORKDIR /app

# 安装生产依赖
COPY package.json package-lock.json* ./
RUN npm ci --only=production

# 从构建阶段复制构建产物
COPY --from=builder /app/.next ./.next
COPY --from=builder /app/public ./public
COPY --from=builder /app/next.config.js ./next.config.js
COPY --from=builder /app/tailwind.config.js ./tailwind.config.js
COPY --from=builder /app/postcss.config.js ./postcss.config.js

# 暴露端口
EXPOSE 3000

# 启动生产服务器
CMD ["npm", "start"]

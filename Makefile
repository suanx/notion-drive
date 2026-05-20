.PHONY: all build run test clean docker-up docker-down

# 默认目标
all: build

# 构建后端
backend-build:
	@echo "🔨 Building backend..."
	cd backend && cargo build --release

# 构建前端
frontend-build:
	@echo "🔨 Building frontend..."
	cd frontend && npm install && npm run build

# 构建全部
build: backend-build frontend-build
	@echo "✅ Build complete"

# 运行后端（开发模式）
backend-run:
	@echo "🚀 Running backend..."
	cd backend && cargo run

# 运行前端（开发模式）
frontend-run:
	@echo "🚀 Running frontend..."
	cd frontend && npm run dev

# 运行全部（Docker）
docker-up:
	@echo "🐳 Starting Docker containers..."
	cd docker && docker-compose up -d

# 停止 Docker
docker-down:
	@echo "🛑 Stopping Docker containers..."
	cd docker && docker-compose down

# 运行测试
test:
	@echo "🧪 Running tests..."
	cd backend && cargo test

# 清理
clean:
	@echo "🧹 Cleaning..."
	cd backend && cargo clean
	rm -rf frontend/node_modules frontend/.next frontend/out
	find . -type d -name "__pycache__" -exec rm -rf {} + 2>/dev/null || true
	find . -type f -name "*.pyc" -delete 2>/dev/null || true

# 格式化代码
fmt:
	cd backend && cargo fmt
	cd frontend && npm run format

# 代码检查
lint:
	cd backend && cargo clippy -- -D warnings
	cd frontend && npm run lint

# 帮助信息
help:
	@echo "Notion Drive - Makefile Commands"
	@echo ""
	@echo "  make build          - Build backend and frontend"
	@echo "  make backend-build  - Build Rust backend only"
	@echo "  make frontend-build - Build Next.js frontend only"
	@echo "  make backend-run    - Run backend in dev mode"
	@echo "  make frontend-run   - Run frontend in dev mode"
	@echo "  make docker-up      - Start Docker containers"
	@echo "  make docker-down    - Stop Docker containers"
	@echo "  make test           - Run tests"
	@echo "  make clean          - Clean build artifacts"
	@echo "  make fmt            - Format code"
	@echo "  make lint           - Run linter"
	@echo "  make help           - Show this help"
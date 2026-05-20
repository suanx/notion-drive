# Cargo.lock 说明

## 为什么 Cargo.lock 未提交？

本项目采用 **Docker 构建时动态生成** `Cargo.lock` 的策略，原因如下：

1. **简化维护**：无需在每次依赖变更时更新 lock 文件
2. **减少冲突**：避免多人协作时的 lock 文件冲突
3. **灵活更新**：构建时自动获取最新兼容版本

## 构建流程

```dockerfile
# Dockerfile 阶段 2
COPY backend/Cargo.toml ./
COPY backend/src ./src
RUN cargo generate-lockfile && cargo fetch
RUN cargo build --release
```

## 本地开发

如需本地构建或生成完整的 `Cargo.lock`：

```bash
cd backend
cargo generate-lockfile    # 生成 Cargo.lock
cargo build                # 本地构建
```

## 提交 Cargo.lock（可选）

如需确保依赖版本完全一致，可提交 `Cargo.lock`：

```bash
cd backend
cargo generate-lockfile
git add Cargo.lock
git commit -m "chore: add Cargo.lock for reproducible builds"
```

然后更新 `.gitignore` 取消注释 `Cargo.lock` 行。

## 依赖更新

```bash
cd backend
cargo update               # 更新所有依赖
cargo update -p <package>  # 更新特定依赖
```
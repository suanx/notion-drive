# GitHub Actions 配置说明

## 触发条件修复

### 问题诊断

| 问题 | 原因 | 修复 |
|------|------|------|
| ❌ 工作流不触发 | 触发分支配置为 `main`，实际分支为 `master` | ✅ 改为 `master` |
| ❌ 手动触发无选项 | 缺少 `workflow_dispatch` | ✅ 添加手动触发 |
| ❌ Lint 失败导致构建中断 | `needs.lint.result == 'failure'` 未处理 | ✅ 添加 failure 条件 |
| ❌ Manifest 步骤冗余 | `docker manifest create` 与 build-push-action 冲突 | ✅ 移除手动 manifest |
| ❌ generate-compose push 失败 | 分支名称硬编码 | ✅ 动态检测 master/main |

### 当前触发配置

```yaml
on:
  push:
    branches:
      - master          # ✅ 修复：匹配实际分支
    tags:
      - 'v*'
  pull_request:
    branches:
      - master          # ✅ 修复：匹配实际分支
  workflow_dispatch:    # ✅ 新增：支持手动触发
    inputs:
      platform:
        description: 'Build platform'
        required: false
        default: 'all'
        type: choice
        options:
          - all
          - linux/amd64
          - linux/arm64
```

---

## 仓库权限设置

### 必须配置项

前往：`https://github.com/suanx/notion-drive/settings/actions`

#### 1. Actions 权限

```
✅ Allow all actions and reusable workflows
   (或选择 "Allow actions created by GitHub" + "Allow actions from verified creators")
```

#### 2. 仓库权限（重要）

前往：`https://github.com/suanx/notion-drive/settings/actions`

```
✅ Read and write permissions
   (允许 GITHUB_TOKEN 推送镜像到 GHCR)
```

#### 3. 分支保护规则（可选）

前往：`https://github.com/suanx/notion-drive/settings/branches`

```
Branch: master
✅ Require status checks to pass before merging
   → 添加 "Lint" 和 "Build (linux/amd64)" 检查
```

---

## GHCR 镜像权限

### 自动推送权限

`GITHUB_TOKEN` 默认具有 `packages: write` 权限，可推送镜像到：

```
ghcr.io/suanx/notion-drive
```

### 验证权限

在 Actions 工作流中：

```yaml
permissions:
  contents: read
  packages: write    # ✅ 必需：推送镜像到 GHCR
```

---

## 手动触发部署

### 方式一：GitHub Web UI

1. 前往：`https://github.com/suanx/notion-drive/actions`
2. 选择 "Docker Build & Publish"
3. 点击 "Run workflow"
4. 选择平台（all/amd64/arm64）
5. 点击 "Run workflow"

### 方式二：GitHub CLI

```bash
gh workflow run docker-publish.yml \
  -f platform=all \
  -R suanx/notion-drive
```

### 方式三：API

```bash
curl -X POST \
  -H "Authorization: token $GITHUB_TOKEN" \
  -H "Accept: application/vnd.github.v3+json" \
  https://api.github.com/repos/suanx/notion-drive/actions/workflows/docker-publish.yml/dispatches \
  -d '{"ref":"master"}'
```

---

## 工作流执行流程

```
触发事件 (push/PR/dispatch)
    │
    ├── PR → lint (Trivy 扫描)
    │       └── 仅扫描，不构建
    │
    └── push/dispatch → build (多架构)
            │
            ├── linux/amd64 → 构建 + 推送
            ├── linux/arm64 → 构建 + 推送
            └── Trivy 镜像扫描
            │
            └── manifest (自动，build-push-action 处理)
            │
            ├── generate-compose (main/tag)
            │   └── 生成 docker-compose.prod.yml
            │   └── 提交到仓库
            │
            ├── cleanup (main)
            │   └── 删除旧版本镜像
            │
            └── release (tag)
                └── 创建 GitHub Release
```

---

## 常见问题排查

### Q1: 工作流显示 "This workflow is skipped"

**原因**：触发条件不匹配

**解决**：
```bash
# 检查分支名称
git branch --show-current

# 确保推送到的分支在触发条件中
# 修改 .github/workflows/docker-publish.yml 的 on.push.branches
```

### Q2: 构建失败 "permission denied"

**原因**：GITHUB_TOKEN 权限不足

**解决**：
1. 前往仓库 Settings → Actions → General
2. 确保 "Workflow permissions" 设置为 "Read and write permissions"

### Q3: GHCR 推送失败 "unauthorized"

**原因**：`docker/login-action` 配置错误

**解决**：确保使用 `secrets.GITHUB_TOKEN`：
```yaml
- uses: docker/login-action@v3
  with:
    registry: ghcr.io
    username: ${{ github.actor }}
    password: ${{ secrets.GITHUB_TOKEN }}
```

### Q4: generate-compose 无法推送

**原因**：GITHUB_TOKEN 无写权限或分支保护

**解决**：
1. 检查仓库权限设置
2. 或临时禁用分支保护规则
3. 或使用 Personal Access Token（需配置 secret）

---

## 验证清单

部署前请确认：

- [ ] 分支名称匹配（master vs main）
- [ ] Actions 权限已启用
- [ ] Workflow permissions 为 "Read and write"
- [ ] GITHUB_TOKEN 可访问 packages
- [ ] 无分支保护规则阻止 workflow
- [ ] 工作流文件语法正确（已修复）

---

## 快速测试

```bash
# 1. 手动触发测试
cd notion-drive
git push origin master

# 2. 观察 Actions
# https://github.com/suanx/notion-drive/actions

# 3. 验证镜像
docker pull ghcr.io/suanx/notion-drive:latest

# 4. 验证多架构
docker manifest inspect ghcr.io/suanx/notion-drive:latest
```

---

## 相关文档

- [GitHub Actions 权限](https://docs.github.com/en/actions/security-guides/automatic-token-authentication)
- [GHCR 文档](https://docs.github.com/en/packages/working-with-a-github-packages-registry/working-with-the-container-registry)
- [workflow_dispatch](https://docs.github.com/en/actions/using-workflows/events-that-trigger-workflows#workflow_dispatch)
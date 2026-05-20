#!/bin/bash
# 生成 Cargo.lock 文件
# 运行此脚本前确保已安装 Rust (cargo)

set -e

echo "🔨 生成 Cargo.lock..."

cd "$(dirname "$0")/backend"

# 如果 Cargo.lock 不存在，生成它
if [ ! -f Cargo.lock ]; then
    echo "   Cargo.lock 不存在，正在生成..."
    cargo generate-lockfile
    echo "✅ Cargo.lock 已生成"
else
    echo "   Cargo.lock 已存在，跳过生成"
fi

# 验证
if [ -f Cargo.lock ]; then
    echo "✅ Cargo.lock 验证通过"
    echo "   文件大小: $(wc -c < Cargo.lock) bytes"
    echo "   依赖数量: $(grep -c '^\[\[package\]\]' Cargo.lock) 个"
else
    echo "❌ Cargo.lock 生成失败"
    exit 1
fi

echo ""
echo "💡 提示: 请将生成的 Cargo.lock 提交到仓库:"
echo "   git add backend/Cargo.lock"
echo "   git commit -m 'chore: add Cargo.lock for reproducible builds'"
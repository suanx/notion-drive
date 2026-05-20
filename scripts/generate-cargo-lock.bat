@echo off
REM 生成 Cargo.lock 文件
REM 运行此脚本前确保已安装 Rust (cargo)

echo 🔨 生成 Cargo.lock...

cd backend

REM 如果 Cargo.lock 不存在，生成它
if not exist Cargo.lock (
    echo    Cargo.lock 不存在，正在生成...
    cargo generate-lockfile
    echo ✅ Cargo.lock 已生成
) else (
    echo    Cargo.lock 已存在，跳过生成
)

REM 验证
if exist Cargo.lock (
    echo ✅ Cargo.lock 验证通过
    for %%A in (Cargo.lock) do echo    文件大小: %%~zA bytes
) else (
    echo ❌ Cargo.lock 生成失败
    exit /b 1
)

echo.
echo 💡 提示: 请将生成的 Cargo.lock 提交到仓库:
echo    git add backend/Cargo.lock
echo    git commit -m "chore: add Cargo.lock for reproducible builds"
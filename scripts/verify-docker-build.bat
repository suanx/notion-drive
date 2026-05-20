@echo off
REM Docker 构建验证脚本
REM 测试前后端 Docker 构建是否正常工作

echo ==========================================
echo 🐳 Notion Drive Docker 构建验证
echo ==========================================
echo.

REM 检查必要文件
echo 📋 检查必要文件...
set FILES=backend\Cargo.toml backend\Cargo.lock backend\Dockerfile frontend\package.json frontend\package-lock.json docker\docker-compose.yml

for %%F in (%FILES%) do (
    if exist "%%F" (
        echo   ✅ %%F
    ) else (
        echo   ❌ %%F (缺失)
        exit /b 1
    )
)

echo.
echo 🔨 开始构建...
echo.

REM 构建 Docker 镜像
cd docker
docker-compose build

echo.
echo ✅ 构建成功！
echo.
echo 📦 镜像列表:
docker images | findstr notion-drive

echo.
echo 🚀 启动服务:
echo    docker-compose up -d
echo.
echo 📊 查看日志:
echo    docker-compose logs -f notion-drive
echo.
echo 🛑 停止服务:
echo    docker-compose down
echo.
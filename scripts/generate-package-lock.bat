@echo off
REM 生成 package-lock.json
REM 运行此脚本前确保已安装 Node.js (npm)

echo 🔨 Generating package-lock.json...

cd frontend

if not exist package-lock.json (
    echo    package-lock.json not found, generating...
    npm install --package-lock-only
    echo ✅ package-lock.json generated
) else (
    echo    package-lock.json exists, skipping
)

if exist package-lock.json (
    echo ✅ package-lock.json verified
    for %%A in (package-lock.json) do echo    File size: %%~zA bytes
) else (
    echo ❌ package-lock.json generation failed
    exit /b 1
)

echo.
echo 💡 Tip: Commit the generated package-lock.json:
echo    git add frontend/package-lock.json
echo    git commit -m "chore: add package-lock.json for reproducible builds"
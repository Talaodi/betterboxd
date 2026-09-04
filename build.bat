@echo off
rem Windows 构建入口：build.bat [--release]（双击或命令行运行）
cd /d "%~dp0"
node build.mjs %*
if errorlevel 1 (
  echo.
  echo 构建失败：请确认 Node.js 已安装且版本 >= 18，并安装 Rust（https://rustup.rs）。
  echo Windows 下 rusqlite 需要 C 编译器：安装 VS Build Tools（C++ 工作负载）或 MinGW。
  pause
  exit /b 1
)
echo.
echo 构建完成。
pause

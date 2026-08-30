#!/usr/bin/env bash
# Betterboxd 开发/运行脚本（在 betterboxd/ 目录下执行）
# 用法:
#   ./dev.sh build   构建前端+后端
#   ./dev.sh run     前台启动服务器 (Ctrl-C 停止)
#   ./dev.sh webdev  前端热更新模式 (需另开一个终端先 run)
set -e
cd "$(dirname "$0")"

build() {
  (cd apps/web && npm install && npm run build)
  cargo build -p betterboxd-server
}

run() {
  # 注意：静态目录按相对路径解析，必须在 betterboxd/ 目录下启动
  exec cargo run -p betterboxd-server
}

webdev() {
  cd apps/web && npm run dev
}

case "${1:-run}" in
  build) build ;;
  run) run ;;
  webdev) webdev ;;
  *) echo "用法: ./dev.sh build|run|webdev" ;;
esac

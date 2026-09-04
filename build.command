#!/usr/bin/env bash
# macOS 双击/终端构建入口：./build.command [--release]（等于 build.sh，独立文件便于 Finder 双击）
set -e
cd "$(dirname "$0")"
exec node build.mjs "$@"

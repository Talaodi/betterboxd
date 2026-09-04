#!/usr/bin/env bash
# Linux / macOS(WSL) 构建入口：./build.sh [--release]
set -e
cd "$(dirname "$0")"
node build.mjs "$@"

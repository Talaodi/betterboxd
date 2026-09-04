# Betterboxd - AI 影迷综合助手

本项目作为清华大学程序设计训练 (Rust) 课程的大作业开发, 目前处于 Demo 阶段, 开发过程使用 Opencode + GLM / Deepseek 辅助.

Betterboxd 是专为影迷开发的综合助手, 本身不仅是一个完善且成熟的记录工具, 还结合 AI 综合优化使用体验.

本项目使用 TMDB (The Movie Database) 作为数据库, 其规定 API Key 不得公开传播. 为了能够正常使用本项目请自行注册 API Key (免费快捷, 过程约 5 分钟). 注册方式见 [GENERATE_TMDB_API_KEY.md](./docs/GENERATE_TMDB_API_KEY.md).

## 构建

构建流程目前仅保证在 Linux 系统 (包括 WSL2) 下适用, 若使用其他系统出现问题请反馈.

编译:

```bash
cd apps/web
npm install
npm run build              # 产物 apps/web/dist
cd ../..
cargo build                # 产物 target/debug/betterboxd-server
```

运行:

```bash
./target/debug/betterboxd-server        # 或 cargo run -p betterboxd-server
```

浏览器打开 http://localhost:3000.

## 快速开始

项目提供了 ./example 作为样例数据展示各项功能, 可自行载入查看或前往 [QUICK_START.md](./docs/QUICK_START.md) 查看各项功能.
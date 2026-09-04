# Betterboxd - AI 影迷综合助手

本项目作为清华大学程序设计训练 (Rust) 课程的大作业开发, 目前处于 Demo 阶段, 开发过程使用 Opencode + GLM / Deepseek 辅助.

Betterboxd 是专为影迷开发的综合助手, 本身不仅是一个完善且成熟的记录工具, 还结合 AI 综合优化使用体验.

本项目使用 TMDB (The Movie Database) 作为数据库, 其规定 API Key 不得公开传播. 为了能够正常使用本项目请自行注册 API Key (免费快捷, 过程约 5 分钟). 注册方式见 [GENERATE_TMDB_API_KEY.md](./docs/GENERATE_TMDB_API_KEY.md).

## 为什么选择 Betterboxd

- **记录工具本身已是完整形态**: Diary 与 Review 双线索俱全 (日期/评分/影院/票价/维度标注/自由标签/Markdown 随记/重看次数/署名日期), List 支持排名拖拽; 从记录到回顾, 一条链路不出岔子.

- **"一切数据皆可工具查询"的设计理念**: 观影记录, 影评, 想看, 喜欢, 清单及其成员, 统计项目, 历史对话, 维度值池 — 全部暴露在 AI 工具面上. 每一个回答都回到数据源头, 没有死角, 也没有神秘的"AI 感觉".

- **AI 会查, 而不是猜**: 只要涉及你的数据库, AI 一律走工具拿数; 影片事实 (年份/导演/剧情) 先查 TMDB 再开口. 你的偏好画像随上下文注入, 回答天然贴着你.

- **统计: 自由度没有上限, 准确度不让幻觉**: 不预置任何统计项目, AI 在只读 SQL 视图上现场写查询 — 计数, 均值, 排序, 窗口任你发问; 涉及风格/情感/主题这类主观文本分析, 则逐条取样阅读后总结, 该写代码的写代码, 该读原文的读原文. 对了, 查询权限是开放给你的, 但它只读你愿意给它看的部分.

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

项目提供了 ./example 作为样例数据展示各项功能, 可自行载入存档.

前往 [QUICK_START.md](./docs/QUICK_START.md) 查看各项功能.
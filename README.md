# Betterboxd - AI 影迷综合助手

本项目作为清华大学程序设计训练 (Rust) 课程的大作业开发, 目前处于 Demo 阶段, 开发过程使用 Opencode + GLM / Deepseek 辅助.

Betterboxd 是专为影迷开发的综合助手, 本身不仅是一个完善且成熟的记录工具, 还使用 AI 综合优化使用体验.

本项目使用 TMDB (The Movie Database) 作为数据库, 其规定 API Key 不得公开传播. 为了能够正常使用本项目请自行注册 API Key (免费快捷, 过程约 5 分钟). 注册方式见 [GENERATE_TMDB_API_KEY.md](./docs/GENERATE_TMDB_API_KEY.md).

## 为什么选择 Betterboxd

- **本体即为完善的观影记录工具**. Diary / Review / List 功能齐全, 连同票价的记录, 标注维度, 自由标签, 署名日期, 拖拽排名, 该有的都有.

- **设计理念: 底层逻辑工具化**. 所有底层数据的读写不仅可以在 UI 界面上操作, 还都做成了工具供 AI 调用: 观影记录, 影评, 想看与喜欢, 清单及成员, 统计项目, 对话历史, 值池标签, 每种数据都有对应查询工具; 而 "记一笔", "改记录", "调整清单", "存个统计项目" 这类写操作同样有对应的工具, 用户不仅可以手动添加修改, 还可以在与 AI 对话时 AI 自动调用工具进行更改, 使用更丝滑. 这样一来, AI 所答有据可查, 所行真实执行.

    - 这样做的一大好处: 即使没有写任何外部数据导入的功能, 也可以直接把记录的数据 (不管是豆瓣 / Letterboxd 还是自己的 Excel) 发给 AI 让他智能识别批量导入. 不管数据以何种方式记录, 都可直接交给 AI 解析, 十分灵活.

- **AI 因此更懂你**. 对话自动注入你的影迷画像, 涉及你的数据一律先查再答, 影片事实先查 TMDB 再开口; 你表达的意图, 会被它真正落进数据库, 而不是答一句就完.

- **统计自由且准确**. 一切客观数据均可统计, AI 通过写 SQL 代码的形式查询, 避免幻觉. 统计项目可保存, 0 成本重跑.

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

浏览器打开 http://localhost:3000 (若在 WSL2 内构建也可选择使用 Windows 的浏览器打开).

## 快速开始

项目提供了 ./example 作为样例数据展示各项功能, 可自行载入存档. 该存档已经删除各项 API Key 的配置, 记得手动去配置一下.

前往 [QUICK_START.md](./docs/QUICK_START.md) 查看各项功能.
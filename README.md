# Betterboxd - AI 影迷综合助手

本项目作为清华大学程序设计训练 (Rust) 课程的大作业开发, 目前处于 Demo 阶段, 开发过程使用 Opencode + GLM / Deepseek 辅助.

Betterboxd 是专为影迷开发的综合助手, 本身不仅是一个完善且成熟的记录工具, 还结合 AI 综合优化使用体验.

本项目使用 TMDB (The Movie Database) 作为数据库, 由其 API 使用规范, API Key 不得公开传播. 为了能够正常使用本项目请自行注册 API Key (免费快捷, 过程约 5 分钟). [注册方式](./docs/About-tmdb-api.md).

## 构建

```bash
# 1) 前端
cd apps/web
npm install
npm run build              # 产物 apps/web/dist
cd ../..

# 2) Rust（workspace 根目录）
cargo build                # 产物 target/debug/betterboxd-server
```

> 前端只需构建一次；后端改动后重新 `cargo build` 并**重启进程**（`cargo check` ≠ `cargo build`）。

---

## 四、运行

**必须在 `betterboxd/`（workspace 根目录）启动**——静态资源 `apps/web/dist` 与默认数据目录 `data` 按当前工作目录相对解析。

```bash
cd betterboxd
./target/debug/betterboxd-server        # 或 cargo run -p betterboxd-server
# 浏览器打开 http://localhost:3000
```

启动参数（可选）：
- `--data-dir <目录>` / 环境变量 `BB_DATA`：指定数据目录（默认 `data/`，即 `data/.betterboxd/`）
- 存档功能会通过内部重启机制自动携带该参数（见「七、存档」）

**端口占用**：默认 `127.0.0.1:3000`，被占用时进程启动会重试约 6 秒后退出；先释放端口再启动。

---

## 五、第一次使用（3 步）

1. **选择存档**：启动网页进入「选择一个存档开始」——可新建（选父目录 + 存档名）、载入已有目录，或直接进入当前。
2. **配置 AI**：设置页 →「+ 添加配置」→ 填 Endpoint / API Key / Model（价格与预算可选填，每 1M token 计价；**首次保存自动启用**）→「保存」。TMDB API Key 单独一行配置同页下方。
3. **开始对话**：Chats 页「+ 新建」→ 对 AI 说「记一下：看了《花样年华》，95 分」→ 确认卡确认 → 入库。

**想先看效果**：先在设置页配置好 TMDB API Key（保存后 config.toml 入库），然后运行内置演示数据播种：

```bash
cargo run -p betterboxd-core --bin seed_demo
```

会插入 15 部影片 + 半年观影史到当前数据目录。

### 什么是「存档」？

一个存档 = 一个包含 `data.db / config.toml / sessions / posters` 的自包含目录（`.betterboxd/` 结构）。存档之间完全隔离（独立数据与配置），可随时新建、载入、切换、删除。存档列表保存在用户级 `~/.config/betterboxd/archives.json`（Windows 下为 `%USERPROFILE%\.config\betterboxd\`），切换存档会重启服务进程（几秒）。

---

## 六、常用操作

| 功能 | 入口 |
|---|---|
| 记录观影 / 写影评 / 管理想看与喜欢 | 对 AI 说，经确认卡；或 Chats / Diary / Reviews / Films 页面表单 |
| 统计（类型/年份/导演/同伴…任意维度） | 对 AI 说「统计…」；AI 现场写 SQL（只读视图 + 1000 行上限），也可保存为常用统计 |
| 讨论影片 / 一条记录 / 一篇影评 | 详情页、Diary 条目、Review 卡片上的 💬 按钮 |
| 清单（List） | Lists 页；支持排名拖拽、字母来源（Letterboxd）导入预留 |
| 会话导出 / 导入 | Chats 页顶部 ⬆ 导入 / ⬇ 导出（本项目 JSON 格式） |
| 用量与预算 | 设置页每配置卡片：总用量（历史）/ 缓存用量（预算判断，可 Reset） |
| AI 对话计费 | 每次回复下方灰色小字显示 token 消耗与花费 |

**隐私说明**：所有随记/评分/会话均存在本地数据目录；服务只监听本机回环。AI 仅获得经工具查询的数据，不会上传你的库文件。

---

## 七、测试

```bash
cargo test -p betterboxd-core      # 核心逻辑 23 个测试
npm run build                      # 前端类型检查 + 构建
```

---

## 八、常见问题

- **保存 AI 配置后还报「未配置 AI」**？新存档默认无配置——打开「设置」添加；确认 `active_profile` 已切换（卡片显示「✓ 当前」）。
- **影片搜索无结果**？检查 TMDB API Key 与代理（设置页 TMDB 一行，保存即生效）。
- **从其他目录启动 404**？必须在 `betterboxd/` 目录运行（静态资源相对路径）。
- **切换存档后网页一直转圈**？重启需要几秒，30 秒后自动提示可刷新。
- **WSL2（Windows 下）**：编译出的是 Linux 二进制，请在 WSL 内运行；选择 Windows 侧文件夹时使用 `/mnt/c/...` 路径（对应 `C:\...`）。

# 如何获取 TMDB API Key

Betterboxd 使用 [TMDB（The Movie Database）](https://www.themoviedb.org) 作为影片元数据源（海报、年份、导演、类型等）。按 TMDB 的要求，API Key 不得公开传播——请自行注册一个（免费，约 5 分钟）。

## 步骤

1. **注册账号**
   - 打开 <https://www.themoviedb.org/account/signup>（页面为英文，可选择浏览器翻译）
   - 用邮箱 + 密码注册即可，不需要手机验证
   - 注册完成后登录

2. **进入 API 设置页**
   - 访问 <https://www.themoviedb.org/settings/api>（右上角头像 → Settings → API）
   - 首次进入时可能要求填写申请理由，例如：
     - 用途（Application description）：`Personal movie diary tool（个人观影记录工具）`
     - 网站（Application URL）：`http://localhost:3000`（本地应用留空/自填均可）
     - 提交后即刻生效

3. **复制 API Key (v3)**
   - 页面左侧 **API Key (v3 auth)**，是一串 **32 位字母数字**（例如 `0123456789abcdef0123456789abcdef`）
   - 复制它。**注意**：右侧 v4 的 Bearer Token 是另一种格式，本软件不使用；请用左侧 v3 Key

4. **填入 Betterboxd**
   - 打开应用 → 设置页 → **TMDB** 区块 → 粘贴 `API Key` → 保存（无需重启，立即生效）
   - 新存档需要先在建档时自动带出或在此行重新粘贴一次

5. **验证**
   - Search 页搜索任意影片（如「花束般的恋爱」）——能出海报/年份即成功
   - 或详情页懒加载导演/时长正常显示

## 常见问题

| 问题 | 处理 |
|---|---|
| 注册页打开慢/失败 | TMDB 服务器在海外，部分网络环境需代理；可稍后重试 |
| 页面显示 v4 token 在哪 | v3 Key 始终在左侧栏；若只看到 v4，点击左侧 "API" 切换 |
| Key 复制后仍搜索失败 | 检查是否多复制了空格；保存后设置页 TMDB 行应显示完整 Key 而非打码 |
| 担心 Key 泄露 | 该 Key 仅随请求发往 `api.themoviedb.org`；请勿将 config.toml / 文档截图外发 |
| 无需 Key 也能用哪些功能 | 本地日记/影评/清单/统计/AI 对话均可离线使用；仅 TMDB 搜索、海报与元数据爬取需要 Key |

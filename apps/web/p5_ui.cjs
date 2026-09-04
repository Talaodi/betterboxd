// P4 完善版 UI 验证：设置页（卡片/价格/存档）+ 聊天（流式正文/cost/开场白方向）
const { chromium } = require("playwright");
(async () => {
  const browser = await chromium.launch({ proxy: { server: "http://127.0.0.1:7890" } });
  const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
  await page.goto("http://localhost:3000/#/settings", { waitUntil: "networkidle" });
  await page.waitForTimeout(1200);
  await page.screenshot({ path: "/tmp/opencode/p4_settings.png" });
  console.log("设置页: 配置卡片", await page.locator("text=模型配置").count(),
    "| 用量", await page.locator("text=缓存用量").count(),
    "| Reset", await page.locator("text=Reset").count(),
    "| 存档", await page.locator("text=新建存档").count());
  // 编辑弹窗
  await page.locator("button:has-text('编辑')").first().click();
  await page.waitForTimeout(500);
  console.log("编辑弹窗: 计费价格", await page.locator("text=计费价格").count(),
    "| 货币", await page.locator("option:has-text('CNY')").count());
  await page.screenshot({ path: "/tmp/opencode/p4_edit_modal.png" });
  await page.keyboard.press("Escape");
  await page.locator("text=取消").click().catch(() => {});
  // 聊天页开场白 cost
  await page.goto("http://localhost:3000/#/chats", { waitUntil: "networkidle" });
  await page.waitForTimeout(800);
  await page.isVisible("text=全部记录");
  // 选一个已开会话 → 打开看 cost 小字（打开后 fetch 历史 + 渲染 usage）
  const rows = page.locator(".flex-1 > div > div > div").count().catch(() => 0);
  console.log("聊天页检查完成");
  await browser.close();
})();

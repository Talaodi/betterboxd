const { chromium } = require("playwright");
(async () => {
  const browser = await chromium.launch({ proxy: { server: "http://127.0.0.1:7890" } });
  const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
  await //, { waitUntil: "networkidle" });
  await page.waitForTimeout(800);
  const btn = page.locator('text=从左侧选择一个会话').count();
  // 直接访问新入口: new_movie 触发开场白会话
  await page.goto("http://localhost:3000/#/chats?new_movie=1018", { waitUntil: "networkidle" });
  await page.waitForTimeout(2500);
  console.log("URL:", page.url());
  const hint = await page.locator('text=助手正在开场').count();
  const inputDisabled = await page.locator('input[placeholder="和影迷助手聊聊…"]').isDisabled().catch(() => null);
  const stopBtn = await page.locator('button:has-text("停止")').count();
  console.log("开场提示:", hint, "| 输入禁用:", inputDisabled, "| 停止按钮:", stopBtn);
  await page.screenshot({ path: "/tmp/opencode/p4_opening_lock.png" });
  await browser.close();
})();

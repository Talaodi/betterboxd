// 验证：选择屏不再无限刷新（停留 4s 监听 reload 次数）+ 新建弹层目录可浏览
const { chromium } = require("playwright");
(async () => {
  const browser = await chromium.launch({ proxy: { server: "http://127.0.0.1:7890" } });
  const ctx = await browser.newContext({ viewport: { width: 1440, height: 900 } });
  const page = await ctx.newPage();
  let reloads = 0;
  page.on("framenavigated", (f) => { if (f === page.mainFrame()) reloads++; });
  await page.goto("http://localhost:3000/#/chats", { waitUntil: "networkidle" });
  await page.waitForTimeout(4000);
  console.log("选择屏在 4s 内 reload 次数(应<=1 仅为初次加载):", reloads);
  const picker = await page.locator("text=选择一个存档开始").count();
  console.log("选择屏可见:", picker);
  // 新建 → 目录浏览器
  await page.locator("button:has-text('+ 新建存档')").click();
  await page.waitForTimeout(800);
  console.log("目录浏览器:", await page.locator("text=/\\\\/").count(), "| 存档名输入:", await page.locator("input[placeholder='存档名']").count());
  await page.screenshot({ path: "/tmp/opencode/p4_picker2.png" });
  await browser.close();
  console.log("DONE");
})();

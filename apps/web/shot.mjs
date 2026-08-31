import { chromium } from "playwright";
const [url, out] = process.argv.slice(2);
const browser = await chromium.launch({
  proxy: { server: "http://127.0.0.1:7890" },
});
const page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
await page.goto(url, { waitUntil: "domcontentloaded", timeout: 30000 });
await page.waitForTimeout(2500);
await page.screenshot({ path: out });
await browser.close();
console.log("截图:", out);

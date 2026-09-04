#!/usr/bin/env node
// Betterboxd 跨平台构建入口：node build.mjs [--release]
// 约定用法请见各平台包装脚本（build.sh / build.bat / build.command）：
//   Linux/macOS: ./build.sh [--release]   Windows: build.bat [--release]
import { execSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import path from "node:path";

const root = path.dirname(fileURLToPath(import.meta.url));
const release = process.argv.includes("--release");
const step = (msg) => console.log(`\n===> ${msg}\n`);
const run = (cmd, cwd) => execSync(cmd, { cwd, stdio: "inherit" });

try {
  step(`1/2 构建前端（apps/web，Vite…）${release ? "（release 模式）" : ""}`);
  run("npm install", path.join(root, "apps", "web"));
  run("npm run build", path.join(root, "apps", "web"));

  step("2/2 编译 Rust（betterboxd-server…）");
  run(`cargo build${release ? " --release" : ""}`, root);

  const bin = path.join(
    root,
    "target",
    release ? "release" : "debug",
    process.platform === "win32" ? "betterboxd-server.exe" : "betterboxd-server",
  );
  step("构建完成");
  console.log(`二进制：${bin}`);
  console.log("运行：进入 betterboxd/ 目录执行");
  console.log(`  ${process.platform === "win32" ? ".\\target\\" + (release ? "release" : "debug") + "\\betterboxd-server.exe" : "./target/" + (release ? "release" : "debug") + "/betterboxd-server"}`);
  console.log("然后浏览器打开 http://localhost:3000");
  console.log("\n提示：也可用 cargo run -p betterboxd-server 启动；完整使用说明见 README.md");
} catch (e) {
  console.error(`\n构建失败：${e.message}`);
  console.error("常见原因：Rust/Node 版本不足（Rust ≥1.85、Node ≥18）、缺少 C 编译器（Windows 需 MSVC Build Tools 或 MinGW；macOS 需 Xcode CLT）");
  process.exit(1);
}

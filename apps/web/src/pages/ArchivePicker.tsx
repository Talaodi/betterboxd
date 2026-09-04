import { useEffect, useState } from "react";
import { getJson, sendJson } from "../api";

type Archive = { name: string; dir: string };
type FsList = { current: string; parent: string | null; home: string; dirs: { name: string; path: string }[]; error?: string };

/** 启动存档选择屏（report 3703c41 3）：每次浏览器会话首次进入先选存档；
 *  选毕（或进入当前）写 sessionStorage 标记，后续切换/刷新不再打扰。 */
export default function ArchivePicker({ onPicked }: { onPicked: () => void }) {
  const [archives, setArchives] = useState<{ active_dir: string | null; archives: Archive[]; current: string } | null>(null);
  const [switching, setSwitching] = useState(false);
  const [browser, setBrowser] = useState<{ mode: "new" | "register"; path: string; name: string } | null>(null);
  const [fs, setFs] = useState<FsList | null>(null);
  const [targetDir, setTargetDir] = useState<string>("");

  const reload = () => getJson<typeof archives>("/api/archives").then(setArchives);
  useEffect(() => {
    reload();
  }, []);

  // 切换轮询：仅 switching=true（发起激活/新建重启）期间运行；当前目录变为目标目录 → reload。
  // 注意：此效应不可在挂载时无条件轮询——那会把「进入当前」也误判成重启完成（导致无限刷新）。
  useEffect(() => {
    if (!switching) return;
    const target = targetDir;
    const start = Date.now();
    const timer = setInterval(() => {
      fetch("/api/archives")
        .then((r) => (r.ok ? r.json() : null))
        .then((d) => {
          if (d && d.current && d.current === target) {
            clearInterval(timer);
            window.location.reload();
          }
        })
        .catch(() => {});
      if (Date.now() - start > 30000) clearInterval(timer);
    }, 800);
    return () => clearInterval(timer);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [switching]);

  const pick = async (a: Archive) => {
    if (switching) return;
    if (archives && a.dir === archives.current) {
      sessionStorage.setItem("bb-archive-picked", "1");
      onPicked();
      return;
    }
    sessionStorage.setItem("bb-archive-picked", "1");
    setSwitching(true);
    setTargetDir(a.dir);
    try {
      await sendJson("/api/archives/register", "POST", { dir: a.dir, name: a.name });
    } catch (e) {
      setSwitching(false);
      setTargetDir("");
      alert(`切换失败: ${(e as Error).message}`);
    }
  };

  const openBrowser = (mode: "new" | "register", startPath?: string) => {
    const p = startPath ?? "";
    setBrowser({ mode, path: p, name: "" });
    getJson<FsList>(`/api/fs/list${startPath ? `?path=${encodeURIComponent(startPath)}` : ""}`).then(setFs).catch(() => {});
  };

  const nav = (path: string) => {
    setBrowser((b) => (b ? { ...b, path } : b));
    getJson<FsList>(`/api/fs/list?path=${encodeURIComponent(path)}`).then(setFs).catch(() => {});
  };

  const submit = async () => {
    if (!browser || !fs) return;
    if (browser.mode === "new") {
      if (!browser.name || !fs.current) return alert("需要存档名");
      const dir = `${fs.current.replace(/[\\/]$/, "")}/${browser.name}`;
      sessionStorage.setItem("bb-archive-picked", "1");
      setSwitching(true);
      setTargetDir(dir);
      try {
        await sendJson("/api/archives/create", "POST", { name: browser.name, parent_dir: fs.current });
      } catch (e) {
        setSwitching(false);
        setTargetDir("");
        alert(`新建失败: ${(e as Error).message}`);
      }
    } else {
      pick({ name: browser.name || fs.current.split(/[\\/]/).pop() || "存档", dir: fs.current });
    }
  };

  return (
    <div className="flex min-h-screen flex-col items-center justify-center bg-[#0d1117] p-6">
      <div className="w-full max-w-[720px]">
        <h1 className="mb-1 text-2xl font-bold text-white">Betterboxd</h1>
        <p className="mb-6 text-sm text-[#5a6b7c]">选择一个存档开始</p>

        {switching && (
          <div className="mb-6 rounded-lg border border-[#33414f] bg-[#1b222b] p-4 text-sm text-[#8899aa]">
            <span className="animate-pulse">服务器切换存档中…（几秒后自动刷新）</span>
          </div>
        )}

        <div className="space-y-3">
          {(archives?.archives ?? []).map((a) => (
            <div key={a.dir} className="flex items-center justify-between rounded-lg border border-[#33414f] bg-[#1b222b] px-4 py-3">
              <div className="min-w-0">
                <span className="font-medium text-white">{a.name}</span>
                {archives?.active_dir === a.dir && <span className="ml-2 text-xs text-[#00e054]">● 当前</span>}
                <div className="truncate text-xs text-[#5a6b7c]">{a.dir}</div>
              </div>
              <button
                className="shrink-0 rounded bg-[#00e054] px-4 py-1.5 text-sm font-medium text-[#0c1a10] disabled:opacity-40"
                disabled={switching}
                onClick={() => pick(a)}
              >
                进入
              </button>
            </div>
          ))}
          {archives && archives.archives.length === 0 && (
            <p className="text-sm text-[#5a6b7c]">还没有存档。</p>
          )}
        </div>

        <div className="mt-6 flex gap-3">
          <button className="rounded border border-[#33414f] px-4 py-1.5 text-sm text-[#8899aa] hover:text-white" disabled={switching} onClick={() => openBrowser("new")}>
            + 新建存档
          </button>
          <button className="rounded border border-[#33414f] px-4 py-1.5 text-sm text-[#8899aa] hover:text-white" disabled={switching} onClick={() => openBrowser("register")}>
            载入已有存档
          </button>
        </div>

        {/* 目录浏览器弹层 */}
        {browser && fs && (
          <div className="mt-4 rounded-lg border border-[#33414f] bg-[#11151a] p-4">
            <div className="flex items-center gap-2 text-sm">
              <button className="text-xs text-[#5a6b7c] hover:text-white" onClick={() => openBrowser(browser.mode)}>🏠 主目录</button>
              {fs.parent && (
                <button className="text-xs text-[#5a6b7c] hover:text-white" onClick={() => nav(fs.parent!)}>↑ 上级</button>
              )}
              <span className="ml-auto min-w-0 truncate font-mono text-xs text-[#8899aa]" title={fs.current}>{fs.current}</span>
            </div>
            {fs.error && <p className="mt-2 text-xs text-[#ff8000]">{fs.error}</p>}
            <div className="mt-2 max-h-[220px] overflow-y-auto rounded border border-[#2c3440] bg-[#14181c]">
              {fs.dirs.length === 0 && <p className="p-3 text-xs text-[#5a6b7c]">（空目录 / 无法读取子项）</p>}
              {fs.dirs.map((d) => (
                <button key={d.path} className="block w-full truncate px-3 py-1.5 text-left text-sm text-[#8899aa] hover:bg-[#1b222b] hover:text-white"
                  onClick={() => nav(d.path)} title={d.path}>
                  📁 {d.name}
                </button>
              ))}
            </div>
            <div className="mt-3 flex items-center gap-2">
              {browser.mode === "new" && (
                <input placeholder="存档名" className="rounded border border-[#33414f] bg-[#14181c] px-2 py-1 text-sm"
                  value={browser.name} onChange={(e) => setBrowser({ ...browser, name: e.target.value })} />
              )}
              <span className="text-xs text-[#5a6b7c]">将作用于：<code className="font-mono">{fs.current}</code></span>
              <button className="ml-auto rounded bg-[#00e054] px-4 py-1 text-sm font-medium text-[#0c1a10]"
                onClick={submit}>
                {browser.mode === "new" ? "在此新建" : "载入此目录"}
              </button>
              <button className="rounded border border-[#33414f] px-3 py-1 text-sm text-[#8899aa]" onClick={() => setBrowser(null)}>取消</button>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

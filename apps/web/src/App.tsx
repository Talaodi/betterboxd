import { useEffect, useState } from "react";
import { HashRouter, Navigate, NavLink, Route, Routes } from "react-router-dom";
import ArchivePicker from "./pages/ArchivePicker";
import { getJson } from "./api";
import Diary from "./pages/Diary";
import Films from "./pages/Films";
import FilmDetail from "./pages/FilmDetail";
import Chats from "./pages/Chats";
import Reviews from "./pages/Reviews";
import Lists, { ListDetail } from "./pages/Lists";
import Search from "./pages/Search";
import Stats from "./pages/Stats";
import Settings from "./pages/Settings";

const NAV = [
  { to: "/search", label: "Search", icon: "🔍" },
  { to: "/chats", label: "Chats", icon: "🗂" },
  { to: "/diary", label: "Diary", icon: "📔" },
  { to: "/reviews", label: "Reviews", icon: "✍" },
  { to: "/films", label: "Films", icon: "🎞" },
  { to: "/lists", label: "Lists", icon: "📋" },
  { to: "/stats", label: "Stats", icon: "📊" },
];

export default function App() {
  const [picked, setPicked] = useState(() => sessionStorage.getItem("bb-archive-picked") === "1");
  const [archiveName, setArchiveName] = useState("");

  useEffect(() => {
    getJson<{ archives: { name: string; dir: string }[]; current: string; active_dir: string | null }>("/api/archives")
      .then((d) => {
        const cur = d.archives.find((a) => a.dir === d.current);
        setArchiveName(cur?.name ?? d.current);
      })
      .catch(() => {});
  }, []);

  // 新存档引导：完成后进入设置页（report b80caa5：新建存档先进入设置界面引导配置 AI）
  const [needsSetup] = useState(() => sessionStorage.getItem("bb-needs-setup") === "1");
  if (needsSetup) {
    sessionStorage.removeItem("bb-needs-setup");
    return <HashRouter><Navigate to="/settings" replace /></HashRouter>;
  }

  if (!picked) {
    return <ArchivePicker onPicked={() => { sessionStorage.setItem("bb-archive-picked", "1"); setPicked(true); }} />;
  }
  return (
    <HashRouter>
      <div className="flex h-full">
        {/* 侧边栏 */}
        <nav className="flex w-[220px] flex-col border-r border-[#1e2630] bg-[#1b222b]">
          <div className="px-4 py-4">
            <div className="text-sm font-bold tracking-wide text-[#00e054]">Betterboxd</div>
            <div className="mt-1 text-xs text-[#5a6b7c]" title="当前存档（切换需重启应用进入选择界面）">
              📦 <span className="font-medium">{archiveName || "…"}</span>
            </div>
            <NavLink to="/settings" className="text-xs text-[#5a6b7c] hover:text-[#40bcf4]">⚙ 设置</NavLink>
          </div>
          {NAV.map((n) => (
            <NavLink
              key={n.to}
              to={n.to}
              className={({ isActive }) =>
                "px-4 py-2.5 text-sm " +
                (isActive
                  ? "border-l-2 border-[#00e054] bg-[#2c3440] text-white"
                  : "border-l-2 border-transparent text-[#8899aa] hover:text-white")
              }
            >
              {n.icon} {n.label}
            </NavLink>
          ))}
          <div className="mt-auto px-4 py-3 text-xs text-[#5a6b7c]">
            v0.1.0 · M3
          </div>
        </nav>

        {/* 主内容 */}
        <main className="flex-1 min-w-0 overflow-y-auto">
          <Routes>
            <Route path="/" element={<Navigate to="/chats" replace />} />
            <Route path="/console" element={<Navigate to="/chats" replace />} />
            <Route path="/diary" element={<Diary />} />
            <Route path="/films" element={<Films />} />
            <Route path="/film/:id" element={<FilmDetail />} />
            <Route path="/search" element={<Search />} />
            <Route path="/reviews" element={<Reviews />} />
            <Route path="/lists" element={<Lists />} />
            <Route path="/lists/:id" element={<ListDetail />} />
            <Route path="/chats" element={<Chats />} />
            <Route path="/stats" element={<Stats />} />
            <Route path="/settings" element={<Settings />} />
          </Routes>
        </main>
      </div>
    </HashRouter>
  );
}

import { HashRouter, NavLink, Route, Routes } from "react-router-dom";
import Console from "./pages/Console";
import Diary from "./pages/Diary";
import Films from "./pages/Films";
import FilmDetail from "./pages/FilmDetail";
import Chats from "./pages/Chats";
import Stats from "./pages/Stats";
import Settings from "./pages/Settings";
import Placeholder from "./pages/Placeholder";

const NAV = [
  { to: "/console", label: "控制台", icon: "💬" },
  { to: "/diary", label: "Diary", icon: "📔" },
  { to: "/reviews", label: "Reviews", icon: "✍" },
  { to: "/films", label: "Films", icon: "🎞" },
  { to: "/lists", label: "Lists", icon: "📋" },
  { to: "/chats", label: "Chats", icon: "🗂" },
  { to: "/stats", label: "Stats", icon: "📊" },
];

export default function App() {
  return (
    <HashRouter>
      <div className="flex h-full">
        {/* 侧边栏 */}
        <nav className="flex w-[220px] flex-col border-r border-[#1e2630] bg-[#1b222b]">
          <div className="px-4 py-4">
            <div className="text-sm font-bold tracking-wide text-[#00e054]">Betterboxd</div>
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
            v0.1.0 · M2c
          </div>
        </nav>

        {/* 主内容 */}
        <main className="flex-1 overflow-y-auto">
          <Routes>
            <Route path="/" element={<Console />} />
            <Route path="/console" element={<Console />} />
            <Route path="/diary" element={<Diary />} />
            <Route path="/films" element={<Films />} />
            <Route path="/film/:id" element={<FilmDetail />} />
            <Route
              path="/reviews"
              element={<Placeholder title="Reviews" note="M3：影评列表页" />}
            />
            <Route
              path="/lists"
              element={<Placeholder title="Lists" note="M5：清单管理页" />}
            />
            <Route
              path="/chats"
              element={<Chats />}
            />
            <Route
              path="/stats"
              element={<Stats />}
            />
            <Route
              path="/settings"
              element={<Settings />}
            />
          </Routes>
        </main>
      </div>
    </HashRouter>
  );
}

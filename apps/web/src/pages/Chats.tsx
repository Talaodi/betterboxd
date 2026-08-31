/** Chats 页（requirement 2）：左侧会话列表（全部记录/同主题记录两 Tab +
 *  新建控制台会话），点击标题在右侧打开对应 Session（同控制台的 ChatPanel）。 */
import { useCallback, useEffect, useState } from "react";
import { Link, useSearchParams } from "react-router-dom";
import { getJson } from "../api";
import Poster from "../components/Poster";
import ChatPanel from "../components/ChatPanel";

type ChatRow = {
  id: string;
  scope: string;
  movie_id: number | null;
  title: string;
  created_at: number;
  last_message_at: number;
  movie_title: string | null;
  tmdb_id: number | null;
};

const SCOPE_LABEL: Record<string, string> = {
  global: "控制台",
  movie: "影片",
  diary_entry: "条目",
  review: "影评",
};

export default function Chats() {
  const [chats, setChats] = useState<ChatRow[] | null>(null);
  const [openId, setOpenId] = useState<string | null>(null);
  const [tab, setTab] = useState<"all" | "topic">("all");
  const [pendingNew, setPendingNew] = useState(0); // >0 → 新建流程（值作 freshKey）
  const [searchParams, setSearchParams] = useSearchParams();

  const loadChats = useCallback(() => {
    getJson<{ chats: ChatRow[] }>("/api/chats")
      .then((d) => setChats(d.chats))
      .catch(() => setChats([]));
  }, []);
  useEffect(loadChats, [loadChats]);

  // URL ?open=<id>：从影片 Log 卡等入口直达指定会话
  useEffect(() => {
    const o = searchParams.get("open");
    if (o) {
      setOpenId(o);
      setSearchParams({}, { replace: true });
    }
  }, [searchParams, setSearchParams]);

  const open = chats?.find((c) => c.id === openId) ?? null;

  // 同主题记录：与当前打开会话同主题（同一部影片，或都是控制台）
  const listed = (chats ?? []).filter((c) => {
    if (tab === "all") return true;
    if (!open) return true;
    return open.scope === "movie"
      ? c.scope === "movie" && c.movie_id === open.movie_id
      : c.scope === open.scope;
  });

  const openSession = (id: string) => {
    setOpenId(id);
    setPendingNew(0);
  };

  return (
    <div className="flex h-full min-h-0">
      {/* 左：会话列表 */}
      <div className="flex w-[320px] shrink-0 flex-col border-r border-[#1e2630]">
        <div className="flex items-center justify-between px-3 py-3">
          <div className="flex gap-1">
            {([["all", "全部记录"], ["topic", "同主题记录"]] as const).map(([k, label]) => (
              <button
                key={k}
                className={
                  "rounded px-2.5 py-1 text-xs " +
                  (tab === k ? "bg-[#2c3440] text-white" : "text-[#8899aa] hover:text-white")
                }
                onClick={() => { setTab(k); loadChats(); }}
              >
                {label}
              </button>
            ))}
          </div>
          <button
            className="rounded bg-[#00e054] px-2 py-1 text-xs font-medium text-[#0c1a10]"
            title="新建控制台会话"
            onClick={() => { setPendingNew(Date.now()); setOpenId(null); setTab("all"); }}
          >
            + 新建
          </button>
        </div>
        <div className="min-h-0 flex-1 overflow-y-auto">
          {chats === null && <p className="px-3 py-2 text-sm text-[#5a6b7c]">加载中…</p>}
          {chats !== null && chats.length === 0 && (
            <p className="px-3 py-4 text-xs text-[#5a6b7c]">还没有对话。点「+ 新建」开始，或去控制台聊聊。</p>
          )}
          {listed.map((c) => (
            <button
              key={c.id}
              className={
                "flex w-full items-center gap-2.5 border-l-2 px-3 py-2.5 text-left hover:bg-[#1b222b] " +
                (openId === c.id && !pendingNew
                  ? "border-[#00e054] bg-[#1b222b]"
                  : "border-transparent")
              }
              onClick={() => openSession(c.id)}
            >
              {c.tmdb_id ? (
                <Poster tmdbId={c.tmdb_id} title={c.movie_title ?? ""} size="thumb" className="h-[42px] w-7 rounded" />
              ) : (
                <span className="inline-flex h-7 w-7 shrink-0 items-center justify-center rounded-full bg-[#00e054] text-[10px] font-bold text-[#0c1a10]">B</span>
              )}
              <span className="min-w-0 flex-1">
                <span className="block truncate text-sm">{c.title || "（无标题）"}</span>
                <span className="block truncate text-xs text-[#5a6b7c]">
                  {c.scope === "movie" ? (c.movie_title ?? "影片") : (SCOPE_LABEL[c.scope] ?? c.scope)}
                  {" · "}
                  {new Date(c.last_message_at * 1000).toLocaleDateString("zh-CN")}
                </span>
              </span>
            </button>
          ))}
        </div>
      </div>

      {/* 右：打开的会话（与控制台同一 ChatPanel） */}
      <div className="min-w-0 flex-1 overflow-y-auto">
        <div className="mx-auto flex h-full max-w-[760px] flex-col px-6 pt-6 pb-4">
          {pendingNew ? (
            <ChatPanel
              key={`new-${pendingNew}`}
              freshKey={`new-${pendingNew}`}
              header="控制台 · 新会话"
              onSessionReady={(sid) => {
                setPendingNew(0);
                setOpenId(sid);
                loadChats();
              }}
            />
          ) : open ? (
            <ChatPanel
              key={open.id}
              sessionId={open.id}
              header={open.scope === "movie" ? `💬 ${open.movie_title ?? "影片"}` : "控制台"}
              hint={
                open.scope === "movie"
                  ? "继续这部片的讨论——助手了解影片与你的观影记录。"
                  : undefined
              }
            />
          ) : (
            <div className="flex flex-1 flex-col items-center justify-center gap-3 text-center">
              <p className="text-sm text-[#5a6b7c]">
                从左侧选择一个会话，或点「+ 新建」开始一段新的控制台对话。
              </p>
              <Link to="/console" className="text-xs text-[#40bcf4] hover:underline">
                去控制台 →
              </Link>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

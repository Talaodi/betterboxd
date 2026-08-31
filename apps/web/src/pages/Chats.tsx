/** Chats 页：左侧会话列表（全部记录/同主题记录 + 新建控制台会话），
 *  右侧打开对应 Session（与控制台同款 ChatPanel，可续聊）。 */
import { useCallback, useEffect, useState } from "react";
import { Link, useSearchParams } from "react-router-dom";
import { getJson } from "../api";
import ChatPanel from "../components/ChatPanel";
import ChatListRow from "../components/ChatListRow";

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

export default function Chats() {
  const [chats, setChats] = useState<ChatRow[] | null>(null);
  const [openId, setOpenId] = useState<string | null>(null);
  /** >0 → 新建流程激活（值为 freshKey 种子）；面板保持挂载直到首次落盘 */
  const [pendingNew, setPendingNew] = useState(0);
  const [tab, setTab] = useState<"all" | "topic">("all");
  const [searchParams, setSearchParams] = useSearchParams();

  const loadChats = useCallback(() => {
    getJson<{ chats: ChatRow[] }>("/api/chats")
      .then((d) => setChats(d.chats))
      .catch(() => setChats([]));
  }, []);
  useEffect(loadChats, [loadChats]);

  // URL ?open=<id>：从影片 Log 聊卡等入口直达指定会话
  useEffect(() => {
    const o = searchParams.get("open");
    if (o) {
      setOpenId(o);
      setPendingNew(0);
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
          {chats !== null && chats.length === 0 && !pendingNew && (
            <p className="px-3 py-4 text-xs text-[#5a6b7c]">
              还没有对话。点「+ 新建」开始，或直接对助手说点什么。
            </p>
          )}
          {listed.map((c) => (
            <ChatListRow
              key={c.id}
              sessionId={c.id}
              title={c.title || "（无标题）"}
              subtitle={`${c.scope === "movie" ? (c.movie_title ?? "影片") : "控制台"} · ${new Date(c.last_message_at * 1000).toLocaleDateString("zh-CN")}`}
              tmdbId={c.tmdb_id}
              movieTitle={c.movie_title}
              selected={openId === c.id && !pendingNew}
              onClick={() => { setOpenId(c.id); setPendingNew(0); }}
            />
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
              /* 新会话首次落盘发生在第一轮流结束；列表届时刷新 */
              onSettled={loadChats}
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
              onSettled={loadChats}
            />
          ) : (
            <div className="flex flex-1 flex-col items-center justify-center gap-3 text-center">
              <p className="text-sm text-[#5a6b7c]">
                从左侧选择一个会话，或点「+ 新建」开始一段新的控制台对话。
              </p>
              <Link to="/films" className="text-xs text-[#40bcf4] hover:underline">
                去 Films 找部片聊 →
              </Link>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

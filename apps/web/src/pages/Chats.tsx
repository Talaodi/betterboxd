/** Chats 页：左侧会话列表（全部记录/同主题记录 + 新建控制台会话），
 *  右侧打开对应 Session（与控制台同款 ChatPanel，可续聊/删除）。
 *  影片页「💬 讨论」→ /chats?new_movie=<id>：新建该影片主题会话并切至同主题 Tab。 */
import { useCallback, useEffect, useState } from "react";
import { Link, useSearchParams } from "react-router-dom";
import { getJson, sendJson } from "../api";
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

type PendingNew = {
  key: number;
  movieId?: number;
  movieTitle?: string | null;
  entryId?: string;
  reviewId?: string;
  /** 面板头：讨论对象说明（条目日期/影评标题） */
  subject?: string;
};

export default function Chats() {
  const [chats, setChats] = useState<ChatRow[] | null>(null);
  const [openId, setOpenId] = useState<string | null>(null);
  const [pendingNew, setPendingNew] = useState<PendingNew | null>(null);
  const [tab, setTab] = useState<"all" | "movies" | "topic">("all");
  const [searchParams, setSearchParams] = useSearchParams();

  const loadChats = useCallback(() => {
    getJson<{ chats: ChatRow[] }>("/api/chats")
      .then((d) => setChats(d.chats))
      .catch(() => setChats([]));
  }, []);
  useEffect(loadChats, [loadChats]);

  // URL 入口：?open=<id> 直达会话；?new_movie/new_entry/new_review 新建对应主题会话
  useEffect(() => {
    const o = searchParams.get("open");
    const nm = searchParams.get("new_movie");
    const ne = searchParams.get("new_entry");
    const nr = searchParams.get("new_review");
    if (o) {
      setOpenId(o);
      setPendingNew(null);
      setSearchParams({}, { replace: true });
    } else if (ne && /^[0-9a-f-]+$/.test(ne)) {
      setTab("movies");
      setOpenId(null);
      setPendingNew({ key: Date.now(), entryId: ne });
      setSearchParams({}, { replace: true });
      // 拉条目信息做面板头（日期+片名）
      getJson<{ entry: { movie_id: number; watched_date: string; title_main?: string } }>(
        `/api/diary/${ne}`,
      )
        .then((d) => {
          const e = d.entry;
          setPendingNew((p) =>
            p && p.entryId === ne
              ? {
                  ...p,
                  movieId: e.movie_id,
                  movieTitle: e.title_main ?? null,
                  subject: `${e.title_main ?? "影片"} · ${e.watched_date} 观看记录`,
                }
              : p,
          );
        })
        .catch(() => {});
    } else if (nr && /^[0-9a-f-]+$/.test(nr)) {
      setTab("movies");
      setOpenId(null);
      setPendingNew({ key: Date.now(), reviewId: nr });
      setSearchParams({}, { replace: true });
      getJson<{ review: { movie_id: number; title: string; title_main?: string } }>(
        `/api/reviews/${nr}`,
      )
        .then((d) => {
          const r = d.review;
          setPendingNew((p) =>
            p && p.reviewId === nr
              ? {
                  ...p,
                  movieId: r.movie_id,
                  movieTitle: r.title_main ?? null,
                  subject: `影评「${r.title || "无题"}」`,
                }
              : p,
          );
        })
        .catch(() => {});
    } else if (nm && /^\d+$/.test(nm)) {
      const mid = Number(nm);
      setTab("topic");
      setOpenId(null);
      setPendingNew({ key: Date.now(), movieId: mid });
      setSearchParams({}, { replace: true });
      // 拉片名做面板标题
      getJson<{ movie: { title_main?: string } }>(`/api/movie/${mid}`)
        .then((d) => {
          const t = d.movie?.title_main ?? null;
          setPendingNew((p) => (p && p.movieId === mid ? { ...p, movieTitle: t } : p));
        })
        .catch(() => {});
    }
  }, [searchParams, setSearchParams]);

  const open = chats?.find((c) => c.id === openId) ?? null;

  // 同主题记录锚点：打开的会话，或新建中的会话主题
  const anchor: { scope: string; movie_id: number | null } | null =
    open
      ? { scope: open.scope, movie_id: open.movie_id }
      : pendingNew
        ? { scope: pendingNew.movieId ? "movie" : "global", movie_id: pendingNew.movieId ?? null }
        : null;

  const listed = (chats ?? []).filter((c) => {
    if (tab === "all") return true;
    // P4.1：scope 统一 movie（Diary/Review 入口同样归属），按 movie_id 非空过滤
    if (tab === "movies") return c.movie_id !== null;
    if (!anchor) return true;
    return anchor.scope === "movie"
      ? c.scope === "movie" && c.movie_id === anchor.movie_id
      : c.scope === anchor.scope;
  });

  const exportOpen = async () => {
    if (!open) return;
    try {
      const d = await getJson<Record<string, unknown>>(`/api/chats/export/${open.id}`);
      const blob = new Blob([JSON.stringify(d, null, 2)], { type: "application/json" });
      const a = document.createElement("a");
      a.href = URL.createObjectURL(blob);
      a.download = `betterboxd-chat-${open.id.slice(0, 8)}.json`;
      a.click();
      URL.revokeObjectURL(a.href);
    } catch (e) {
      alert(`导出失败: ${(e as Error).message}`);
    }
  };
  const importChat = async () => {
    const input = document.createElement("input");
    input.type = "file";
    input.accept = ".json";
    input.onchange = async () => {
      const f = input.files?.[0];
      if (!f) return;
      try {
        const text = await f.text();
        const data = JSON.parse(text);
        const r = await sendJson<{ session_id: string }>("/api/chats/import", "POST", data);
        setOpenId(r.session_id);
        setPendingNew(null);
        loadChats();
      } catch (e) {
        alert(`导入失败: ${(e as Error).message}`);
      }
    };
    input.click();
  };

  const delOpen = async () => {
    if (!open) return;
    if (!window.confirm(`删除会话「${open.title || "（无标题）"}」？此操作不可撤销。`)) return;
    try {
      await sendJson(`/api/chats/${open.id}`, "DELETE");
      setOpenId(null);
      loadChats();
    } catch (e) {
      alert(`删除失败: ${(e as Error).message}`);
    }
  };

  return (
    <div className="flex h-full min-h-0">
      {/* 左：会话列表 */}
      <div className="flex w-[320px] shrink-0 flex-col border-r border-[#1e2630]">
        <div className="flex items-center justify-between px-3 py-3">
          <div className="flex gap-1">
            {([["all", "全部记录"], ["movies", "电影记录"], ["topic", "同主题记录"]] as const).map(([k, label]) => (
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
          <div className="flex items-center gap-1">
            <button
              className="rounded px-2 py-1 text-xs text-[#8899aa] hover:text-white"
              title="导入会话 JSON（本项目格式）"
              onClick={importChat}
            >
              ⬆ 导入
            </button>
            {open && (
              <button
                className="rounded px-2 py-1 text-xs text-[#8899aa] hover:text-white"
                title="导出当前会话为 JSON"
                onClick={exportOpen}
              >
                ⬇ 导出
              </button>
            )}
            <button
              className="rounded bg-[#00e054] px-2 py-1 text-xs font-medium text-[#0c1a10]"
              title="新建控制台会话"
              onClick={() => { setPendingNew({ key: Date.now() }); setOpenId(null); setTab("topic"); }}
            >
              + 新建
            </button>
          </div>
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
              onClick={() => { setOpenId(c.id); setPendingNew(null); }}
            />
          ))}
        </div>
      </div>

      {/* 右：打开的会话（与控制台同一 ChatPanel） */}
      <div className="min-w-0 flex-1 overflow-y-auto">
        <div className="mx-auto flex h-full max-w-[760px] flex-col px-6 pt-6 pb-4">
          {pendingNew ? (
            <ChatPanel
              key={`new-${pendingNew.key}`}
              freshKey={`new-${pendingNew.key}`}
              movieId={pendingNew.movieId}
              entryId={pendingNew.entryId}
              reviewId={pendingNew.reviewId}
              header={
                pendingNew.subject
                  ? `💬 ${pendingNew.subject} · 新会话`
                  : pendingNew.movieId
                    ? `💬 ${pendingNew.movieTitle ?? "影片"} · 新会话`
                    : "控制台 · 新会话"
              }
              hint={
                pendingNew.movieTitle
                  ? `和影迷助手聊聊《${pendingNew.movieTitle}》——它了解这部片和你的观影记录。`
                  : undefined
              }
              onHello={loadChats}
              onSettled={(sid) => {
                // 首轮流结束：会话已落盘 → 切回 open 模式（可删除/高亮），刷新列表
                if (sid) {
                  setPendingNew(null);
                  setOpenId(sid);
                }
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
              onSettled={loadChats}
              onDelete={delOpen}
            />
          ) : (
            <div className="flex flex-1 flex-col items-center justify-center gap-3 text-center">
              <p className="text-sm text-[#5a6b7c]">
                从左侧选择一个会话，或点「+ 新建」开始一段新的对话。
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

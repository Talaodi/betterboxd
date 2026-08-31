/** Chats 页：历史会话列表（点击展开查看消息，附继续入口）。 */
import { useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { getJson } from "../api";
import Poster from "../components/Poster";
import ReactMarkdown from "react-markdown";

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

type UiMsg = { role: "user" | "assistant"; text: string } | { role: "tool"; name: string };

const SCOPE_LABEL: Record<string, string> = {
  global: "控制台",
  movie: "影片",
  diary_entry: "条目",
  review: "影评",
};

export default function Chats() {
  const [chats, setChats] = useState<ChatRow[] | null>(null);
  const [openId, setOpenId] = useState<string | null>(null);
  const [detail, setDetail] = useState<{ messages: UiMsg[] } | null>(null);
  const [loadingId, setLoadingId] = useState<string | null>(null);

  useEffect(() => {
    getJson<{ chats: ChatRow[] }>("/api/chats")
      .then((d) => setChats(d.chats))
      .catch(() => setChats([]));
  }, []);

  const toggle = (id: string) => {
    if (openId === id) {
      setOpenId(null);
      setDetail(null);
      return;
    }
    setOpenId(id);
    setDetail(null);
    setLoadingId(id);
    getJson<{ messages: UiMsg[] }>(`/api/chats/${id}`)
      .then((d) => setDetail({ messages: d.messages ?? [] }))
      .catch(() => setDetail({ messages: [] }))
      .finally(() => setLoadingId(null));
  };

  if (chats === null) return <p className="p-6 text-[#5a6b7c]">加载中…</p>;
  if (chats.length === 0)
    return (
      <div className="p-10 text-center text-[#5a6b7c]">
        还没有对话。去控制台聊聊吧。
      </div>
    );

  return (
    <div className="mx-auto max-w-[900px] pb-10">
      <div className="flex items-center justify-between px-2 py-3">
        <span className="text-sm font-medium tracking-widest text-[#8899aa]">CHATS</span>
        <span className="text-xs text-[#5a6b7c]">共 {chats.length} 个</span>
      </div>
      {chats.map((c) => (
        <div key={c.id} className="border-b border-[#1e2630]">
          <div
            className="flex cursor-pointer items-center gap-3 px-2 py-2.5 hover:bg-[#1b222b]"
            onClick={() => toggle(c.id)}
          >
            {c.tmdb_id ? (
              <Poster tmdbId={c.tmdb_id} title={c.movie_title ?? ""} size="thumb" className="h-[48px] w-8 rounded object-cover" />
            ) : (
              <span className="inline-flex h-8 w-8 shrink-0 items-center justify-center rounded-full bg-[#00e054] text-xs font-bold text-[#0c1a10]">B</span>
            )}
            <div className="min-w-0 flex-1">
              <div className="truncate text-sm font-medium">
                {c.title || "（无标题）"}
              </div>
              <div className="text-xs text-[#5a6b7c]">
                {SCOPE_LABEL[c.scope] ?? c.scope}
                {c.movie_title ? ` · ${c.movie_title}` : ""}
                {" · "}
                {new Date(c.last_message_at * 1000).toLocaleDateString("zh-CN")}
              </div>
            </div>
            <span className="shrink-0 text-xs text-[#5a6b7c]">{openId === c.id ? "▲" : "▼"}</span>
          </div>

          {/* 展开的会话内容 */}
          {openId === c.id && (
            <div className="space-y-2 bg-[#14181c] px-4 py-3">
              {loadingId === c.id && <p className="text-xs text-[#5a6b7c]">加载中…</p>}
              {detail && detail.messages.length === 0 && (
                <p className="text-xs text-[#5a6b7c]">（该会话没有消息记录）</p>
              )}
              {detail?.messages.map((m, i) =>
                m.role === "tool" ? (
                  <div key={i} className="pl-8 text-xs text-[#5a6b7c]">
                    🔧 {m.name}
                  </div>
                ) : (
                  <div key={i} className={"flex " + (m.role === "user" ? "justify-end" : "justify-start")}>
                    <span
                      className={
                        "inline-block max-w-[85%] whitespace-pre-wrap rounded-lg px-3 py-2 text-sm " +
                        (m.role === "user" ? "bg-[#2c3440]" : "bg-[#232b35]")
                      }
                    >
                      {m.role === "assistant" ? (
                        <div className="prose prose-invert prose-sm max-w-none [&_p]:mb-1">
                          <ReactMarkdown>{m.text}</ReactMarkdown>
                        </div>
                      ) : (
                        m.text
                      )}
                    </span>
                  </div>
                ),
              )}
              {/* 继续入口 */}
              <div className="pt-1 text-right text-xs">
                {c.scope === "movie" && c.movie_id ? (
                  <Link to={`/film/${c.movie_id}`} className="text-[#40bcf4] hover:underline">
                    去影片页继续讨论 →
                  </Link>
                ) : c.scope === "global" ? (
                  <Link to="/console" className="text-[#40bcf4] hover:underline">
                    去控制台继续 →
                  </Link>
                ) : null}
              </div>
            </div>
          )}
        </div>
      ))}
    </div>
  );
}

import { useEffect, useState } from "react";
import { getJson } from "../api";

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

  useEffect(() => {
    getJson<{ chats: ChatRow[] }>("/api/chats")
      .then((d) => setChats(d.chats))
      .catch(() => setChats([]));
  }, []);

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
        <div
          key={c.id}
          className="flex items-center gap-3 border-b border-[#1e2630] px-2 py-2.5 hover:bg-[#1b222b]"
        >
          {c.tmdb_id ? (
            <img
              src={`/api/poster/${c.tmdb_id}`}
              className="h-[48px] w-8 rounded object-cover"
              onError={(e) => ((e.target as HTMLImageElement).style.visibility = "hidden")}
            />
          ) : (
            <span className="inline-flex h-8 w-8 items-center justify-center rounded-full bg-[#00e054] text-xs font-bold text-[#0c1a10]">B</span>
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
        </div>
      ))}
    </div>
  );
}

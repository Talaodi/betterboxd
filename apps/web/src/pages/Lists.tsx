/** Lists 页：清单列表 + 详情（ranked 清单显示排名，海报墙）。 */
import { useCallback, useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { getJson } from "../api";
import Poster from "../components/Poster";
import { StarsDisplay } from "../components/Stars";

type ListMeta = {
  id: string;
  name: string;
  description: string | null;
  source: string;
  ranked: number;
  created_at: number;
  updated_at: number;
  item_count: number;
};

type ListItem = {
  rank: number | null;
  added_at: number;
  tmdb_id: number;
  title_main: string;
  title_sub: string;
  year: string | null;
  my_rating: number | null;
  liked: number;
  watched: number;
  posters: string;
};

export default function Lists() {
  const [lists, setLists] = useState<ListMeta[] | null>(null);
  const [openId, setOpenId] = useState<string | null>(null);
  const [detail, setDetail] = useState<{ list: ListMeta; items: ListItem[] } | null>(null);

  const loadLists = useCallback(() => {
    getJson<{ lists: ListMeta[] }>("/api/lists")
      .then((d) => setLists(d.lists))
      .catch((e) => alert(`加载失败: ${e.message}`));
  }, []);
  useEffect(loadLists, [loadLists]);

  useEffect(() => {
    if (!openId) return;
    setDetail(null);
    getJson<{ list: ListMeta; items: ListItem[] }>(`/api/lists/${openId}`)
      .then(setDetail)
      .catch((e) => alert(`加载失败: ${e.message}`));
  }, [openId]);

  if (openId && detail) {
    const l = detail.list;
    return (
      <div className="mx-auto max-w-[1100px] px-6 pt-6">
        <button className="mb-4 text-sm text-[#40bcf4]" onClick={() => setOpenId(null)}>
          ← 返回清单列表
        </button>
        <h1 className="text-2xl font-bold">{l.name}</h1>
        {l.description && <p className="mt-1 text-sm text-[#8899aa]">{l.description}</p>}
        <p className="mt-1 text-xs text-[#5a6b7c]">
          {detail.items.length} 部影片
          {l.ranked === 1 ? " · 排名清单" : ""}
          {l.source === "letterboxd" ? " · 来自 Letterboxd" : ""}
        </p>
        <div className="mt-6 grid grid-cols-[repeat(auto-fill,minmax(130px,1fr))] gap-4">
          {detail.items.map((it) => (
            <div key={it.tmdb_id} className="relative">
              {l.ranked === 1 && it.rank !== null && (
                <span className="absolute left-1 top-1 z-10 rounded bg-black/70 px-1.5 py-0.5 text-xs font-bold text-[#ff8000]">
                  #{it.rank}
                </span>
              )}
              <Link to={`/film/${it.tmdb_id}`}>
                <Poster tmdbId={it.tmdb_id} title={it.title_main} size="grid" className="rounded" />
              </Link>
              <p className="mt-1 truncate text-xs" title={it.title_main}>{it.title_main}</p>
              <div className="flex items-center justify-between">
                <span className="text-[10px] text-[#5a6b7c]">{it.year}</span>
                {it.liked === 1 && <span className="text-[10px] text-[#00e054]">♥</span>}
              </div>
              <StarsDisplay value={it.my_rating} />
            </div>
          ))}
        </div>
        {detail.items.length === 0 && <p className="mt-6 text-sm text-[#5a6b7c]">清单为空</p>}
      </div>
    );
  }

  return (
    <div className="mx-auto max-w-[860px] px-6 pt-6">
      <div className="mb-4 flex items-baseline justify-between">
        <h1 className="text-lg tracking-wide text-[#8899aa]">Lists</h1>
        <span className="text-xs text-[#5a6b7c]">{lists?.length ?? 0} 个清单</span>
      </div>
      {!lists && <p className="text-[#5a6b7c]">加载中…</p>}
      {lists && lists.length === 0 && (
        <p className="text-sm text-[#5a6b7c]">还没有清单。可在控制台对助手说"建一个清单"。</p>
      )}
      <div className="space-y-3">
        {lists?.map((l) => (
          <button
            key={l.id}
            className="block w-full rounded-lg border border-[#2c3440] bg-[#1b222b] p-4 text-left hover:border-[#40bcf4]"
            onClick={() => setOpenId(l.id)}
          >
            <div className="flex items-baseline justify-between">
              <h2 className="text-base font-medium">{l.name}</h2>
              <span className="text-xs text-[#5a6b7c]">{l.item_count} 部</span>
            </div>
            {l.description && <p className="mt-1 text-sm text-[#8899aa]">{l.description}</p>}
            <p className="mt-2 flex gap-2 text-[10px] text-[#5a6b7c]">
              {l.ranked === 1 && <span className="rounded bg-[#2c3440] px-1.5 py-0.5">排名</span>}
              {l.source === "letterboxd" && (
                <span className="rounded bg-[#2c3440] px-1.5 py-0.5">Letterboxd</span>
              )}
              <span className="rounded bg-[#2c3440] px-1.5 py-0.5">
                {new Date(l.updated_at * 1000).toISOString().slice(0, 10)}
              </span>
            </p>
          </button>
        ))}
      </div>
    </div>
  );
}

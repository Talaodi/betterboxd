/** Search 页（report 修复版）：纵向行卡（类 Diary 布局），结果即入库（搜索时已建桩），
 *  点击直接进详情页；一次 5 条，底部「更多」每次 +5；防抖 400ms + 竞态守卫。 */
import { useEffect, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";
import { getJson, posterUrl } from "../api";

type SearchResult = {
  tmdb_id: number;
  title: string;
  original_title?: string;
  year?: string | null;
  overview?: string;
  poster_path?: string | null;
};

export default function Search() {
  const [q, setQ] = useState("");
  const [results, setResults] = useState<SearchResult[]>([]);
  const [page, setPage] = useState(0); // 已加载的页数
  const [hasMore, setHasMore] = useState(false);
  const [loading, setLoading] = useState(false);
  const [err, setErr] = useState("");
  const navigate = useNavigate();
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const seq = useRef(0);

  const fetchPage = (query: string, nextPage: number, append: boolean) => {
    const my = ++seq.current;
    setLoading(true);
    setErr("");
    getJson<{ results: SearchResult[] }>(
      `/api/tmdb/search?q=${encodeURIComponent(query.trim())}&page=${nextPage}`,
    )
      .then((d) => {
        if (my !== seq.current) return;
        setResults((prev) => (append ? [...prev, ...d.results] : d.results));
        setPage(nextPage);
        setHasMore(d.results.length >= 5);
      })
      .catch((e) => {
        if (my === seq.current) setErr((e as Error).message);
      })
      .finally(() => {
        if (my === seq.current) setLoading(false);
      });
  };

  useEffect(() => {
    if (timer.current) clearTimeout(timer.current);
    timer.current = setTimeout(() => {
      if (q.trim()) fetchPage(q, 1, false);
      else {
        setResults([]);
        setPage(0);
        setHasMore(false);
      }
    }, 400);
    return () => {
      if (timer.current) clearTimeout(timer.current);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [q]);

  return (
    <div className="mx-auto max-w-[1100px] px-6 pt-6 pb-10">
      <input
        autoFocus
        placeholder="输入片名（中/英文）…"
        className="w-full rounded border border-[#33414f] bg-[#14181c] px-3 py-2 text-sm"
        value={q}
        onChange={(e) => setQ(e.target.value)}
      />
      {err && <p className="mt-3 text-sm text-[#ff8000]">{err}</p>}
      <div className="mt-4 border-t border-[#2c3440]">
        {results.map((r) => (
          <button
            key={r.tmdb_id}
            className="flex w-full items-start gap-4 border-b border-[#1e2630] px-2 py-3 text-left hover:bg-[#1b222b]"
            onClick={() => navigate(`/film/${r.tmdb_id}`)}
          >
            <img
              src={posterUrl(r.tmdb_id)}
              onError={(e) => ((e.target as HTMLImageElement).style.visibility = "hidden")}
              className="h-[63px] w-[42px] shrink-0 rounded bg-[#14181c] object-cover"
              alt=""
            />
            <div className="min-w-0 flex-1">
              <div className="truncate text-base font-semibold">
                {r.title}
                {r.original_title && r.original_title !== r.title && (
                  <span className="ml-2 text-sm font-normal italic text-[#5a6b7c]">
                    ({r.original_title})
                  </span>
                )}
              </div>
              <p className="mt-0.5 line-clamp-2 text-xs leading-5 text-[#8899aa]">
                {r.year ? `${r.year} · ` : ""}
                {r.overview}
              </p>
            </div>
          </button>
        ))}
      </div>
      {results.length === 0 && !loading && q.trim() && !err && (
        <p className="mt-6 text-sm text-[#5a6b7c]">无结果</p>
      )}
      {loading && <p className="mt-4 text-sm text-[#5a6b7c]">搜索中…</p>}
      {hasMore && !loading && (
        <button
          className="mt-4 block w-full rounded border border-[#456] py-2 text-sm text-[#8899aa] hover:text-white"
          onClick={() => fetchPage(q, page + 1, true)}
        >
          更多
        </button>
      )}
    </div>
  );
}

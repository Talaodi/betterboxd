/** Search 页（report v2）：手动触发（按钮/Enter）；整页拉取（≤20）+ 页内切片「更多」；
 *  模块级缓存——点进详情返回后恢复搜索内容。 */
import { useRef, useState } from "react";
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

/** 模块级缓存：离开页面（进详情）不清空，返回时恢复 */
const cache: {
  q: string;
  results: SearchResult[];
  visible: number;
  page: number;
  totalPages: number;
} | null = null;
const saved: {
  q: string;
  results: SearchResult[];
  visible: number;
  page: number;
  totalPages: number;
} = { q: "", results: [], visible: 0, page: 0, totalPages: 0 };

const PAGE_SIZE = 5;

export default function Search() {
  const restored = saved.q !== "" || saved.results.length > 0;
  const [q, setQ] = useState(restored ? saved.q : "");
  const [results, setResults] = useState<SearchResult[]>(restored ? saved.results : []);
  const [visible, setVisible] = useState(restored ? saved.visible : 0);
  const [page, setPage] = useState(restored ? saved.page : 0);
  const [totalPages, setTotalPages] = useState(restored ? saved.totalPages : 0);
  const [loading, setLoading] = useState(false);
  const [err, setErr] = useState("");
  const navigate = useNavigate();
  const seq = useRef(0);
  void cache;

  const fetchPage = (query: string, nextPage: number, append: boolean) => {
    const my = ++seq.current;
    setLoading(true);
    setErr("");
    getJson<{ results: SearchResult[]; total_pages: number }>(
      `/api/tmdb/search?q=${encodeURIComponent(query.trim())}&page=${nextPage}`,
    )
      .then((d) => {
        if (my !== seq.current) return;
        const merged = append ? [...results, ...d.results] : d.results;
        setResults(merged);
        setPage(nextPage);
        setTotalPages(d.total_pages);
        setVisible(append ? visible : Math.min(PAGE_SIZE, d.results.length));
        Object.assign(saved, {
          q: query.trim(),
          results: merged,
          visible: append ? visible : Math.min(PAGE_SIZE, d.results.length),
          page: nextPage,
          totalPages: d.total_pages,
        });
      })
      .catch((e) => {
        if (my === seq.current) setErr((e as Error).message);
      })
      .finally(() => {
        if (my === seq.current) setLoading(false);
      });
  };

  const search = () => {
    if (!q.trim()) return;
    fetchPage(q, 1, false);
  };

  const more = () => {
    const nv = visible + PAGE_SIZE;
    setVisible(nv);
    Object.assign(saved, { ...saved, visible: nv });
    // 页内切片用尽且还有下一页 → 后台补拉
    if (nv > results.length && page < totalPages) fetchPage(q, page + 1, true);
  };

  const hasMore = visible < results.length || page < totalPages;

  return (
    <div className="mx-auto max-w-[1100px] px-6 pt-6 pb-10">
      <div className="flex gap-2">
        <input
          autoFocus
          placeholder="输入片名（中/英文）…"
          className="min-w-0 flex-1 rounded border border-[#33414f] bg-[#14181c] px-3 py-2 text-sm"
          value={q}
          onChange={(e) => setQ(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && search()}
        />
        <button
          className="shrink-0 rounded bg-[#00e054] px-4 py-2 text-sm font-medium text-[#0c1a10] disabled:opacity-40"
          disabled={!q.trim() || loading}
          onClick={search}
        >
          搜索
        </button>
      </div>
      {err && <p className="mt-3 text-sm text-[#ff8000]">{err}</p>}
      <div className="mt-4 border-t border-[#2c3440]">
        {results.slice(0, visible).map((r) => (
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
          onClick={more}
        >
          更多
        </button>
      )}
    </div>
  );
}

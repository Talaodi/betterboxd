/** Search 页（P2）：TMDB 搜索 → 点击结果入库（桩+懒拉取）→ 跳详情页。 */
import { useEffect, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";
import { getJson, posterUrl, sendJson } from "../api";

type SearchResult = {
  id: number;
  title: string;
  original_title?: string;
  poster_path?: string | null;
  release_date?: string | null;
  overview?: string;
};

export default function Search() {
  const [q, setQ] = useState("");
  const [results, setResults] = useState<SearchResult[] | null>(null);
  const [busy, setBusy] = useState<number | null>(null);
  const [err, setErr] = useState("");
  const navigate = useNavigate();
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const search = (query: string) => {
    if (!query.trim()) {
      setResults(null);
      return;
    }
    setErr("");
    getJson<{ results: SearchResult[] }>(
      `/api/tmdb/search?q=${encodeURIComponent(query.trim())}`,
    )
      .then((d) => setResults(d.results))
      .catch((e) => setErr((e as Error).message));
  };

  // 防抖 400ms
  useEffect(() => {
    if (timer.current) clearTimeout(timer.current);
    timer.current = setTimeout(() => search(q), 400);
    return () => {
      if (timer.current) clearTimeout(timer.current);
    };
  }, [q]);

  const add = async (r: SearchResult) => {
    setErr("");
    setBusy(r.id);
    try {
      await sendJson("/api/movies/add_from_tmdb", "POST", { tmdb_id: r.id });
      navigate(`/film/${r.id}`);
    } catch (e) {
      setErr(`入库失败: ${(e as Error).message}`);
      setBusy(null);
    }
  };

  return (
    <div className="mx-auto max-w-[1100px] px-6 pt-6 pb-10">
      <h1 className="mb-4 text-lg tracking-wide text-[#8899aa]">Search · 添加影片</h1>
      <input
        autoFocus
        placeholder="输入片名（中/英文）…"
        className="w-full rounded border border-[#33414f] bg-[#14181c] px-3 py-2 text-sm"
        value={q}
        onChange={(e) => setQ(e.target.value)}
      />
      {err && <p className="mt-3 text-sm text-[#ff8000]">{err}</p>}
      {results !== null && results.length === 0 && (
        <p className="mt-6 text-sm text-[#5a6b7c]">无结果</p>
      )}
      <div className="mt-6 grid grid-cols-[repeat(auto-fill,minmax(120px,1fr))] gap-4">
        {(results ?? []).map((r) => (
          <button
            key={r.id}
            className="group text-left disabled:opacity-50"
            disabled={busy !== null}
            onClick={() => add(r)}
            title={r.overview}
          >
            <div className="relative overflow-hidden rounded">
              <img
                src={r.poster_path ? `${posterUrl(r.id)}` : ""}
                onError={(e) => ((e.target as HTMLImageElement).style.visibility = "hidden")}
                className="aspect-[2/3] w-full bg-[#14181c] object-cover"
                alt=""
              />
              <span className="absolute inset-0 hidden items-center justify-center bg-black/60 text-sm text-[#00e054] group-hover:flex">
                {busy === r.id ? "入库中…" : "+ 入库"}
              </span>
            </div>
            <p className="mt-1 truncate text-sm" title={r.title}>
              {r.title}
            </p>
            <span className="text-[10px] text-[#5a6b7c]">
              {r.release_date?.slice(0, 4) || ""}
            </span>
          </button>
        ))}
      </div>
    </div>
  );
}

import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { getJson, type MovieRow } from "../api";
import Poster from "../components/Poster";
import { StarsDisplay } from "../components/Stars";

const TABS = [
  { key: "watched", label: "Watched" },
  { key: "watchlist", label: "Watchlist" },
  { key: "all", label: "All" },
];

export default function Films() {
  const [tab, setTab] = useState("watched");
  const [sort, setSort] = useState("recent");
  const [movies, setMovies] = useState<MovieRow[] | null>(null);
  const navigate = useNavigate();

  useEffect(() => {
    getJson<{ movies: MovieRow[] }>(`/api/movies?filter=${tab}&sort=${sort}`)
      .then((d) => setMovies(d.movies))
      .catch((e) => alert(`加载失败: ${e.message}`));
  }, [tab, sort]);

  return (
    <div className="mx-auto max-w-[1100px] p-4">
      <div className="mb-4 flex items-center justify-between">
        <div className="flex gap-1">
          {TABS.map((t) => (
            <button
              key={t.key}
              className={
                "rounded px-3 py-1.5 text-sm " +
                (tab === t.key ? "bg-[#2c3440] text-white" : "text-[#8899aa]")
              }
              onClick={() => setTab(t.key)}
            >
              {t.label}
            </button>
          ))}
        </div>
        <select
          className="rounded border border-[#33414f] bg-[#1b222b] px-2 py-1 text-sm"
          value={sort}
          onChange={(e) => setSort(e.target.value)}
        >
          <option value="recent">最近</option>
          <option value="rating">我的评分</option>
          <option value="title">片名</option>
        </select>
      </div>
      {movies === null ? (
        <p className="text-[#5a6b7c]">加载中…</p>
      ) : movies.length === 0 ? (
        <p className="p-10 text-center text-[#5a6b7c]">这里还空着。</p>
      ) : (
        <div className="grid grid-cols-[repeat(auto-fill,minmax(130px,1fr))] gap-3">
          {movies.map((m) => (
            <div
              key={m.tmdb_id}
              className="group relative cursor-pointer"
              onClick={() => navigate(`/film/${m.tmdb_id}`)}
            >
              <Poster tmdbId={m.tmdb_id} title={m.title_main} size="grid" />
              <div className="absolute left-1 top-1 rounded bg-black/60 px-1 text-xs text-[#00e054]">
                {m.watched ? <StarsDisplay value={m.my_rating} /> : "🔖"}
              </div>
              {m.liked === 1 && (
                <div className="absolute right-1 top-1 text-[#00e054]">♥</div>
              )}
              <div className="mt-1 truncate text-xs" title={`${m.title_main} ${m.title_sub ?? ""}`}>
                {m.title_main}
                <span className="ml-1 text-[#5a6b7c]">{m.year}</span>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

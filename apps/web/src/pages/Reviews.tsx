/** Reviews 页（requirement 4）：卡片左侧海报占满固定高度，右侧内容
 *  收起时裁切到海报高度，可展开全文。 */
import { useCallback, useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { getJson, sendJson } from "../api";
import Poster from "../components/Poster";
import { StarsDisplay } from "../components/Stars";
import { Markdown } from "../components/ChatPanel";

type ReviewRow = {
  review_id: string;
  movie_id: number;
  title: string;
  body_md: string;
  rating: number | null;
  liked: number;
  created_at: number;
  title_zh: string;
  title_sub: string;
  my_rating: number | null;
};

const POSTER_H = 168; // 海报固定高度（px），收起时右侧内容裁切到此高度

export default function Reviews() {
  const [reviews, setReviews] = useState<ReviewRow[] | null>(null);
  const [expanded, setExpanded] = useState<Set<string>>(new Set());

  const load = useCallback(() => {
    getJson<{ reviews: ReviewRow[] }>("/api/reviews")
      .then((d) => setReviews(d.reviews))
      .catch((e) => alert(`加载失败: ${e.message}`));
  }, []);
  useEffect(load, [load]);

  const del = async (r: ReviewRow) => {
    if (!window.confirm(`删除影评《${r.title || r.title_zh}》？此操作不可撤销。`)) return;
    try {
      await sendJson(`/api/reviews/${r.review_id}`, "DELETE");
      load();
    } catch (e) {
      alert(`删除失败: ${(e as Error).message}`);
    }
  };

  const toggle = (id: string) =>
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });

  if (!reviews) return <p className="p-6 text-[#5a6b7c]">加载中…</p>;

  return (
    <div className="mx-auto max-w-[900px] px-6 pt-6 pb-10">
      <div className="mb-4 flex items-baseline justify-between">
        <h1 className="text-lg tracking-wide text-[#8899aa]">Reviews</h1>
        <span className="text-xs text-[#5a6b7c]">{reviews.length} 篇影评</span>
      </div>
      {reviews.length === 0 && (
        <p className="text-sm text-[#5a6b7c]">
          还没有影评。去影片详情页点「+ 写影评」，或直接在控制台对助手说。
        </p>
      )}
      <div className="space-y-4">
        {reviews.map((r) => {
          const open = expanded.has(r.review_id);
          return (
            <article key={r.review_id} className="overflow-hidden rounded-lg border border-[#2c3440] bg-[#1b222b]">
              <div className="flex gap-4">
                {/* 左：海报（固定高度，收起时与右侧等高） */}
                <Link to={`/film/${r.movie_id}`} className="shrink-0 self-start">
                  <Poster
                    tmdbId={r.movie_id}
                    title={r.title_zh}
                    size="grid"
                    className="h-[168px]! w-[112px]! object-cover"
                  />
                </Link>

                {/* 右：收起时整体裁切到海报高度 */}
                <div className="min-w-0 flex-1 py-3 pr-4">
                  <div
                    className={open ? "" : "overflow-hidden"}
                    style={open ? undefined : { maxHeight: POSTER_H - 24 }}
                  >
                    <div className="flex items-start justify-between gap-3">
                      <div className="min-w-0">
                        <p className="text-xs text-[#5a6b7c]">
                          <Link to={`/film/${r.movie_id}`} className="hover:text-[#40bcf4]">
                            {r.title_zh}
                            {r.title_sub && r.title_sub !== r.title_zh && (
                              <span className="ml-1 italic">({r.title_sub})</span>
                            )}
                          </Link>
                          <span className="ml-2">
                            {new Date(r.created_at * 1000).toISOString().slice(0, 10)}
                          </span>
                        </p>
                        <h2 className="mt-1 text-base font-medium">{r.title || "无题"}</h2>
                        <div className="mt-1 flex items-center gap-3">
                          <StarsDisplay value={r.rating} />
                          {r.liked === 1 && <span className="text-sm text-[#00e054]">♥</span>}
                        </div>
                      </div>
                      <button
                        className="shrink-0 text-xs text-[#5a6b7c] hover:text-[#ff8000]"
                        onClick={() => del(r)}
                      >
                        删除
                      </button>
                    </div>
                    <div className="mt-2">
                      <Markdown>{r.body_md}</Markdown>
                    </div>
                  </div>
                  <button
                    className="mt-1 text-xs text-[#40bcf4]"
                    onClick={() => toggle(r.review_id)}
                  >
                    {open ? "收起 ▲" : "展开全文 ▼"}
                  </button>
                </div>
              </div>
            </article>
          );
        })}
      </div>
    </div>
  );
}

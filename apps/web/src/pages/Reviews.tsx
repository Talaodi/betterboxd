/** Reviews 页：全库影评列表（markdown 正文 + 删除）。 */
import { useCallback, useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { getJson, sendJson } from "../api";
import { StarsDisplay } from "../components/Stars";
import ReactMarkdown from "react-markdown";

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
    <div className="mx-auto max-w-[860px] px-6 pt-6">
      <div className="mb-4 flex items-baseline justify-between">
        <h1 className="text-lg tracking-wide text-[#8899aa]">Reviews</h1>
        <span className="text-xs text-[#5a6b7c]">{reviews.length} 篇影评</span>
      </div>
      {reviews.length === 0 && (
        <p className="text-sm text-[#5a6b7c]">
          还没有影评。去影片详情页点「+ 写影评」，或直接在控制台对助手说。
        </p>
      )}
      <div className="space-y-3">
        {reviews.map((r) => {
          const open = expanded.has(r.review_id);
          return (
            <article key={r.review_id} className="rounded-lg border border-[#2c3440] bg-[#1b222b] p-4">
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
              <div
                className={
                  "mt-3 prose prose-invert prose-sm max-w-none text-[#c8d2dc] [&_p]:mb-2 [&_strong]:text-white " +
                  (open ? "" : "max-h-[120px] overflow-hidden")
                }
              >
                <ReactMarkdown>{r.body_md}</ReactMarkdown>
              </div>
              <button
                className="mt-2 text-xs text-[#40bcf4]"
                onClick={() => toggle(r.review_id)}
              >
                {open ? "收起 ▲" : "展开全文 ▼"}
              </button>
            </article>
          );
        })}
      </div>
    </div>
  );
}

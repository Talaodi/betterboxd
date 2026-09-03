/** Reviews 页：海报卡列表（渲染复用 ReviewPosterCard）。 */
import { useCallback, useEffect, useState } from "react";
import { getJson, sendJson } from "../api";
import ReviewPosterCard from "../components/ReviewPosterCard";
import EditReviewModal from "../components/EditReviewModal";

type ReviewRow = {
  review_id: string;
  movie_id: number;
  title: string;
  body_md: string;
  rating: number | null;
  liked: number;
  created_at: number;
  signature_date: string | null;
  title_zh: string;
  title_sub: string;
  my_rating: number | null;
};

export default function Reviews() {
  const [reviews, setReviews] = useState<ReviewRow[] | null>(null);
  const [editId, setEditId] = useState<string | null>(null);

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

  if (!reviews) return <p className="p-6 text-[#5a6b7c]">加载中…</p>;

  return (
    <div className="mx-auto max-w-[900px] px-6 pt-6 pb-10">
      <div className="mb-4 flex items-baseline justify-between">
        <h1 className="text-lg tracking-wide text-[#8899aa]">Reviews</h1>
        <span className="text-xs text-[#5a6b7c]">{reviews.length} 篇影评</span>
      </div>
      {reviews.length === 0 && (
        <p className="text-sm text-[#5a6b7c]">
          还没有影评。去影片详情页点「+ 写影评」，或直接对助手说。
        </p>
      )}
      <div className="space-y-4">
        {reviews.map((r) => (
          <ReviewPosterCard
            key={r.review_id}
            review={{
              review_id: r.review_id,
              title: r.title,
              body_md: r.body_md,
              rating: r.rating,
              liked: r.liked,
              created_at: r.created_at,
              signature_date: r.signature_date,
            }}
            movieId={r.movie_id}
            movieTitle={r.title_zh}
            titleSub={r.title_sub}
            onDelete={() => del(r)}
            onEdit={() => setEditId(r.review_id)}
          />
        ))}
      </div>
      {editId && (
        <EditReviewModal reviewId={editId} onClose={() => setEditId(null)} onSaved={load} />
      )}
    </div>
  );
}

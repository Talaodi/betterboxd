/** Review 编辑弹窗（P2）：GET 回填 → title/body/rating/liked 可编辑 → PUT。 */
import { useEffect, useState } from "react";
import { getJson, sendJson } from "../api";
import { StarsEditor } from "./Stars";

type ReviewFull = {
  review_id: string;
  title: string | null;
  body_md: string;
  rating: number | null;
  liked: number;
};

export default function EditReviewModal({
  reviewId,
  onClose,
  onSaved,
}: {
  reviewId: string;
  onClose: () => void;
  onSaved: () => void;
}) {
  const [title, setTitle] = useState("");
  const [body, setBody] = useState("");
  const [rating, setRating] = useState<number | null>(null);
  const [liked, setLiked] = useState(false);
  const [ready, setReady] = useState(false);
  const [err, setErr] = useState("");

  useEffect(() => {
    getJson<{ review: ReviewFull }>(`/api/reviews/${reviewId}`)
      .then((d) => {
        setTitle(d.review.title ?? "");
        setBody(d.review.body_md);
        setRating(d.review.rating);
        setLiked(d.review.liked === 1);
        setReady(true);
      })
      .catch((e) => setErr(`回填失败: ${(e as Error).message}`));
  }, [reviewId]);

  const submit = async (override: boolean) => {
    try {
      // 覆盖警告是 200 响应体，不是 HTTP 错误
      const out = await sendJson<{ require_confirmation?: boolean }>(`/api/reviews/${reviewId}`, "PUT", {
        title,
        body_md: body,
        rating,
        liked,
        ...(override ? { override_confirmed: true } : {}),
      });
      if (out?.require_confirmation && !override) {
        if (
          window.confirm(
            "此修改将覆盖更新的最终评分/喜欢状态，确定覆盖？（账目保留历史）",
          )
        ) {
          await submit(true);
        } else {
          setErr("已取消。如需保留最新评分，请勿修改评分/喜欢。");
        }
        return;
      }
      onSaved();
      onClose();
    } catch (e) {
      setErr((e as Error).message);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60">
      <div className="w-[560px] rounded-lg border border-[#33414f] bg-[#1b222b] p-4">
        <h2 className="mb-3 text-base font-medium">编辑影评</h2>
        {err && <p className="mb-2 text-sm text-[#ff8000]">{err}</p>}
        {ready ? (
          <div className="space-y-3">
            <input
              placeholder="标题"
              className="w-full rounded border border-[#33414f] bg-[#14181c] px-2 py-1.5 text-sm"
              value={title}
              onChange={(e) => setTitle(e.target.value)}
            />
            <StarsEditor value={rating} onChange={setRating} />
            <label className="flex items-center gap-1 text-sm">
              <input type="checkbox" checked={liked} onChange={(e) => setLiked(e.target.checked)} />
              喜欢
            </label>
            <textarea
              rows={8}
              placeholder="正文（Markdown）—— 公开长评，可对外导出"
              className="w-full rounded border border-[#33414f] bg-[#14181c] px-2 py-1.5 text-sm"
              value={body}
              onChange={(e) => setBody(e.target.value)}
            />
          </div>
        ) : (
          !err && <p className="text-sm text-[#5a6b7c]">正在回填影评…</p>
        )}
        <div className="mt-4 flex justify-end gap-2">
          <button className="rounded px-3 py-1.5 text-sm text-[#8899aa]" onClick={onClose}>
            取消
          </button>
          <button
            className="rounded bg-[#00e054] px-4 py-1.5 text-sm font-medium text-[#0c1a10] disabled:opacity-40"
            disabled={!ready}
            onClick={() => submit(false)}
          >
            保存
          </button>
        </div>
      </div>
    </div>
  );
}

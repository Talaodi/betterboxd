/** Review 编辑弹窗：GET 回填 → title/body/rating/署名日期 可编辑 → PUT。
 *  终态评分改由署名日期绑定（缺省回退创建日期），无覆盖警告概念。 */
import { useEffect, useState } from "react";
import { getJson, sendJson } from "../api";
import { StarsEditor } from "./Stars";

type ReviewFull = {
  review_id: string;
  title: string | null;
  body_md: string;
  rating: number | null;
  signature_date: string | null;
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
  const [sigDate, setSigDate] = useState("");
  const [ready, setReady] = useState(false);
  const [err, setErr] = useState("");

  useEffect(() => {
    getJson<{ review: ReviewFull }>(`/api/reviews/${reviewId}`)
      .then((d) => {
        setTitle(d.review.title ?? "");
        setBody(d.review.body_md);
        setRating(d.review.rating);
        setSigDate(d.review.signature_date ?? "");
        setReady(true);
      })
      .catch((e) => setErr(`回填失败: ${(e as Error).message}`));
  }, [reviewId]);

  const submit = async () => {
    try {
      await sendJson(`/api/reviews/${reviewId}`, "PUT", {
        title,
        body_md: body,
        rating,
        signature_date: sigDate || null,
      });
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
            <label className="flex items-center gap-2 text-sm">
              <span className="text-[#8899aa]">署名日期（终态评分绑定此日期，缺省=创建日期）</span>
              <input
                type="date"
                className="rounded border border-[#33414f] bg-[#14181c] px-2 py-1 text-sm"
                value={sigDate}
                onChange={(e) => setSigDate(e.target.value)}
              />
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
            onClick={submit}
          >
            保存
          </button>
        </div>
      </div>
    </div>
  );
}

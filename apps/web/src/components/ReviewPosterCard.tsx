/** 影评海报卡（从 Reviews 页抽取，影片 Log 段复用同款渲染）。
 *  左侧海报固定高度占满；右侧内容收起时裁切到海报等高（无空白）；
 *  「展开全文」浮层按钮叠加在右下角；短文（未超高）不渲染按钮。 */
import { useEffect, useRef, useState } from "react";
import { Link } from "react-router-dom";
import Poster from "./Poster";
import { StarsDisplay } from "./Stars";
import { Markdown } from "./ChatPanel";

const POSTER_H = 168;

export type ReviewCardData = {
  title: string | null;
  body_md: string;
  rating: number | null;
  liked: number;
  created_at: number | null;
};

export default function ReviewPosterCard({
  review,
  movieId,
  movieTitle,
  titleSub,
  onDelete,
  onEdit,
}: {
  review: ReviewCardData;
  movieId: number;
  movieTitle: string;
  titleSub?: string | null;
  onDelete?: () => void;
  /** P2：打开编辑弹窗（✎） */
  onEdit?: () => void;
}) {
  const [open, setOpen] = useState(false);
  // 内容实际高度 ≤ 收起高度 → 短文，无需展开按钮
  const [overflow, setOverflow] = useState(false);
  const measureRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const el = measureRef.current;
    if (el) setOverflow(el.scrollHeight > POSTER_H - 30 + 2);
  }, [review.body_md]);
  const date = review.created_at
    ? new Date(review.created_at * 1000).toISOString().slice(0, 10)
    : null;

  return (
    <article className="overflow-hidden rounded-lg border border-[#2c3440] bg-[#1b222b]">
      <div className="flex gap-4">
        {/* 左：海报（固定高度） */}
        <Link to={`/film/${movieId}`} className="shrink-0 self-start">
          <Poster
            tmdbId={movieId}
            title={movieTitle}
            size="grid"
            className="h-[168px]! w-[112px]! object-cover"
          />
        </Link>

        {/* 右：内容（相对定位承载浮层按钮；收起时高度与海报一致） */}
        <div className="relative min-w-0 flex-1 py-2.5 pr-4">
          <div
            ref={measureRef}
            className={open ? "" : "overflow-hidden"}
            style={open ? undefined : { maxHeight: POSTER_H - 30 }}
          >
            <div className="flex items-start justify-between gap-3">
              <div className="min-w-0">
                <p className="text-xs text-[#5a6b7c]">
                  <Link to={`/film/${movieId}`} className="hover:text-[#40bcf4]">
                    {movieTitle}
                    {titleSub && titleSub !== movieTitle && (
                      <span className="ml-1 italic">({titleSub})</span>
                    )}
                  </Link>
                  {date && <span className="ml-2">{date}</span>}
                </p>
                <h2 className="mt-1 text-base font-medium">{review.title || "无题"}</h2>
                <div className="mt-1 flex items-center gap-3">
                  <StarsDisplay value={review.rating} />
                  {review.liked === 1 && <span className="text-sm text-[#00e054]">♥</span>}
                </div>
              </div>
              <div className="flex shrink-0 items-center gap-3 text-xs">
                {onEdit && (
                  <button
                    className="text-[#5a6b7c] hover:text-[#40bcf4]"
                    onClick={(e) => {
                      e.stopPropagation();
                      onEdit();
                    }}
                  >
                    ✎ 编辑
                  </button>
                )}
                {onDelete && (
                  <button
                    className="text-[#5a6b7c] hover:text-[#ff8000]"
                    onClick={(e) => {
                      e.stopPropagation();
                      onDelete();
                    }}
                  >
                    删除
                  </button>
                )}
              </div>
            </div>
            <div className="mt-2">
              <Markdown>{review.body_md}</Markdown>
            </div>
          </div>
          {/* 浮层展开按钮：不占布局高度 → 收起时无海报下方空白；短文不渲染 */}
          {(overflow || open) && (
            <button
              className="absolute bottom-1.5 right-4 rounded bg-[#232b35]/95 px-2 py-0.5 text-xs text-[#40bcf4]"
              onClick={() => setOpen(!open)}
            >
              {open ? "收起 ▲" : "展开全文 ▼"}
            </button>
          )}
        </div>
      </div>
    </article>
  );
}

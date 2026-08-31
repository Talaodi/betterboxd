/** 可互动卡片（requirement 3）：Diary 条目 / 影评 / 会话。
 *  DiaryCard 与 ReviewCard 可展开；ChatCard 点击进入 Chats 页对应会话。 */
import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { parseJsonArray, type DiaryRow } from "../api";
import { StarsDisplay } from "./Stars";
import { Markdown } from "./ChatPanel";

export function DiaryCard({
  entry,
  onDelete,
}: {
  entry: DiaryRow;
  onDelete?: (entryId: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const dims = parseJsonArray(entry.dimensions_flat);
  const tags = parseJsonArray(entry.tags);
  return (
    <div className="rounded-lg border border-[#2c3440] bg-[#1b222b]">
      <button
        className="flex w-full items-center gap-3 px-3 py-2 text-left"
        onClick={() => setOpen(!open)}
      >
        <span className="shrink-0 rounded px-1.5 py-0.5 text-xs font-medium text-[#00e054]">看</span>
        <span className="shrink-0 text-xs text-[#8899aa]">{entry.watched_date}</span>
        <span className="min-w-0 flex-1 truncate text-sm">
          {entry.private_note || `第 ${entry.rewatch_index + 1} 次观看`}
        </span>
        <StarsDisplay value={entry.rating} />
        {entry.liked === 1 && <span className="shrink-0 text-sm text-[#00e054]">♥</span>}
        {entry.in_theater === 1 && <span className="shrink-0 text-xs text-[#40bcf4]">影院</span>}
        <span className="shrink-0 text-xs text-[#5a6b7c]">{open ? "▲" : "▼"}</span>
      </button>
      {open && (
        <div className="border-t border-[#2c3440] px-4 py-3">
          <div className="mb-2 flex flex-wrap items-center gap-x-4 gap-y-1 text-xs text-[#8899aa]">
            <span>观看日期 {entry.watched_date}</span>
            <span>第 {entry.rewatch_index + 1} 刷</span>
            {entry.in_theater === 1 && (
              <span>
                影院观看{entry.ticket_price_cents != null ? ` · 票价 ¥${(entry.ticket_price_cents / 100).toFixed(0)}` : ""}
              </span>
            )}
          </div>
          {dims.length > 0 && (
            <div className="mb-2 flex flex-wrap gap-1.5">
              {dims.map((d, i) => {
                const o = d as { dimension: string; name: string };
                return (
                  <span key={i} className="rounded-full bg-[#2c3440] px-2 py-0.5 text-xs">
                    <span className="text-[#5a6b7c]">{o.dimension}</span>{" "}
                    <span className="text-[#c8d2dc]">{o.name}</span>
                  </span>
                );
              })}
            </div>
          )}
          {tags.length > 0 && (
            <div className="mb-2 flex flex-wrap gap-1.5">
              {tags.map((t, i) => (
                <span key={i} className="rounded-full border border-[#33414f] px-2 py-0.5 text-xs text-[#8899aa]">
                  #{String(t)}
                </span>
              ))}
            </div>
          )}
          {entry.private_note && (
            <p className="whitespace-pre-wrap text-sm leading-relaxed text-[#c8d2dc]">
              {entry.private_note}
            </p>
          )}
          {onDelete && (
            <button
              className="mt-2 text-xs text-[#5a6b7c] hover:text-[#ff8000]"
              onClick={(e) => {
                e.stopPropagation();
                onDelete(entry.entry_id);
              }}
            >
              删除此记录
            </button>
          )}
        </div>
      )}
    </div>
  );
}

export type ReviewPayload = {
  title: string | null;
  body_md: string;
  rating: number | null;
  liked: number;
  created_at: number | null;
};

export function ReviewCard({
  review,
  onDelete,
}: {
  review: ReviewPayload;
  onDelete?: () => void;
}) {
  const [open, setOpen] = useState(false);
  const date = review.created_at ? new Date(review.created_at * 1000).toISOString().slice(0, 10) : null;
  return (
    <div className="rounded-lg border border-[#2c3440] bg-[#1b222b]">
      <button
        className="flex w-full items-center gap-3 px-3 py-2 text-left"
        onClick={() => setOpen(!open)}
      >
        <span className="shrink-0 rounded px-1.5 py-0.5 text-xs font-medium text-[#40bcf4]">评</span>
        {date && <span className="shrink-0 text-xs text-[#8899aa]">{date}</span>}
        <span className="min-w-0 flex-1 truncate text-sm font-medium">
          {review.title || "（无题）"}
        </span>
        <StarsDisplay value={review.rating} />
        {review.liked === 1 && <span className="shrink-0 text-sm text-[#00e054]">♥</span>}
        <span className="shrink-0 text-xs text-[#5a6b7c]">{open ? "▲" : "▼"}</span>
      </button>
      {open && (
        <div className="border-t border-[#2c3440] px-4 py-3">
          <Markdown>{review.body_md}</Markdown>
          {onDelete && (
            <button
              className="mt-2 text-xs text-[#5a6b7c] hover:text-[#ff8000]"
              onClick={(e) => {
                e.stopPropagation();
                onDelete();
              }}
            >
              删除此影评
            </button>
          )}
        </div>
      )}
    </div>
  );
}

export function ChatCard({ sessionId, title, date }: { sessionId: string; title: string; date?: string }) {
  const navigate = useNavigate();
  return (
    <button
      className="flex w-full items-center gap-3 rounded-lg border border-[#2c3440] bg-[#1b222b] px-3 py-2 text-left hover:border-[#40bcf4]"
      onClick={() => navigate(`/chats?open=${sessionId}`)}
    >
      <span className="shrink-0 rounded px-1.5 py-0.5 text-xs font-medium text-[#8899aa]">聊</span>
      {date && <span className="shrink-0 text-xs text-[#8899aa]">{date}</span>}
      <span className="min-w-0 flex-1 truncate text-sm">{title || "（无标题）"}</span>
      <span className="shrink-0 text-xs text-[#40bcf4]">打开会话 →</span>
    </button>
  );
}

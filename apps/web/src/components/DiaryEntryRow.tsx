/** Diary 行卡（从 Diary 页抽取，影片 Log 段复用同款渲染）。
 *  LB 式：月份日历徽章 + 大日期 + 海报 + 片名 + 星级/喜欢/影院 + 可展开详情。 */
import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { parseJsonArray, type DiaryRow } from "../api";
import { StarsDisplay } from "./Stars";
import { Markdown } from "./ChatPanel";

const MONTHS = ["JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC"];

export default function DiaryEntryRow({
  entry,
  firstOfMonth = false,
  onDelete,
  onEdit,
}: {
  entry: DiaryRow;
  /** 该行是否为其所在月份的首行（显示月份徽章） */
  firstOfMonth?: boolean;
  onDelete?: (entryId: string) => void;
  /** P2：打开编辑弹窗（✎ 图标） */
  onEdit?: (entryId: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const navigate = useNavigate();
  const [y, m, d] = entry.watched_date.split("-");
  const dims = parseJsonArray(entry.dimensions_flat);
  const tags = parseJsonArray(entry.tags);

  return (
    <div className="border-b border-[#1e2630]">
      <div
        className="flex cursor-pointer items-center px-2 py-2 hover:bg-[#1b222b]"
        onClick={() => setOpen(!open)}
      >
        {/* 月份日历徽章（LB 式，调大版） */}
        <div className="w-24 shrink-0 text-center">
          {firstOfMonth && (
            <div className="inline-block rounded border border-[#33414f] bg-[#1b222b] px-3 py-1.5 leading-tight">
              <div className="text-lg font-bold tracking-wide text-[#dfe7ef]">
                {MONTHS[Number(m) - 1]}
              </div>
              <div className="text-sm text-[#5a6b7c]">{y}</div>
            </div>
          )}
        </div>
        {/* 大日期数字 */}
        <div className="w-14 shrink-0 text-center text-3xl text-[#8899aa]">{d}</div>
        {/* 海报 */}
        <img
          src={`/api/poster/${entry.movie_id}`}
          onError={(e) => ((e.target as HTMLImageElement).style.visibility = "hidden")}
          className="mr-4 h-[63px] w-[42px] rounded object-cover"
        />
        {/* 片名 + 标记（主副标题相同则不重复渲染） */}
        <div className="min-w-0 flex-1">
          <div className="truncate text-base font-semibold">
            {entry.title_main}
            {entry.title_sub && entry.title_sub !== entry.title_main && (
              <span className="ml-2 font-normal italic text-[#5a6b7c]">
                ({entry.title_sub})
              </span>
            )}
          </div>
        </div>
        {/* 年份 */}
        <div className="w-14 shrink-0 text-center text-sm text-[#8899aa]">{entry.year}</div>
        {/* 评分 */}
        <div className="w-28 shrink-0 text-center">
          <StarsDisplay value={entry.rating} />
        </div>
        {/* 喜欢 */}
        <div className="w-10 shrink-0 text-center">
          {entry.liked === 1 && <span className="text-[#00e054]">♥</span>}
        </div>
        {/* 讨论/编辑/删除 */}
        <div className="flex w-24 shrink-0 justify-end gap-3 pr-2 text-[#5a6b7c]">
          <span
            className="cursor-pointer hover:text-[#00e054]"
            onClick={(e) => {
              e.stopPropagation();
              navigate(`/chats?new_entry=${entry.entry_id}`);
            }}
            title="与 AI 讨论这条记录"
          >
            💬
          </span>
          <span
            className="cursor-pointer hover:text-[#40bcf4]"
            onClick={(e) => {
              e.stopPropagation();
              if (onEdit) onEdit(entry.entry_id);
              else setOpen(!open);
            }}
            title={onEdit ? "编辑" : "展开"}
          >
            ✎
          </span>
          {onDelete && (
            <span
              className="cursor-pointer hover:text-[#ff8000]"
              onClick={(e) => {
                e.stopPropagation();
                onDelete(entry.entry_id);
              }}
              title="删除"
            >
              🗑
            </span>
          )}
        </div>
      </div>
      {/* 展开区：随记 + 维度 + 标签 + 影院信息 */}
      {open && (
        <div className="ml-[120px] mr-4 border-l-2 border-[#2c3440] py-3 pl-4">
          <div className="mb-2 flex flex-wrap items-center gap-x-4 gap-y-1 text-xs text-[#8899aa]">
            {entry.in_theater === 1 && (
              <span>
                影院观看
                {entry.ticket_price_cents != null
                  ? ` · 票价 ¥${(entry.ticket_price_cents / 100).toFixed(0)}`
                  : ""}
              </span>
            )}
            {/* rewatch_index 为 1-based（ROW_NUMBER），第 1 条 = 第 1 刷 */}
            <span>第 {entry.rewatch_index} 刷</span>
          </div>
          {dims.length > 0 && (
            <div className="mb-2.5 flex flex-wrap gap-1.5">
              {dims.map((d2, di) => {
                const o = d2 as { dimension: string; name: string };
                return (
                  <span key={di} className="rounded-full bg-[#2c3440] px-2 py-0.5 text-xs">
                    <span className="text-[#5a6b7c]">{o.dimension}</span>{" "}
                    <span className="text-[#c8d2dc]">{o.name}</span>
                  </span>
                );
              })}
            </div>
          )}
          {tags.length > 0 && (
            <div className="mb-2.5 flex flex-wrap gap-1.5">
              {tags.map((t, ti) => (
                <span key={ti} className="rounded-full border border-[#33414f] px-2 py-0.5 text-xs text-[#8899aa]">
                  #{String(t)}
                </span>
              ))}
            </div>
          )}
          {entry.private_note ? (
            <div className="text-sm leading-relaxed text-[#c8d2dc]">
              <Markdown>{entry.private_note}</Markdown>
            </div>
          ) : null}
          <button
            className="mt-1.5 text-xs text-[#40bcf4] hover:underline"
            onClick={(e) => {
              e.stopPropagation();
              navigate(`/film/${entry.movie_id}`);
            }}
          >
            进影片页 →
          </button>
        </div>
      )}
    </div>
  );
}

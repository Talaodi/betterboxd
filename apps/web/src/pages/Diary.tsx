import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { getJson, parseJsonArray, sendJson, type DiaryRow } from "../api";
import { StarsDisplay } from "../components/Stars";

const MONTHS = ["JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC"];

/** LB 式 Diary：月份日历徽章 + 大日期数字 + 表格列（年份/评分/喜欢/重温/编辑）。 */
export default function Diary() {
  const [rows, setRows] = useState<DiaryRow[] | null>(null);
  const [expanded, setExpanded] = useState<string | null>(null);
  const navigate = useNavigate();

  const load = () => {
    getJson<{ entries: DiaryRow[] }>("/api/diary?limit=500")
      .then((d) => setRows(d.entries))
      .catch((e) => alert(`加载失败: ${e.message}`));
  };
  useEffect(load, []);

  const del = async (entryId: string) => {
    if (!window.confirm("删除这条观影记录？（Action 账目会保留并回退状态）")) return;
    try {
      await sendJson(`/api/diary/${entryId}`, "DELETE");
      load();
    } catch (e) {
      alert(`删除失败: ${(e as Error).message}`);
    }
  };

  if (rows === null) return <p className="p-6 text-[#5a6b7c]">加载中…</p>;
  if (rows.length === 0)
    return (
      <div className="p-10 text-center text-[#5a6b7c]">
        还没有观影记录。
        <button className="ml-2 text-[#00e054]" onClick={() => navigate("/console")}>
          去控制台记一笔 →
        </button>
      </div>
    );

  let prevMonth = "";

  return (
    <div className="mx-auto max-w-[1100px] pb-10">
      {/* LB 式筛选条（M3 补全交互） */}
      <div className="flex items-center justify-between px-2 py-3">
        <span className="text-sm font-medium tracking-widest text-[#8899aa]">DIARY</span>
        <span className="text-xs text-[#5a6b7c]">共 {rows.length} 条</span>
      </div>
      <div className="border-t border-[#2c3440]">
        {rows.map((r) => {
          const [y, m, d] = r.watched_date.split("-");
          const monthKey = `${y}-${m}`;
          const firstOfMonth = monthKey !== prevMonth;
          prevMonth = monthKey;
          const isExpanded = expanded === r.entry_id;
          return (
            <div key={r.entry_id} className="border-b border-[#1e2630]">
              <div
                className="flex cursor-pointer items-center px-2 py-2 hover:bg-[#1b222b]"
                onClick={() => setExpanded(isExpanded ? null : r.entry_id)}
              >
                {/* 月份日历徽章（每月首行） */}
                <div className="w-20 shrink-0 text-center">
                  {firstOfMonth && (
                    <div className="inline-block rounded border border-[#33414f] bg-[#1b222b] px-2 py-1 leading-tight">
                      <div className="text-sm font-bold text-[#dfe7ef]">
                        {MONTHS[Number(m) - 1]}
                      </div>
                      <div className="text-xs text-[#5a6b7c]">{y}</div>
                    </div>
                  )}
                </div>
                {/* 大日期数字 */}
                <div className="w-14 shrink-0 text-center text-3xl text-[#8899aa]">
                  {d}
                </div>
                {/* 海报 */}
                <img
                  src={`/api/poster/${r.movie_id}`}
                  onError={(e) => ((e.target as HTMLImageElement).style.visibility = "hidden")}
                  className="mr-4 h-[63px] w-[42px] rounded object-cover"
                />
                {/* 片名 + 标记 */}
                <div className="min-w-0 flex-1">
                  <div className="truncate text-base font-semibold">
                    {r.title_main}
                    <span className="ml-2 text-sm font-normal italic text-[#5a6b7c]">
                      ({r.title_sub})
                    </span>
                  </div>
                </div>
                {/* 年份 */}
                <div className="w-14 shrink-0 text-center text-sm text-[#8899aa]">
                  {r.year}
                </div>
                {/* 评分 */}
                <div className="w-28 shrink-0 text-center">
                  <StarsDisplay value={r.rating} />
                </div>
                {/* 喜欢 */}
                <div className="w-10 shrink-0 text-center">
                  {r.liked === 1 && <span className="text-[#00e054]">♥</span>}
                </div>
                {/* 编辑/删除 */}
                <div className="flex w-16 shrink-0 justify-end gap-3 pr-2 text-[#5a6b7c]">
                  <span
                    className="cursor-pointer hover:text-[#40bcf4]"
                    onClick={(e) => {
                      e.stopPropagation();
                      setExpanded(isExpanded ? null : r.entry_id);
                    }}
                    title="展开/编辑"
                  >
                    ✎
                  </span>
                  <span
                    className="cursor-pointer hover:text-[#ff8000]"
                    onClick={(e) => {
                      e.stopPropagation();
                      del(r.entry_id);
                    }}
                    title="删除"
                  >
                    🗑
                  </span>
                </div>
              </div>
              {/* 展开区：随记 + 维度 + 双轨提示 */}
              {isExpanded && (
                <div className="space-y-3 bg-[#161c23] px-10 py-4 text-sm">
                  {parseJsonArray(r.dimensions_flat).length > 0 && (
                    <p className="text-xs text-[#8899aa]">
                      {parseJsonArray(r.dimensions_flat)
                        .map((d) => {
                          const o = d as { dimension: string; name: string };
                          return `${o.dimension}:${o.name}`;
                        })
                        .join(" · ")}
                    </p>
                  )}
                  {r.private_note ? (
                    <p className="whitespace-pre-wrap leading-relaxed text-[#dfe7ef]">{r.private_note}</p>
                  ) : (
                    <p className="text-[#5a6b7c]">（无随记）</p>
                  )}
                  {parseJsonArray(r.dimensions_flat).length > 0 && (
                    <p className="text-xs text-[#8899aa]">
                      {parseJsonArray(r.dimensions_flat)
                        .map((d) => {
                          const o = d as { dimension: string; name: string };
                          return `${o.dimension}:${o.name}`;
                        })
                        .join(" · ")}
                    </p>
                  )}
                  <button className="text-xs text-[#40bcf4]" onClick={() => navigate(`/film/${r.movie_id}`)}>
                    进影片页 →
                  </button>
                </div>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}

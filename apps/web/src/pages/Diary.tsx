/** LB 式 Diary：月份日历徽章 + 大日期数字 + 行卡（渲染复用 DiaryEntryRow）。 */
import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { getJson, sendJson, type DiaryRow } from "../api";
import DiaryEntryRow from "../components/DiaryEntryRow";

export default function Diary() {
  const [rows, setRows] = useState<DiaryRow[] | null>(null);
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
        <button className="ml-2 text-[#00e054]" onClick={() => navigate("/chats")}>
          去 Chats 记一笔 →
        </button>
      </div>
    );

  let prevMonth = "";

  return (
    <div className="mx-auto max-w-[1100px] pb-10">
      <div className="flex items-center justify-between px-2 py-3">
        <span className="text-sm font-medium tracking-widest text-[#8899aa]">DIARY</span>
        <span className="text-xs text-[#5a6b7c]">共 {rows.length} 条</span>
      </div>
      <div className="border-t border-[#2c3440]">
        {rows.map((r) => {
          const monthKey = r.watched_date.slice(0, 7);
          const firstOfMonth = monthKey !== prevMonth;
          prevMonth = monthKey;
          return (
            <DiaryEntryRow key={r.entry_id} entry={r} firstOfMonth={firstOfMonth} onDelete={del} />
          );
        })}
      </div>
    </div>
  );
}

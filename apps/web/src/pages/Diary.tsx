import { useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import { getJson, parseJsonArray, posterUrl, sendJson, type DiaryRow } from "../api";
import { StarsDisplay } from "../components/Stars";

const fmtDims = (row: DiaryRow) =>
  parseJsonArray(row.dimensions_flat)
    .map((d) => {
      const o = d as { dimension: string; name: string };
      return `${o.dimension}:${o.name}`;
    })
    .join(" · ");

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

  const groups = useMemo(() => {
    const map = new Map<string, DiaryRow[]>();
    for (const r of rows ?? []) {
      const m = r.watched_date.slice(0, 7);
      if (!map.has(m)) map.set(m, []);
      map.get(m)!.push(r);
    }
    return [...map.entries()].map(([month, rs]) => {
      const rated = rs.filter((r) => r.rating !== null);
      return {
        month,
        rows: rs,
        count: rs.length,
        avg: rated.length
          ? Math.round(rated.reduce((a, r) => a + (r.rating ?? 0), 0) / rated.length)
          : null,
        cost: rs.reduce((a, r) => a + (r.ticket_price_cents ?? 0), 0) / 100,
      };
    });
  }, [rows]);

  const del = async (entryId: string) => {
    if (!window.confirm("删除这条观影记录？（Action 账目会保留并回退状态）")) return;
    await sendJson(`/api/diary/${entryId}`, "DELETE");
    load();
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

  return (
    <div className="mx-auto max-w-[900px] pb-10">
      {groups.map((g) => (
        <div key={g.month}>
          <div className="sticky top-0 z-10 border-b border-[#2c3440] bg-[#14181c] px-2 py-2 text-sm text-[#8899aa]">
            {g.month} · {g.count} 部
            {g.avg !== null && ` · 均分 ${g.avg}`}
            {g.cost > 0 && ` · 花费 ¥${g.cost}`}
          </div>
          {g.rows.map((r) => (
            <div key={r.entry_id} className="border-b border-[#1e2630]">
              <div
                className="flex cursor-pointer items-center gap-3 px-2 py-2 hover:bg-[#1b222b]"
                onClick={() => setExpanded(expanded === r.entry_id ? null : r.entry_id)}
                onDoubleClick={() => navigate(`/film/${r.movie_id}`)}
              >
                <img
                  src={posterUrl(r.movie_id)}
                  onError={(e) => ((e.target as HTMLImageElement).style.visibility = "hidden")}
                  className="h-[72px] w-12 rounded object-cover"
                />
                <div className="flex-1">
                  <div className="text-sm font-medium">
                    {r.title_main}
                    <span className="ml-2 text-xs text-[#5a6b7c]">{r.title_sub}</span>
                  </div>
                  <div className="mt-1 flex flex-wrap items-center gap-2 text-xs text-[#8899aa]">
                    <span>{r.watched_date}</span>
                    <StarsDisplay value={r.rating} />
                    {r.liked === 1 && <span className="text-[#00e054]">♥</span>}
                    {r.in_theater === 1 && (
                      <span className="text-[#40bcf4]">🎬{r.ticket_price_cents ? `¥${r.ticket_price_cents / 100}` : ""}</span>
                    )}
                    <span className="rounded bg-[#2c3440] px-1">第{r.rewatch_index}次</span>
                    {parseJsonArray(r.tags).slice(0, 3).map((t) => (
                      <span key={String(t)} className="rounded bg-[#2c3440] px-1 text-[#8899aa]">
                        {String(t)}
                      </span>
                    ))}
                  </div>
                </div>
                <span className="text-[#5a6b7c]">{expanded === r.entry_id ? "⌃" : "⌄"}</span>
              </div>
              {expanded === r.entry_id && (
                <div className="space-y-2 bg-[#161c23] px-4 py-3 text-sm">
                  {r.private_note ? (
                    <p className="whitespace-pre-wrap text-[#dfe7ef]">{r.private_note}</p>
                  ) : (
                    <p className="text-[#5a6b7c]">（无随记）</p>
                  )}
                  {r.rating !== null && r.my_rating !== null && r.rating !== r.my_rating && (
                    <p className="text-xs text-[#5a6b7c]">
                      当次 {r.rating} · 影片最终评分 {r.my_rating}（双轨评分）
                    </p>
                  )}
                  <p className="text-xs text-[#8899aa]">{fmtDims(r) || "无维度标记"}</p>
                  <div className="flex gap-3 text-xs">
                    <button className="text-[#40bcf4]" onClick={() => navigate(`/film/${r.movie_id}`)}>
                      进影片页
                    </button>
                    <button className="text-[#ff8000]" onClick={() => del(r.entry_id)}>
                      删除
                    </button>
                  </div>
                </div>
              )}
            </div>
          ))}
        </div>
      ))}
    </div>
  );
}

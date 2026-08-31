import { useEffect, useState } from "react";
import { getJson, sendJson } from "../api";

type SavedQuery = {
  id: string;
  name: string;
  payload_json: string;
  last_run_at: number | null;
};

type ChartData = {
  ok: boolean;
  sql: string;
  chart: { type?: string; title?: string };
  columns: string[];
  rows: Record<string, unknown>[];
  truncated?: boolean;
};

function SimpleBar({ data }: { data: ChartData }) {
  const { columns, rows } = data;
  if (rows.length === 0 || columns.length < 2) return null;
  const max = Math.max(...rows.map((r) => Number(r[columns[1]]) || 0), 1);
  return (
    <div className="space-y-1.5 py-2">
      {rows.slice(0, 15).map((r, i) => {
        const label = String(r[columns[0]] ?? "");
        const val = Number(r[columns[1]]) || 0;
        return (
          <div key={i} className="flex items-center gap-2 text-xs">
            <span className="w-24 shrink-0 truncate text-right text-[#8899aa]">{label}</span>
            <div className="h-4 flex-1 rounded-sm bg-[#1e2630]">
              <div className="h-full rounded-sm bg-[#00e054]/60" style={{ width: `${(val / max) * 100}%` }} />
            </div>
            <span className="w-12 text-right text-[#dfe7ef]">{val}</span>
          </div>
        );
      })}
    </div>
  );
}

export default function Stats() {
  const [queries, setQueries] = useState<SavedQuery[] | null>(null);
  const [charts, setCharts] = useState<Record<string, ChartData>>({});
  const [showNew, setShowNew] = useState(false);
  const [nl, setNl] = useState("");
  const [running, setRunning] = useState(false);
  const [pendingSave, setPendingSave] = useState<ChartData | null>(null);

  const loadQueries = () => {
    getJson<{ queries: SavedQuery[] }>("/api/saved-queries")
      .then((d) => setQueries(d.queries))
      .catch(() => setQueries([]));
  };
  useEffect(loadQueries, []);

  const runNL = async () => {
    if (!nl.trim() || running) return;
    setRunning(true);
    try {
      const d = await sendJson<ChartData>("/api/stats/run", "POST", { nl });
      setCharts((c) => ({ ...c, __new: d }));
      setPendingSave(d);
    } catch (e) {
      alert(`查询失败: ${(e as Error).message}`);
    }
    setRunning(false);
  };

  const runSaved = async (sq: SavedQuery) => {
    try {
      const payload = JSON.parse(sq.payload_json);
      const d = await sendJson<ChartData>("/api/stats/run", "POST", {
        sql: payload.sql, chart: payload.chart,
      });
      setCharts((c) => ({ ...c, [sq.id]: d }));
    } catch (e) {
      alert(`查询失败: ${(e as Error).message}`);
    }
  };

  const saveNew = async () => {
    const d = charts["__new"];
    if (!d) return;
    const name = window.prompt("统计名称：", d.chart?.title ?? "未命名");
    if (!name) return;
    await sendJson("/api/saved-queries", "POST", {
      name, payload: { sql: d.sql, chart: d.chart },
    });
    setPendingSave(null);
    loadQueries();
  };

  const del = async (id: string) => {
    if (!window.confirm("删除此统计？")) return;
    await sendJson(`/api/saved-queries/${id}`, "DELETE");
    loadQueries();
  };

  const renderCard = (id: string, name: string, data: ChartData, onDelete?: () => void) => (
    <div key={id} className="rounded-lg border border-[#33414f] bg-[#1b222b] p-3">
      <div className="mb-2 flex items-center justify-between">
        <span className="text-sm font-medium text-[#dfe7ef]">{name}</span>
        {onDelete && (
          <button className="text-xs text-[#5a6b7c] hover:text-[#ff8000]" onClick={onDelete}>🗑</button>
        )}
      </div>
      {data.rows.length === 0 ? (
        <p className="py-6 text-center text-xs text-[#5a6b7c]">无数据</p>
      ) : data.chart?.type === "table" || data.columns.length > 3 ? (
        <div className="max-h-[220px] overflow-auto text-xs">
          <table className="w-full text-left">
            <thead><tr>{data.columns.map(c => <th key={c} className="border-b border-[#2c3440] px-2 py-1 text-[#5a6b7c]">{c}</th>)}</tr></thead>
            <tbody>{data.rows.slice(0, 20).map((r, i) => (
              <tr key={i}>{data.columns.map(c => <td key={c} className="border-b border-[#1e2630] px-2 py-1">{String(r[c] ?? "")}</td>)}</tr>
            ))}</tbody>
          </table>
        </div>
      ) : (
        <SimpleBar data={data} />
      )}
    </div>
  );

  return (
    <div className="mx-auto max-w-[1100px] p-4">
      <div className="mb-4 flex items-center justify-between">
        <h1 className="text-lg font-medium">Stats</h1>
        <button
          className="rounded bg-[#00e054] px-3 py-1.5 text-sm font-medium text-[#0c1a10]"
          onClick={() => setShowNew(true)}
        >
          + 新建查询
        </button>
      </div>

      {showNew && (
        <div className="mb-6 rounded-lg border border-[#33414f] bg-[#1b222b] p-4">
          <div className="mb-3 flex gap-2">
            <input
              className="flex-1 rounded border border-[#33414f] bg-[#14181c] px-3 py-2 text-sm"
              placeholder="描述统计需求，如：今年各月观影量和平均分"
              value={nl}
              onChange={(e) => setNl(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && runNL()}
            />
            <button
              className="rounded bg-[#40bcf4] px-4 py-2 text-sm font-medium text-[#0c1a10] disabled:opacity-40"
              onClick={runNL}
              disabled={running || !nl.trim()}
            >
              {running ? "查询中…" : "查询"}
            </button>
          </div>
          {charts["__new"] && (
            <div>
              {renderCard("__new", charts["__new"].chart?.title ?? "新查询", charts["__new"])}
              {pendingSave && (
                <button
                  className="mt-2 rounded bg-[#00e054] px-3 py-1.5 text-sm font-medium text-[#0c1a10]"
                  onClick={saveNew}
                >
                  💾 保存此统计
                </button>
              )}
            </div>
          )}
        </div>
      )}

      {queries === null ? (
        <p className="text-[#5a6b7c]">加载中…</p>
      ) : queries.length === 0 ? (
        <p className="p-10 text-center text-[#5a6b7c]">
          还没有收藏的统计。点击"新建查询"开始。
        </p>
      ) : (
        <div className="grid grid-cols-[repeat(auto-fill,minmax(340px,1fr))] gap-4">
          {queries.map((sq) => {
            const payload = JSON.parse(sq.payload_json);
            return (
              <div key={sq.id}>
                {renderCard(sq.id, sq.name, {
                  ok: true, sql: payload.sql ?? "", chart: payload.chart ?? {},
                  columns: [], rows: [],
                })}
                <div className="mt-1 flex gap-2 text-xs">
                  <button className="text-[#40bcf4]" onClick={() => runSaved(sq)}>▶ 重跑</button>
                  <button className="text-[#5a6b7c]" onClick={() => del(sq.id)}>🗑</button>
                </div>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}

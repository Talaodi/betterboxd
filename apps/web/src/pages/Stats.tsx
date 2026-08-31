/** Stats 页：已保存统计项目列表（SQL 只读展示 + 结果可重跑）。
 *  统计项目的创建只能在控制台与 AI 对话中进行（run_stats + manage_saved_queries）。 */
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

function ResultView({ data }: { data: ChartData }) {
  const { columns, rows } = data;
  if (rows.length === 0) return <p className="py-4 text-center text-xs text-[#5a6b7c]">无数据</p>;
  // 两列且第二列为数值 → 横向条形图；否则表格
  const numericSecond =
    columns.length === 2 && rows.every((r) => !isNaN(Number(r[columns[1]])));
  if (numericSecond) {
    const max = Math.max(...rows.map((r) => Number(r[columns[1]]) || 0), 1);
    return (
      <div className="space-y-1.5 py-2">
        {rows.slice(0, 15).map((r, i) => {
          const label = String(r[columns[0]] ?? "");
          const val = Number(r[columns[1]]) || 0;
          return (
            <div key={i} className="flex items-center gap-2 text-xs">
              <span className="w-28 shrink-0 truncate text-right text-[#8899aa]">{label}</span>
              <div className="h-4 flex-1 rounded-sm bg-[#1e2630]">
                <div className="h-full rounded-sm bg-[#00e054]/60" style={{ width: `${(val / max) * 100}%` }} />
              </div>
              <span className="w-14 text-right text-[#dfe7ef]">{val}</span>
            </div>
          );
        })}
      </div>
    );
  }
  return (
    <div className="max-h-[260px] overflow-auto text-xs">
      <table className="w-full text-left">
        <thead>
          <tr>
            {columns.map((c) => (
              <th key={c} className="border-b border-[#2c3440] px-2 py-1 text-[#5a6b7c]">{c}</th>
            ))}
          </tr>
        </thead>
        <tbody>
          {rows.slice(0, 20).map((r, i) => (
            <tr key={i}>
              {columns.map((c) => (
                <td key={c} className="border-b border-[#1e2630] px-2 py-1">{String(r[c] ?? "")}</td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

export default function Stats() {
  const [queries, setQueries] = useState<SavedQuery[] | null>(null);
  const [results, setResults] = useState<Record<string, ChartData>>({});
  const [running, setRunning] = useState<string | null>(null);

  const loadQueries = () => {
    getJson<{ queries: SavedQuery[] }>("/api/saved-queries")
      .then((d) => setQueries(d.queries))
      .catch(() => setQueries([]));
  };
  useEffect(loadQueries, []);

  const runSaved = async (sq: SavedQuery) => {
    if (running) return;
    setRunning(sq.id);
    try {
      const d = await sendJson<ChartData>("/api/stats/run", "POST", {
        saved_query_id: sq.id,
      });
      setResults((c) => ({ ...c, [sq.id]: d }));
    } catch (e) {
      alert(`重跑失败: ${(e as Error).message}`);
    }
    setRunning(null);
  };

  const del = async (sq: SavedQuery) => {
    if (!window.confirm(`删除统计项目「${sq.name}」？此操作不可撤销。`)) return;
    try {
      await sendJson(`/api/saved-queries/${sq.id}`, "DELETE");
      setResults((c) => {
        const n = { ...c };
        delete n[sq.id];
        return n;
      });
      loadQueries();
    } catch (e) {
      alert(`删除失败: ${(e as Error).message}`);
    }
  };

  return (
    <div className="mx-auto max-w-[900px] p-4">
      <div className="mb-1 flex items-baseline justify-between">
        <h1 className="text-lg tracking-wide text-[#8899aa]">Stats</h1>
        <span className="text-xs text-[#5a6b7c]">{queries?.length ?? 0} 个统计项目</span>
      </div>
      <p className="mb-4 text-xs text-[#5a6b7c]">
        统计项目在控制台与 AI 对话创建——例如对助手说"统计今年各月观影量，并保存为统计项目"。
      </p>

      {queries === null ? (
        <p className="text-[#5a6b7c]">加载中…</p>
      ) : queries.length === 0 ? (
        <div className="rounded-lg border border-dashed border-[#33414f] p-10 text-center text-sm text-[#5a6b7c]">
          还没有统计项目。
          <br />
          去 <span className="text-[#40bcf4]">控制台</span> 对助手说，例如：
          "统计今年各月观影量和平均分"——AI 会先算给你看，确认后即可保存到这里。
        </div>
      ) : (
        <div className="space-y-4">
          {queries.map((sq) => {
            let payload: { sql?: string; chart?: { title?: string } } = {};
            try {
              payload = JSON.parse(sq.payload_json);
            } catch { /* 容忍损坏 payload */ }
            const result = results[sq.id];
            return (
              <div key={sq.id} className="rounded-lg border border-[#33414f] bg-[#1b222b] p-4">
                <div className="flex items-start justify-between gap-3">
                  <div className="min-w-0">
                    <h2 className="text-base font-medium">{sq.name}</h2>
                    <p className="mt-0.5 text-xs text-[#5a6b7c]">
                      {sq.last_run_at
                        ? `最近运行 ${new Date(sq.last_run_at * 1000).toLocaleDateString("zh-CN")}`
                        : "从未运行"}
                    </p>
                  </div>
                  <div className="flex shrink-0 gap-3 text-xs">
                    <button
                      className="text-[#40bcf4] disabled:opacity-40"
                      disabled={running !== null}
                      onClick={() => runSaved(sq)}
                    >
                      {running === sq.id ? "运行中…" : "▶ 重跑"}
                    </button>
                    <button className="text-[#5a6b7c] hover:text-[#ff8000]" onClick={() => del(sq)}>
                      🗑
                    </button>
                  </div>
                </div>

                {/* SQL 只读展示（默认折叠） */}
                <details className="mt-2">
                  <summary className="cursor-pointer text-xs text-[#5a6b7c] hover:text-[#40bcf4]">
                    查看 SQL
                  </summary>
                  <pre className="mt-1 max-h-[200px] overflow-auto rounded border border-[#2c3440] bg-[#14181c] p-2 text-xs text-[#40bcf4]">
                    {payload.sql ?? "（无 SQL）"}
                  </pre>
                </details>

                {/* 结果区（重跑后展示） */}
                {result && (
                  <div className="mt-3 border-t border-[#2c3440] pt-2">
                    <ResultView data={result} />
                    {result.truncated && (
                      <p className="text-xs text-[#ff8000]">结果已截断到 1000 行</p>
                    )}
                  </div>
                )}
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}

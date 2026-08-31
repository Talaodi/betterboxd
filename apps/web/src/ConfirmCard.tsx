import { useEffect, useState } from "react";

type Pending = {
  call_id: string;
  name: string;
  args: Record<string, unknown>;
  movieTitle?: string;
};

/** 维度槽位（design.md 定稿四槽） */
const DIMS = ["地点", "同伴", "情绪", "场景"];

/** 可编辑确认卡：Agent 写操作的等待裁决界面。
 *  - 确认：以（可能被用户修改的）args 回传执行
 *  - 拒绝：回传 None，Agent 会继续对话询问原因
 */
export default function ConfirmCard({
  pending,
  onConfirm,
  onReject,
}: {
  pending: Pending;
  onConfirm: (args: Record<string, unknown>) => void;
  onReject: () => void;
}) {
  const [values, setValues] = useState<Record<string, unknown>>(pending.args);

  useEffect(() => {
    setValues(pending.args);
  }, [pending.call_id]); // eslint-disable-line react-hooks/exhaustive-deps

  const set = (k: string, v: unknown) =>
    setValues((old) => ({ ...old, [k]: v }));

  const field = (k: string): React.ReactNode => {
    const v = values[k];
    if (k === "action") return null;
    if (k === "movie_id") {
      return (
        <div key={k} className="text-sm">
          <span className="text-[#5a6b7c]">影片：</span>
          <span className="text-[#40bcf4]">{pending.movieTitle ?? String(v)}</span>
        </div>
      );
    }
    if (k === "watched_date") {
      return (
        <label key={k} className="block text-sm">
          <span className="text-[#5a6b7c]">观看日期 </span>
          <input
            type="date"
            className="mt-1 w-full rounded border border-[#33414f] bg-[#14181c] px-2 py-1"
            value={v == null ? "" : String(v)}
            onChange={(e) => set(k, e.target.value)}
          />
        </label>
      );
    }
    if (k === "rating" || k === "my_rating") {
      return (
        <label key={k} className="block text-sm">
          <span className="text-[#5a6b7c]">评分（0-100，半星=10） </span>
          <input
            type="number"
            min={0}
            max={100}
            step={10}
            className="mt-1 w-full rounded border border-[#33414f] bg-[#14181c] px-2 py-1"
            value={v === null || v === undefined ? "" : String(v)}
            onChange={(e) =>
              set(k, e.target.value === "" ? null : Number(e.target.value))
            }
          />
        </label>
      );
    }
    if (k === "ticket_price_cents") {
      return (
        <label key={k} className="block text-sm">
          <span className="text-[#5a6b7c]">票价（元） </span>
          <input
            type="number"
            min={0}
            step="0.01"
            className="mt-1 w-full rounded border border-[#33414f] bg-[#14181c] px-2 py-1"
            value={typeof v === "number" ? v / 100 : ""}
            onChange={(e) =>
              set(
                k,
                e.target.value === ""
                  ? null
                  : Math.round(Number(e.target.value) * 100),
              )
            }
          />
        </label>
      );
    }
    if (k === "liked" || k === "in_theater" || k === "in_watchlist") {
      const label =
        k === "liked" ? "喜欢" : k === "in_theater" ? "影院观看" : "想看";
      return (
        <label key={k} className="flex items-center gap-2 text-sm">
          <input
            type="checkbox"
            checked={Boolean(v)}
            onChange={(e) => set(k, e.target.checked)}
          />
          {label}
        </label>
      );
    }
    if (k === "note" || k === "private_note" || k === "body_md") {
      return (
        <label key={k} className="block text-sm">
          <span className="text-[#5a6b7c]">
            {k === "body_md" ? "正文(Markdown)" : "随记"}
          </span>
          <textarea
            rows={k === "body_md" ? 6 : 3}
            className="mt-1 w-full rounded border border-[#33414f] bg-[#14181c] px-2 py-1"
            value={v == null ? "" : String(v)}
            onChange={(e) => set(k, e.target.value)}
          />
        </label>
      );
    }
    if (k === "dimensions") {
      const dims = (v as Record<string, string[]>) ?? {};
      return (
        <div key={k} className="space-y-1 text-sm">
          {DIMS.map((dim) => (
            <label key={dim} className="block">
              <span className="text-[#5a6b7c]">{dim}（逗号分隔） </span>
              <input
                className="mt-1 w-full rounded border border-[#33414f] bg-[#14181c] px-2 py-1"
                value={(dims[dim] ?? []).join(",")}
                onChange={(e) =>
                  set(k, {
                    ...dims,
                    [dim]: e.target.value
                      .split(/[,，]/)
                      .map((s) => s.trim())
                      .filter(Boolean),
                  })
                }
              />
            </label>
          ))}
        </div>
      );
    }
    if (k === "tags") {
      return (
        <label key={k} className="block text-sm">
          <span className="text-[#5a6b7c]">标签（逗号分隔） </span>
          <input
            className="mt-1 w-full rounded border border-[#33414f] bg-[#14181c] px-2 py-1"
            value={((v as string[]) ?? []).join(",")}
            onChange={(e) =>
              set(
                k,
                e.target.value
                  .split(/[,，]/)
                  .map((s) => s.trim())
                  .filter(Boolean),
              )
            }
          />
        </label>
      );
    }
    if (typeof v === "string") {
      return (
        <label key={k} className="block text-sm">
          <span className="text-[#5a6b7c]">{k} </span>
          <input
            className="mt-1 w-full rounded border border-[#33414f] bg-[#14181c] px-2 py-1"
            value={v}
            onChange={(e) => set(k, e.target.value)}
          />
        </label>
      );
    }
    return null; // 其余机读字段不进卡片
  };

  return (
    <div className="my-2 rounded-lg border border-[#ff8000]/60 bg-[#1f262e] p-3">
      <div className="mb-2 flex items-center justify-between">
        <span className="text-sm font-medium text-[#ff8000]">
          ✎ 待确认 · {pending.name}
        </span>
        <span className="text-xs text-[#5a6b7c]">可编辑后确认</span>
      </div>
      <div className="space-y-2">
        {Object.keys(values).map((k) => (
          <div key={k}>{field(k)}</div>
        ))}
      </div>
      <div className="mt-3 flex gap-2">
        <button
          className="rounded bg-[#00e054] px-3 py-1.5 text-sm font-medium text-[#0c1a10]"
          onClick={() => onConfirm(values)}
        >
          确认执行
        </button>
        <button
          className="rounded border border-[#456] px-3 py-1.5 text-sm"
          onClick={onReject}
        >
          拒绝
        </button>
      </div>
    </div>
  );
}

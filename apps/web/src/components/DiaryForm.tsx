import { useState } from "react";
import { StarsEditor } from "./Stars";
import TextWithPreview from "./TextWithPreview";

export type DiaryFormData = {
  watched_date: string;
  rating: number | null;
  in_theater: boolean;
  ticket_price_cents: number | null;
  note: string;
  dimensions: Record<string, string[]>;
  tags: string[];
};

const DIMS = ["地点", "场景", "同伴"]; // 「情绪」已迁移清除（P3）
const ALL_CATS = [...DIMS, "自由标签"];

const todayStr = () => {
  const nowD = new Date();
  return `${nowD.getFullYear()}-${String(nowD.getMonth() + 1).padStart(2, "0")}-${String(nowD.getDate()).padStart(2, "0")}`;
};

export function emptyDiaryForm(): DiaryFormData {
  return {
    watched_date: todayStr(),
    rating: null,
    in_theater: false,
    ticket_price_cents: null,
    note: "",
    dimensions: {},
    tags: [],
  };
}

/** 从 API args（模型的 JSON）构造表单初始值。
 *  缺失字段回退**当日/默认值**而非占位垃圾——update 场景应由调用方
 *  拉取条目回填（评审缺陷 2），此处只服务 add。 */
export function diaryFormFromArgs(args: Record<string, unknown>): DiaryFormData {
  const dims: Record<string, string[]> = {};
  if (args["dimensions"] && typeof args["dimensions"] === "object") {
    for (const [k, v] of Object.entries(args["dimensions"] as Record<string, unknown>)) {
      // 只接受三槽位（情绪已清除；AI 残留旧键会提交失败）
      if (Array.isArray(v) && (DIMS as string[]).includes(k)) dims[k] = v.map(String);
    }
  }
  return {
    watched_date: (args["watched_date"] as string) ?? todayStr(),
    rating: (args["rating"] as number) ?? null,
    in_theater: (args["in_theater"] as boolean) ?? false,
    ticket_price_cents: (args["ticket_price_cents"] as number) ?? null,
    note: (args["note"] as string) ?? "",
    dimensions: dims,
    tags: Array.isArray(args["tags"]) ? (args["tags"] as string[]) : [],
  };
}

type Taxonomy = {
  dimensions: Record<string, { name: string; used: number }[]>;
  tags: { name: string; used: number }[];
};

/** 「+ 添加标签」弹层（统一添加流）：选类别 → 填值（datalist=值池+历史）→ chip */
function AddTagPanel({
  taxonomy,
  existing,
  onAdd,
  onClose,
}: {
  taxonomy: Taxonomy | null;
  existing: Record<string, string[]>;
  onAdd: (cat: string, value: string) => void;
  onClose: () => void;
}) {
  const [cat, setCat] = useState(DIMS[0]);
  const [val, setVal] = useState("");
  const pool: { name: string; used: number }[] =
    cat === "自由标签"
      ? (taxonomy?.tags ?? [])
      : (taxonomy?.dimensions?.[cat] ?? []);
  const add = () => {
    const v = val.trim();
    if (!v) return;
    onAdd(cat, v);
    setVal("");
  };
  return (
    <div className="space-y-2 rounded border border-[#2c3440] bg-[#14181c] p-2">
      <div className="flex gap-2">
        <select
          className="w-28 rounded border border-[#33414f] bg-[#14181c] px-1 py-1 text-sm"
          value={cat}
          onChange={(e) => setCat(e.target.value)}
        >
          {ALL_CATS.map((c) => (
            <option key={c} value={c}>
              {c}
            </option>
          ))}
        </select>
        <input
          autoFocus
          list={`tag-dl-${cat}`}
          placeholder="输入或选择…"
          className="min-w-0 flex-1 rounded border border-[#33414f] bg-[#1b222b] px-2 py-1 text-sm"
          value={val}
          onChange={(e) => setVal(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && (e.preventDefault(), add())}
        />
        <datalist id={`tag-dl-${cat}`}>
          {pool
            .filter((p) => !(existing[cat] ?? (cat === "自由标签" ? existing["自由标签"] ?? [] : [])).includes(p.name))
            .map((p) => (
              <option key={p.name} value={p.name} />
            ))}
        </datalist>
        <button
          className="shrink-0 rounded bg-[#00e054] px-3 py-1 text-sm font-medium text-[#0c1a10]"
          onClick={add}
        >
          添加
        </button>
        <button className="shrink-0 text-sm text-[#8899aa]" onClick={onClose}>
          收起
        </button>
      </div>
      {pool.length > 0 && (
        <p className="text-[10px] leading-4 text-[#5a6b7c]">
          可选：{pool.slice(0, 12).map((p) => `${p.name}${p.used > 0 ? `×${p.used}` : ""}`).join("、")}
          {pool.length > 12 ? " …" : ""}
        </p>
      )}
    </div>
  );
}

/** 共享表单体（DiaryModal 与确认卡共用同一套字段渲染）。 */
export function DiaryForm({
  data,
  onChange,
  showAdvanced,
  onToggleAdvanced,
}: {
  data: DiaryFormData;
  onChange: (d: DiaryFormData) => void;
  showAdvanced: boolean;
  onToggleAdvanced: () => void;
}) {
  const [adding, setAdding] = useState(false);
  const [taxonomy, setTaxonomy] = useState<Taxonomy | null>(null);
  const set = (patch: Partial<DiaryFormData>) => onChange({ ...data, ...patch });

  const removeValue = (cat: string, v: string) => {
    if (cat === "自由标签") set({ tags: data.tags.filter((t) => t !== v) });
    else
      set({
        dimensions: {
          ...data.dimensions,
          [cat]: (data.dimensions[cat] ?? []).filter((x) => x !== v),
        },
      });
  };

  const addValue = (cat: string, v: string) => {
    if (cat === "自由标签") {
      if (!data.tags.includes(v)) set({ tags: [...data.tags, v] });
    } else {
      const cur = data.dimensions[cat] ?? [];
      if (!cur.includes(v))
        set({ dimensions: { ...data.dimensions, [cat]: [...cur, v] } });
    }
  };

  const openAdding = () => {
    setAdding(true);
    if (!taxonomy) {
      getTaxonomy().then(setTaxonomy).catch(() => {});
    }
  };

  const chipRow = (cat: string, values: string[]) => (
    <div className="flex flex-wrap items-center gap-1.5">
      <span className="w-16 shrink-0 text-xs text-[#5a6b7c]">{cat}</span>
      {values.map((v) => (
        <span
          key={v}
          className="flex items-center gap-1 rounded-full bg-[#2c3440] px-2 py-0.5 text-xs"
        >
          {v}
          <button
            className="text-[#8899aa] hover:text-[#ff8000]"
            onClick={() => removeValue(cat, v)}
          >
            ×
          </button>
        </span>
      ))}
      {values.length === 0 && <span className="text-xs text-[#3a4653]">（无）</span>}
    </div>
  );

  return (
    <div className="space-y-3">
      <StarsEditor value={data.rating} onChange={(v) => set({ rating: v })} />
      <input
        type="date"
        className="w-full rounded border border-[#33414f] bg-[#14181c] px-2 py-1.5 text-sm"
        value={data.watched_date}
        onChange={(e) => set({ watched_date: e.target.value })}
      />
      <label className="flex items-center gap-4 text-sm">
        <span className="flex items-center gap-1">
          <input type="checkbox" checked={data.in_theater} onChange={(e) => set({ in_theater: e.target.checked })} />
          影院观看
        </span>
        {data.in_theater && (
          <span className="flex items-center gap-1">
            票价 ¥
            <input
              type="number"
              className="w-20 rounded border border-[#33414f] bg-[#14181c] px-2 py-1"
              value={data.ticket_price_cents !== null ? data.ticket_price_cents / 100 : ""}
              onChange={(e) =>
                set({ ticket_price_cents: e.target.value === "" ? null : Math.round(Number(e.target.value) * 100) })
              }
            />
          </span>
        )}
      </label>
      <TextWithPreview
        value={data.note}
        onChange={(v) => set({ note: v })}
        rows={3}
        placeholder="随记（仅本地，支持 Markdown）"
      />
      <button className="text-xs text-[#40bcf4]" onClick={onToggleAdvanced}>
        {showAdvanced ? "收起 Advanced ▲" : "Advanced（维度与标签）▼"}
      </button>
      {showAdvanced && (
        <div className="space-y-2 rounded border border-[#2c3440] p-2">
          {DIMS.map((dim) => chipRow(dim, data.dimensions[dim] ?? []))}
          {chipRow("自由标签", data.tags)}
          {adding ? (
            <AddTagPanel
              taxonomy={taxonomy}
              existing={data.dimensions}
              onAdd={addValue}
              onClose={() => setAdding(false)}
            />
          ) : (
            <button className="text-xs text-[#40bcf4]" onClick={openAdding}>
              + 添加标签
            </button>
          )}
        </div>
      )}
    </div>
  );
}

async function getTaxonomy(): Promise<Taxonomy> {
  const r = await fetch("/api/taxonomy");
  if (!r.ok) throw new Error(`${r.status}`);
  return r.json();
}

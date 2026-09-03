import { StarsEditor } from "./Stars";

export type DiaryFormData = {
  watched_date: string;
  rating: number | null;
  in_theater: boolean;
  ticket_price_cents: number | null;
  note: string;
  dimensions: Record<string, string[]>;
  tags: string[];
};

const DIMS = ["地点", "同伴", "情绪", "场景"];

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
      if (Array.isArray(v)) dims[k] = v.map(String);
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
  const set = (patch: Partial<DiaryFormData>) => onChange({ ...data, ...patch });

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
      <textarea
        rows={3}
        placeholder="随记（仅本地）"
        className="w-full rounded border border-[#33414f] bg-[#14181c] px-2 py-1.5 text-sm"
        value={data.note}
        onChange={(e) => set({ note: e.target.value })}
      />
      <button className="text-xs text-[#40bcf4]" onClick={onToggleAdvanced}>
        {showAdvanced ? "收起 Advanced ▲" : "Advanced（维度与标签）▼"}
      </button>
      {showAdvanced && (
        <div className="space-y-2 rounded border border-[#2c3440] p-2">
          {DIMS.map((dim) => (
            <label key={dim} className="block text-sm">
              <span className="text-[#5a6b7c]">{dim}（逗号分隔，可新建） </span>
              <input
                className="mt-1 w-full rounded border border-[#33414f] bg-[#14181c] px-2 py-1"
                value={(data.dimensions[dim] ?? []).join(",")}
                onChange={(e) =>
                  set({
                    dimensions: {
                      ...data.dimensions,
                      [dim]: e.target.value.split(/[,，]/).map((s) => s.trim()).filter(Boolean),
                    },
                  })
                }
              />
            </label>
          ))}
          <label className="block text-sm">
            <span className="text-[#5a6b7c]">自由标签（逗号分隔） </span>
            <input
              className="mt-1 w-full rounded border border-[#33414f] bg-[#14181c] px-2 py-1"
              value={data.tags.join(",")}
              onChange={(e) => set({ tags: e.target.value.split(/[,，]/).map((s) => s.trim()).filter(Boolean) })}
            />
          </label>
        </div>
      )}
    </div>
  );
}

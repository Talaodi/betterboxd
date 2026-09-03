/** Diary 编辑弹窗：GET 回填条目现状 → DiaryForm 可编辑 → PUT 全字段（presence 语义）。
 *  终态评分改由署名日期（=观看日期）绑定，无覆盖警告概念。 */
import { useEffect, useState } from "react";
import { getJson, parseJsonArray, sendJson } from "../api";
import { DiaryForm, type DiaryFormData } from "./DiaryForm";

type EntryFull = {
  entry_id: string;
  watched_date: string;
  rating: number | null;
  in_theater: number;
  ticket_price_cents: number | null;
  private_note: string;
  tags: string;
  dimensions_flat: string;
};

function entryToForm(e: EntryFull): DiaryFormData {
  const dims: Record<string, string[]> = {};
  for (const d of parseJsonArray(e.dimensions_flat)) {
    const o = d as { dimension?: string; name?: string };
    if (o.dimension && o.name) {
      (dims[o.dimension] ??= []).push(o.name);
    }
  }
  return {
    watched_date: e.watched_date,
    rating: e.rating,
    in_theater: e.in_theater === 1,
    ticket_price_cents: e.ticket_price_cents,
    note: e.private_note,
    dimensions: dims,
    tags: parseJsonArray(e.tags).map(String),
  };
}

export default function EditDiaryModal({
  entryId,
  onClose,
  onSaved,
}: {
  entryId: string;
  onClose: () => void;
  onSaved: () => void;
}) {
  const [data, setData] = useState<DiaryFormData | null>(null);
  const [showAdvanced, setShowAdvanced] = useState(false);
  const [err, setErr] = useState("");

  useEffect(() => {
    getJson<{ entry: EntryFull }>(`/api/diary/${entryId}`)
      .then((d) => setData(entryToForm(d.entry)))
      .catch((e) => setErr(`回填失败: ${(e as Error).message}`));
  }, [entryId]);

  const submit = async () => {
    if (!data) return;
    try {
      await sendJson(`/api/diary/${entryId}`, "PUT", data);
      onSaved();
      onClose();
    } catch (e) {
      setErr((e as Error).message);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60">
      <div className="w-[460px] rounded-lg border border-[#33414f] bg-[#1b222b] p-4">
        <h2 className="mb-3 text-base font-medium">编辑观影记录</h2>
        {err && <p className="mb-2 text-sm text-[#ff8000]">{err}</p>}
        {data ? (
          <DiaryForm
            data={data}
            onChange={setData}
            showAdvanced={showAdvanced}
            onToggleAdvanced={() => setShowAdvanced(!showAdvanced)}
          />
        ) : (
          !err && <p className="text-sm text-[#5a6b7c]">正在回填条目…</p>
        )}
        <div className="mt-4 flex justify-end gap-2">
          <button className="rounded px-3 py-1.5 text-sm text-[#8899aa]" onClick={onClose}>
            取消
          </button>
          <button
            className="rounded bg-[#00e054] px-4 py-1.5 text-sm font-medium text-[#0c1a10] disabled:opacity-40"
            disabled={!data}
            onClick={submit}
          >
            保存
          </button>
        </div>
      </div>
    </div>
  );
}

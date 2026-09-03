/** 可编辑确认卡（评审缺陷 2/16 重写）：
 *  - manage_diary add    → DiaryForm（args 初始化）
 *  - manage_diary update → 按 entry_id 拉取当前条目回填表单，确认时只提交 diff
 *  - manage_reviews      → 专用面板（title/body/rating/署名日期 可编辑，不再是 DiaryForm）
 *  - 任意 delete         → 简单确认文案（删除语义不渲染编辑表单）
 *  - manage_saved_queries create → 名称可编辑 + SQL 只读 */
import { useEffect, useState } from "react";
import {
  DiaryForm,
  diaryFormFromArgs,
  emptyDiaryForm,
  type DiaryFormData,
} from "./DiaryForm";
import { getJson, parseJsonArray } from "../api";
import TextWithPreview from "./TextWithPreview";
import { StarsEditor } from "./Stars";

type Pending = {
  call_id: string;
  name: string;
  args: Record<string, unknown>;
  movieTitle?: string;
};

type EntryRow = {
  entry_id: string;
  watched_date: string;
  rating: number | null;
  in_theater: number;
  liked: number;
  ticket_price_cents: number | null;
  private_note: string;
  tags: string;
  dimensions_flat: string;
};

/** 条目行 → 表单数据（回填用）。 */
function diaryFormFromEntry(e: EntryRow): DiaryFormData {
  const dims: Record<string, string[]> = {};
  for (const d of parseJsonArray(e.dimensions_flat)) {
    const o = d as { dimension: string; name: string };
    // 情绪维度已迁移清除（P3）——旧数据残留键不进表单（提交会被 ensure_dims 拒绝）
    if (["地点", "场景", "同伴"].includes(o.dimension ?? "")) {
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

/** 表单 vs 条目 → 只含用户改过字段的 update patch（缺陷 2：不再污染部分更新）。 */
function diffDiary(form: DiaryFormData, loaded: EntryRow): Record<string, unknown> {
  const patch: Record<string, unknown> = {};
  if (form.watched_date !== loaded.watched_date) patch.watched_date = form.watched_date;
  if (form.rating !== loaded.rating) patch.rating = form.rating; // null = 清除
  if (form.in_theater !== (loaded.in_theater === 1)) patch.in_theater = form.in_theater;
  if (form.ticket_price_cents !== loaded.ticket_price_cents)
    patch.ticket_price_cents = form.ticket_price_cents; // null = 清除
  if (form.note !== loaded.private_note) patch.note = form.note;
  const dimsNow: Record<string, string[]> = {};
  for (const [k, v] of Object.entries(form.dimensions)) {
    if (v.length) dimsNow[k] = v;
  }
  const dimsLoaded = parseJsonArray(loaded.dimensions_flat).map(
    (d) => (d as { dimension: string; name: string }).name,
  );
  const dimsNowFlat = Object.values(dimsNow).flat();
  if (JSON.stringify(dimsNowFlat.slice().sort()) !== JSON.stringify(dimsLoaded.slice().sort()))
    patch.dimensions = dimsNow;
  const tagsNow = form.tags;
  const tagsLoaded = parseJsonArray(loaded.tags).map(String);
  if (JSON.stringify(tagsNow.slice().sort()) !== JSON.stringify(tagsLoaded.slice().sort()))
    patch.tags = tagsNow;
  return patch;
}

export default function ConfirmCard({
  pending,
  onConfirm,
  onReject,
}: {
  pending: Pending;
  onConfirm: (args: Record<string, unknown>) => void;
  onReject: () => void;
}) {
  const action = pending.args?.action as string | undefined;
  const isDiary = pending.name === "manage_diary";
  const isReview = pending.name === "manage_reviews";
  const isState = pending.name === "set_movie_state";
  const isList = pending.name === "manage_lists";
  const isListForm = isList && (action === "create" || action === "update");
  const isSqCreate =
    pending.name === "manage_saved_queries" && action === "create";
  const isDelete = action === "delete";
  const isDiaryUpdate = isDiary && action === "update";

  // manage_diary add：复用 DiaryForm
  const [diaryData, setDiaryData] = useState<DiaryFormData>(() =>
    isDiary && action === "add" ? diaryFormFromArgs(pending.args) : emptyDiaryForm(),
  );
  const [showAdvanced, setShowAdvanced] = useState(false);

  // manage_diary update：拉取当前条目回填（评审缺陷 2 第①层）
  const [loaded, setLoaded] = useState<EntryRow | null>(null);
  const [loadErr, setLoadErr] = useState("");
  useEffect(() => {
    if (!isDiaryUpdate) return;
    const entryId = String(pending.args?.entry_id ?? "");
    if (!entryId) {
      setLoadErr("缺少 entry_id，无法回填条目");
      return;
    }
    getJson<{ entry: EntryRow }>(`/api/diary/${entryId}`)
      .then((d) => {
        setLoaded(d.entry);
        const fd = diaryFormFromEntry(d.entry);
        // 叠加 AI 的修改意图（缺省键保持条目现值）——表单=现状+提议，用户可再编辑
        const a = pending.args ?? {};
        if (a.watched_date !== undefined) fd.watched_date = String(a.watched_date);
        if (a.rating !== undefined) fd.rating = (a.rating as number | null) ?? null;
        if (a.in_theater !== undefined) fd.in_theater = a.in_theater === true;
        if (a.ticket_price_cents !== undefined)
          fd.ticket_price_cents = (a.ticket_price_cents as number | null) ?? null;
        if (a.note !== undefined) fd.note = String(a.note);
        if (a.dimensions && typeof a.dimensions === "object") {
          const dims: Record<string, string[]> = { ...fd.dimensions };
          for (const [k, v] of Object.entries(a.dimensions as Record<string, unknown>)) {
            if (Array.isArray(v)) dims[k] = v.map(String);
          }
          fd.dimensions = dims;
        }
        if (Array.isArray(a.tags)) fd.tags = (a.tags as unknown[]).map(String);
        setDiaryData(fd);
      })
      .catch((e) => setLoadErr(`条目回填失败: ${(e as Error).message}`));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [pending.call_id]);

  // manage_reviews：专用可编辑面板状态（缺陷 16）
  const [revTitle, setRevTitle] = useState(() =>
    isReview ? String(pending.args?.title ?? "") : "",
  );
  const [revBody, setRevBody] = useState(() =>
    isReview ? String(pending.args?.body_md ?? "") : "",
  );
  const [revRating, setRevRating] = useState<number | null>(() =>
    isReview ? ((pending.args?.rating as number | null) ?? null) : null,
  );
  const [revSigDate, setRevSigDate] = useState(() =>
    isReview ? String(pending.args?.signature_date ?? "") : "",
  );

  // manage_saved_queries create：名称可编辑，SQL 只读
  const [sqName, setSqName] = useState(() =>
    isSqCreate ? String(pending.args?.name ?? "") : "",
  );

  // manage_lists create/update：名称/描述/ranked 可编辑
  const [listName, setListName] = useState(() =>
    isListForm ? String(pending.args?.name ?? "") : "",
  );
  const [listDesc, setListDesc] = useState(() =>
    isListForm ? String(pending.args?.description ?? "") : "",
  );
  const [listRanked, setListRanked] = useState(() =>
    isListForm ? pending.args?.ranked === true : false,
  );
  // update 回填现值（评审缺陷：rename-only 更新会把 description/ranked 重置）
  const [listLoaded, setListLoaded] = useState<{
    name: string;
    description: string | null;
    ranked: number;
  } | null>(null);
  const [listLoadErr, setListLoadErr] = useState("");
  useEffect(() => {
    if (!isListForm || action !== "update") return;
    const id = String(pending.args?.list_id ?? "");
    if (!id) {
      setListLoadErr("缺少 list_id，无法回填清单");
      return;
    }
    getJson<{ list: { name: string; description: string | null; ranked: number } }>(
      `/api/lists/${id}`,
    )
      .then((d) => {
        setListLoaded(d.list);
        // 表单 = 现状 + 提议（模型未提供的键保持清单现值）
        setListName((prev) => (pending.args?.name !== undefined ? prev : d.list.name));
        setListDesc((prev) =>
          pending.args?.description !== undefined ? prev : (d.list.description ?? ""),
        );
        setListRanked((prev) =>
          pending.args?.ranked !== undefined ? prev : d.list.ranked === 1,
        );
      })
      .catch((e) => setListLoadErr(`清单回填失败: ${(e as Error).message}`));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [pending.call_id]);

  const confirm = () => {
    if (isDelete) {
      onConfirm(pending.args);
    } else if (isDiary && action === "add") {
      onConfirm({ ...pending.args, ...diaryData, override_confirmed: true });
    } else if (isDiaryUpdate) {
      if (!loaded) return; // 未回填完成不允许确认
      const patch = diffDiary(diaryData, loaded);
      onConfirm({
        action: "update",
        entry_id: pending.args?.entry_id,
        ...patch,
        override_confirmed: true,
      });
    } else if (isReview) {
      onConfirm({
        ...pending.args,
        title: revTitle,
        body_md: revBody,
        rating: revRating,
        signature_date: revSigDate || null,
        override_confirmed: true,
      });
    } else if (isSqCreate) {
      onConfirm({ ...pending.args, name: sqName.trim() || "未命名统计" });
    } else if (isListForm) {
      if (action === "update" && listLoaded) {
        // presence 语义：只提交相对现值有变化的键（防确认卡重置未提及字段）
        const patch: Record<string, unknown> = {
          action: "update",
          list_id: pending.args?.list_id,
        };
        if (listName.trim() !== listLoaded.name) patch.name = listName.trim();
        if (listDesc !== (listLoaded.description ?? "")) patch.description = listDesc;
        if (listRanked !== (listLoaded.ranked === 1)) patch.ranked = listRanked;
        onConfirm(patch);
      } else {
        onConfirm({
          ...pending.args,
          name: listName.trim(),
          description: listDesc,
          ranked: listRanked,
        });
      }
    } else {
      onConfirm(pending.args);
    }
  };

  const reject = () => onReject();

  // ===== 删除：简单确认（缺陷 16：删除不渲染编辑表单） =====
  if (isDelete) {
    const what = isDiary
      ? "此观影记录"
      : isReview
        ? "此影评"
        : pending.name === "manage_saved_queries"
          ? `统计项目「${String(pending.args?.saved_query_id ?? "")}」`
          : isList
            ? `清单「${String(pending.args?.list_id ?? "")}」`
            : "此对象";
    return (
      <div className="my-2 rounded-lg border border-[#ff8000]/60 bg-[#1f262e] p-4">
        <p className="mb-3 text-sm font-medium text-[#ff8000]">✎ 待确认 · 删除</p>
        <p className="text-sm text-[#dfe7ef]">确认删除{what}？此操作不可撤销（账目保留）。</p>
        <div className="mt-3 flex gap-2">
          <button
            className="rounded bg-[#00e054] px-3 py-1.5 text-sm font-medium text-[#0c1a10]"
            onClick={confirm}
          >
            确认删除
          </button>
          <button className="rounded border border-[#456] px-3 py-1.5 text-sm" onClick={reject}>
            拒绝
          </button>
        </div>
      </div>
    );
  }

  // ===== manage_diary update：回填 + diff =====
  if (isDiaryUpdate) {
    return (
      <div className="my-2 rounded-lg border border-[#ff8000]/60 bg-[#1f262e] p-4">
        <div className="mb-3 flex items-center justify-between">
          <span className="text-sm font-medium text-[#ff8000]">✎ 待确认 · 修改观影记录</span>
          <span className="text-xs text-[#5a6b7c]">仅提交你改动的字段</span>
        </div>
        {loadErr && <p className="mb-2 text-sm text-[#ff8000]">{loadErr}</p>}
        {!loaded && !loadErr && <p className="text-sm text-[#5a6b7c]">正在回填当前条目…</p>}
        {loaded && (
          <DiaryForm
            data={diaryData}
            onChange={setDiaryData}
            showAdvanced={showAdvanced}
            onToggleAdvanced={() => setShowAdvanced(!showAdvanced)}
          />
        )}
        <div className="mt-3 flex gap-2">
          <button
            className="rounded bg-[#00e054] px-3 py-1.5 text-sm font-medium text-[#0c1a10] disabled:opacity-40"
            onClick={confirm}
            disabled={!loaded}
          >
            确认执行
          </button>
          <button className="rounded border border-[#456] px-3 py-1.5 text-sm" onClick={reject}>
            拒绝
          </button>
        </div>
      </div>
    );
  }

  // ===== manage_reviews：专用可编辑面板（缺陷 16） =====
  if (isReview) {
    return (
      <div className="my-2 rounded-lg border border-[#ff8000]/60 bg-[#1f262e] p-4">
        <div className="mb-3 flex items-center justify-between">
          <span className="text-sm font-medium text-[#ff8000]">
            ✎ 待确认 · {action === "add" ? "写影评" : "修改影评"}
          </span>
          <span className="text-xs text-[#5a6b7c]">可编辑后确认</span>
        </div>
        <div className="space-y-2">
          <input
            placeholder="标题"
            className="w-full rounded border border-[#33414f] bg-[#14181c] px-2 py-1.5 text-sm"
            value={revTitle}
            onChange={(e) => setRevTitle(e.target.value)}
          />
          <StarsEditor value={revRating} onChange={setRevRating} />
          <label className="flex items-center gap-2 text-sm">
            <span className="text-[#8899aa]">署名日期（终态评分绑定此日期，缺省=创建日期）</span>
            <input
              type="date"
              className="rounded border border-[#33414f] bg-[#14181c] px-2 py-1 text-sm"
              value={revSigDate}
              onChange={(e) => setRevSigDate(e.target.value)}
            />
          </label>
          <TextWithPreview
            value={revBody}
            onChange={setRevBody}
            rows={6}
            placeholder="正文（Markdown）"
          />
        </div>
        <div className="mt-3 flex gap-2">
          <button
            className="rounded bg-[#00e054] px-3 py-1.5 text-sm font-medium text-[#0c1a10]"
            onClick={confirm}
          >
            确认执行
          </button>
          <button className="rounded border border-[#456] px-3 py-1.5 text-sm" onClick={reject}>
            拒绝
          </button>
        </div>
      </div>
    );
  }

  // ===== manage_saved_queries create =====
  if (isSqCreate) {
    return (
      <div className="my-2 rounded-lg border border-[#ff8000]/60 bg-[#1f262e] p-4">
        <div className="mb-3 flex items-center justify-between">
          <span className="text-sm font-medium text-[#ff8000]">✎ 待确认 · 保存统计项目</span>
          <span className="text-xs text-[#5a6b7c]">名称可编辑，SQL 不可修改</span>
        </div>
        <div className="space-y-2">
          <label className="block text-sm">
            <span className="text-[#8899aa]">统计名称</span>
            <input
              className="mt-1 w-full rounded border border-[#33414f] bg-[#14181c] px-2 py-1.5 text-sm"
              value={sqName}
              onChange={(e) => setSqName(e.target.value)}
              autoFocus
            />
          </label>
          <div>
            <span className="text-[#8899aa] text-sm">SQL（只读）</span>
            <pre className="mt-1 max-h-[160px] overflow-auto rounded border border-[#2c3440] bg-[#14181c] p-2 text-xs text-[#40bcf4]">
              {String(pending.args?.sql ?? "")}
            </pre>
          </div>
        </div>
        <div className="mt-3 flex gap-2">
          <button
            className="rounded bg-[#00e054] px-3 py-1.5 text-sm font-medium text-[#0c1a10]"
            onClick={confirm}
          >
            确认保存
          </button>
          <button className="rounded border border-[#456] px-3 py-1.5 text-sm" onClick={reject}>
            拒绝
          </button>
        </div>
      </div>
    );
  }

  // ===== manage_lists create/update：可编辑表单 =====
  if (isListForm) {
    return (
      <div className="my-2 rounded-lg border border-[#ff8000]/60 bg-[#1f262e] p-4">
        <div className="mb-3 flex items-center justify-between">
          <span className="text-sm font-medium text-[#ff8000]">
            ✎ 待确认 · {action === "create" ? "新建清单" : "修改清单"}
          </span>
          <span className="text-xs text-[#5a6b7c]">
            {action === "update" ? "仅提交你改动的字段" : "可编辑后确认"}
          </span>
        </div>
        {listLoadErr && <p className="mb-2 text-sm text-[#ff8000]">{listLoadErr}</p>}
        {action === "update" && !listLoaded && !listLoadErr && (
          <p className="mb-2 text-sm text-[#5a6b7c]">正在回填清单现状…</p>
        )}
        <div className="space-y-2">
          <input
            placeholder="清单名"
            className="w-full rounded border border-[#33414f] bg-[#14181c] px-2 py-1.5 text-sm"
            value={listName}
            onChange={(e) => setListName(e.target.value)}
          />
          <textarea
            rows={2}
            placeholder="描述（可选）"
            className="w-full rounded border border-[#33414f] bg-[#14181c] px-2 py-1.5 text-sm"
            value={listDesc}
            onChange={(e) => setListDesc(e.target.value)}
          />
          <label className="flex items-center gap-1 text-sm">
            <input
              type="checkbox"
              checked={listRanked}
              onChange={(e) => setListRanked(e.target.checked)}
            />
            排名清单（Rank #N）
          </label>
        </div>
        <div className="mt-3 flex gap-2">
          <button
            className="rounded bg-[#00e054] px-3 py-1.5 text-sm font-medium text-[#0c1a10] disabled:opacity-40"
            onClick={confirm}
            disabled={action === "update" && !listLoaded}
          >
            确认执行
          </button>
          <button className="rounded border border-[#456] px-3 py-1.5 text-sm" onClick={reject}>
            拒绝
          </button>
        </div>
      </div>
    );
  }

  // ===== manage_lists add_item/remove_item：摘要确认 =====
  if (isList) {
    const summary =
      action === "add_item"
        ? `把影片 ${String(pending.args?.movie_id ?? "")} 加入清单 ${String(pending.args?.list_id ?? "")}`
        : `把影片 ${String(pending.args?.movie_id ?? "")} 移出清单 ${String(pending.args?.list_id ?? "")}`;
    return (
      <div className="my-2 rounded-lg border border-[#ff8000]/60 bg-[#1f262e] p-4">
        <p className="mb-3 text-sm font-medium text-[#ff8000]">✎ 待确认 · 修改清单归属</p>
        <p className="text-sm text-[#dfe7ef]">{summary}</p>
        <div className="mt-3 flex gap-2">
          <button
            className="rounded bg-[#00e054] px-3 py-1.5 text-sm font-medium text-[#0c1a10]"
            onClick={confirm}
          >
            确认执行
          </button>
          <button className="rounded border border-[#456] px-3 py-1.5 text-sm" onClick={reject}>
            拒绝
          </button>
        </div>
      </div>
    );
  }

  // ===== set_movie_state：仅想看切换（架构收敛后唯一字段） =====
  if (isState) {
    const on = pending.args?.in_watchlist === true;
    return (
      <div className="my-2 rounded-lg border border-[#ff8000]/60 bg-[#1f262e] p-4">
        <p className="mb-3 text-sm font-medium text-[#ff8000]">✎ 待确认 · 修改想看状态</p>
        <p className="text-sm text-[#dfe7ef]">
          {String(pending.args?.movie_id ?? "")} → 想看：{on ? "加入想看" : "移出想看"}
        </p>
        <div className="mt-3 flex gap-2">
          <button
            className="rounded bg-[#00e054] px-3 py-1.5 text-sm font-medium text-[#0c1a10]"
            onClick={confirm}
          >
            确认执行
          </button>
          <button className="rounded border border-[#456] px-3 py-1.5 text-sm" onClick={reject}>
            拒绝
          </button>
        </div>
      </div>
    );
  }

  // ===== 默认（manage_diary add 等）：DiaryForm =====
  return (
    <div className="my-2 rounded-lg border border-[#ff8000]/60 bg-[#1f262e] p-4">
      <div className="mb-3 flex items-center justify-between">
        <span className="text-sm font-medium text-[#ff8000]">
          ✎ 待确认 {isDiary ? "· 记一笔" : ""}
        </span>
        <span className="text-xs text-[#5a6b7c]">可编辑后确认</span>
      </div>
      {isDiary && (
        <DiaryForm
          data={diaryData}
          onChange={setDiaryData}
          showAdvanced={showAdvanced}
          onToggleAdvanced={() => setShowAdvanced(!showAdvanced)}
        />
      )}
      {!isDiary && (
        <pre className="text-xs text-[#8899aa]">{JSON.stringify(pending.args, null, 2)}</pre>
      )}
      <div className="mt-3 flex gap-2">
        <button
          className="rounded bg-[#00e054] px-3 py-1.5 text-sm font-medium text-[#0c1a10]"
          onClick={confirm}
        >
          确认执行
        </button>
        <button className="rounded border border-[#456] px-3 py-1.5 text-sm" onClick={reject}>
          拒绝
        </button>
      </div>
    </div>
  );
}

import { useState } from "react";
import {
  DiaryForm,
  diaryFormFromArgs,
  emptyDiaryForm,
  type DiaryFormData,
} from "./DiaryForm";

type Pending = {
  call_id: string;
  name: string;
  args: Record<string, unknown>;
  movieTitle?: string;
};

/** 可编辑确认卡：复用 DiaryForm 表单体（视觉与"记一笔"一致），
 *  底部为确认执行 / 拒绝按钮。Manage reviews / set_movie_state 使用
 *  简化的字段子集。 */
export default function ConfirmCard({
  pending,
  onConfirm,
  onReject,
}: {
  pending: Pending;
  onConfirm: (args: Record<string, unknown>) => void;
  onReject: () => void;
}) {
  const isDiary = pending.name === "manage_diary";
  const isReview = pending.name === "manage_reviews";
  const isState = pending.name === "set_movie_state";
  const isSqCreate =
    pending.name === "manage_saved_queries" && pending.args?.action === "create";
  const isSqDelete =
    pending.name === "manage_saved_queries" && pending.args?.action === "delete";

  // manage_diary：复用 DiaryForm
  const [diaryData, setDiaryData] = useState<DiaryFormData>(() =>
    isDiary ? diaryFormFromArgs(pending.args) : emptyDiaryForm(),
  );
  const [showAdvanced, setShowAdvanced] = useState(false);
  // manage_saved_queries create：名称可编辑，SQL 只读
  const [sqName, setSqName] = useState(() =>
    isSqCreate ? String(pending.args?.name ?? "") : "",
  );

  const confirm = () => {
    if (isDiary) {
      onConfirm({
        ...pending.args,
        ...diaryData,
        override_confirmed: true,
      });
    } else if (isSqCreate) {
      onConfirm({ ...pending.args, name: sqName.trim() || "未命名统计" });
    } else {
      onConfirm(pending.args);
    }
  };

  const reject = () => onReject();

  // manage_saved_queries 面板：create 名称+SQL / delete 提示
  if (isSqCreate || isSqDelete) {
    return (
      <div className="my-2 rounded-lg border border-[#ff8000]/60 bg-[#1f262e] p-4">
        <div className="mb-3 flex items-center justify-between">
          <span className="text-sm font-medium text-[#ff8000]">
            ✎ 待确认 · {isSqCreate ? "保存统计项目" : "删除统计项目"}
          </span>
          {isSqCreate && <span className="text-xs text-[#5a6b7c]">名称可编辑，SQL 不可修改</span>}
        </div>
        {isSqCreate ? (
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
        ) : (
          <p className="text-sm text-[#dfe7ef]">
            确认删除统计项目「{String(pending.args?.saved_query_id ?? "")}」？此操作不可撤销。
          </p>
        )}
        <div className="mt-3 flex gap-2">
          <button
            className="rounded bg-[#00e054] px-3 py-1.5 text-sm font-medium text-[#0c1a10]"
            onClick={confirm}
          >
            {isSqCreate ? "确认保存" : "确认删除"}
          </button>
          <button className="rounded border border-[#456] px-3 py-1.5 text-sm" onClick={onReject}>
            拒绝
          </button>
        </div>
      </div>
    );
  }

  // set_movie_state 简化面板
  if (isState) {
    const mr = pending.args["my_rating"];
    const lk = pending.args["liked"];
    const iw = pending.args["in_watchlist"];
    return (
      <div className="my-2 rounded-lg border border-[#ff8000]/60 bg-[#1f262e] p-3">
        <p className="mb-2 text-sm font-medium text-[#ff8000]">✎ 待确认 · 修改影片状态</p>
        <div className="space-y-1 text-sm text-[#dfe7ef]">
          {mr !== undefined && <p>最终评分 → {String(mr)}</p>}
          {lk !== undefined && <p>喜欢 → {String(lk)}</p>}
          {iw !== undefined && <p>想看 → {String(iw)}</p>}
        </div>
        <div className="mt-3 flex gap-2">
          <button className="rounded bg-[#00e054] px-3 py-1.5 text-sm font-medium text-[#0c1a10]"
            onClick={() => onConfirm(pending.args)}>
            确认执行
          </button>
          <button className="rounded border border-[#456] px-3 py-1.5 text-sm" onClick={onReject}>
            拒绝
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="my-2 rounded-lg border border-[#ff8000]/60 bg-[#1f262e] p-4">
      <div className="mb-3 flex items-center justify-between">
        <span className="text-sm font-medium text-[#ff8000]">
          ✎ 待确认 {isDiary ? "· 记一笔" : isReview ? "· 影评" : ""}
        </span>
        <span className="text-xs text-[#5a6b7c]">可编辑后确认</span>
      </div>
      {(isDiary || isReview) && (
        <DiaryForm
          data={diaryData}
          onChange={setDiaryData}
          showAdvanced={showAdvanced}
          onToggleAdvanced={() => setShowAdvanced(!showAdvanced)}
        />
      )}
      {!isDiary && !isReview && (
        <pre className="text-xs text-[#8899aa]">{JSON.stringify(pending.args, null, 2)}</pre>
      )}
      <div className="mt-3 flex gap-2">
        <button
          className="rounded bg-[#00e054] px-3 py-1.5 text-sm font-medium text-[#0c1a10]"
          onClick={confirm}
        >
          确认执行
        </button>
        <button
          className="rounded border border-[#456] px-3 py-1.5 text-sm"
          onClick={reject}
        >
          拒绝
        </button>
      </div>
    </div>
  );
}

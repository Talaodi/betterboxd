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

  // manage_diary：复用 DiaryForm
  const [diaryData, setDiaryData] = useState<DiaryFormData>(() =>
    isDiary ? diaryFormFromArgs(pending.args) : emptyDiaryForm(),
  );
  const [showAdvanced, setShowAdvanced] = useState(false);

  const confirm = () => {
    if (isDiary) {
      onConfirm({
        ...pending.args,
        ...diaryData,
        override_confirmed: true,
      });
    } else {
      onConfirm(pending.args);
    }
  };

  const reject = () => onReject();

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

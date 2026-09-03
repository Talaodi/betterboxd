import { useEffect, useState, type ReactNode } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { getJson, parseJsonArray, sendJson, type DiaryRow, type LogRow, type MovieRow } from "../api";
import Poster from "../components/Poster";
import DiaryEntryRow from "../components/DiaryEntryRow";
import ReviewPosterCard from "../components/ReviewPosterCard";
import EditDiaryModal from "../components/EditDiaryModal";
import EditReviewModal from "../components/EditReviewModal";
import ChatListRow from "../components/ChatListRow";
import { StarsDisplay, StarsEditor } from "../components/Stars";
import { DiaryForm, emptyDiaryForm, type DiaryFormData } from "../components/DiaryForm";
import TextWithPreview from "../components/TextWithPreview";

type Detail = {
  movie: MovieRow;
  logs: LogRow[];
  lists: {
    list_id: string;
    name: string;
    rank: number | null;
    ranked: number;
    updated_at: number;
    item_count: number;
    cover_movie_id: number | null;
  }[];
};

type ListMetaLite = {
  id: string;
  name: string;
  source: string;
  ranked: number;
};

export default function FilmDetail() {
  const { id } = useParams<{ id: string }>();
  const navigate = useNavigate();
  const [detail, setDetail] = useState<Detail | null>(null);
  const [showDiary, setShowDiary] = useState(false);
  const [showReview, setShowReview] = useState(false);
  const [editDiaryId, setEditDiaryId] = useState<string | null>(null);
  const [editReviewId, setEditReviewId] = useState<string | null>(null);
  // List 归属区：加入清单下拉 + 新建内联
  const [showListPicker, setShowListPicker] = useState(false);
  const [allLists, setAllLists] = useState<ListMetaLite[]>([]);
  const [newListName, setNewListName] = useState("");
  // Log 三板块折叠态（点击板块标题收起内容仅剩标题条）
  const [collapsed, setCollapsed] = useState<Record<string, boolean>>({});

  const load = () => {
    getJson<Detail>(`/api/movie/${id}`)
      .then(setDetail)
      .catch((e) => alert(`加载失败: ${e.message}`));
  };
  useEffect(load, [id]);

  const setState = async (patch: Record<string, unknown>) => {
    try {
      await sendJson(`/api/movie/${id}/state`, "POST", patch);
      load();
    } catch (e) {
      alert(`状态修改失败: ${(e as Error).message}`);
    }
  };

  const openListPicker = async () => {
    setShowListPicker((v) => !v);
    if (!showListPicker) {
      try {
        const d = await getJson<{ lists: ListMetaLite[] }>("/api/lists");
        setAllLists(d.lists);
      } catch (e) {
        alert(`清单加载失败: ${(e as Error).message}`);
      }
    }
  };

  const addToList = async (listId: string) => {
    try {
      await sendJson(`/api/lists/${listId}/items`, "POST", { movie_id: Number(id) });
      setShowListPicker(false);
      load();
    } catch (e) {
      alert(`加入失败: ${(e as Error).message}`);
    }
  };

  const createAndAdd = async () => {
    const name = newListName.trim();
    if (!name) return;
    try {
      const out = await sendJson<{ list_id: string }>("/api/lists", "POST", { name });
      setNewListName("");
      await sendJson(`/api/lists/${out.list_id}/items`, "POST", { movie_id: Number(id) });
      setShowListPicker(false);
      load();
    } catch (e) {
      alert(`新建失败: ${(e as Error).message}`);
    }
  };

  const delEntry = async (entryId: string) => {
    if (!window.confirm("删除这条观影记录？（Action 账目会保留并回退状态）")) return;
    try {
      await sendJson(`/api/diary/${entryId}`, "DELETE");
      load();
    } catch (e) {
      alert(`删除失败: ${(e as Error).message}`);
    }
  };

  if (!detail) return <p className="p-6 text-[#5a6b7c]">加载中…</p>;
  const m = detail.movie;
  const directors = parseJsonArray(m.directors).join(" / ");
  const genres = parseJsonArray(m.genres);
  const logs = detail.logs;
  const diaryLogs = logs.filter((l) => l.kind === "watch");
  const reviewLogs = logs.filter((l) => l.kind === "review");
  const chatLogs = logs.filter((l) => l.kind === "chat");

  return (
    <div className="mx-auto max-w-[1100px] px-6 pt-6">
      {/* LB 式三栏：海报+动作面板 | 标题与信息 */}
      <div className="flex gap-8">
        {/* 左：海报 */}
        <div className="w-[210px] shrink-0">
          <Poster tmdbId={m.tmdb_id} title={m.title_main} size="detail" className="shadow-xl" />
        </div>

        {/* 中：标题与信息（LB 式排版） */}
        <div className="min-w-0 flex-1">
          <h1 className="text-4xl font-bold leading-tight">
            {m.title_main}
            {m.title_sub && m.title_sub !== m.title_main && (
              <span className="ml-3 font-normal italic text-[#5a6b7c]">
                ({m.title_sub})
              </span>
            )}
          </h1>
          <p className="mt-3 text-sm text-[#8899aa]">
            {[
              m.year,
              directors || null,
              m.runtime ? `${m.runtime} 分钟` : null,
              genres.length > 0 ? genres.join(" / ") : null,
            ].filter(Boolean).join(" · ")}
          </p>
          {m.tagline && (
            <p className="mt-3 text-xs uppercase tracking-widest text-[#8899aa]">{m.tagline}</p>
          )}
          <p className="mt-4 text-base leading-relaxed font-light text-[#c8d2dc]">
            {m.overview}
          </p>
          <p className="mt-3 text-xs text-[#5a6b7c]">
            简介来自 TMDB
          </p>
          {m.tmdb_rating !== null && m.tmdb_rating !== undefined && (
            <p className="mt-2 text-xs text-[#5a6b7c]" title={`${m.tmdb_votes} 人评分`}>
              TMDB {m.tmdb_rating.toFixed(1)}
            </p>
          )}
        </div>

        {/* 右：动作面板（LB 式竖排；评分/喜欢只读——唯一断言源 = Diary/Review） */}
        <div className="w-[210px] shrink-0 rounded-lg border border-[#33414f] bg-[#1b222b] p-4">
          <div className="flex justify-around pb-3">
            <button
              className={"flex flex-col items-center text-xs " +
                (m.in_watchlist ? "text-[#ff8000]" : "text-[#8899aa]")}
              onClick={() => setState({ in_watchlist: !m.in_watchlist })}
            >
              <span className="text-xl">🔖</span>
              想看
            </button>
            <button
              className={"flex flex-col items-center text-xs " +
                (m.liked ? "text-[#00e054]" : m.watched ? "text-[#8899aa]" : "text-[#3a4653]")}
              onClick={() => m.watched && setState({ liked: !m.liked })}
              disabled={!m.watched}
              title={m.watched ? (m.liked ? "取消喜欢" : "喜欢") : "先记一笔或写影评才能点喜欢"}
            >
              <span className="text-xl">{m.liked ? "♥" : "♡"}</span>
              喜欢
            </button>
          </div>
          <div className="border-t border-[#2c3440] pt-3">
            <p className="mb-1 text-center text-xs text-[#8899aa]">Rate</p>
            <div className="flex justify-center">
              <StarsDisplay value={m.my_rating} />
            </div>
            <p className="mt-1 text-center text-[10px] leading-4 text-[#5a6b7c]">
              评分绑定署名日期最靠后的有分记录
            </p>
          </div>
          <div className="mt-3 space-y-2 border-t border-[#2c3440] pt-3">
            <button
              className="block w-full rounded bg-[#00e054] px-3 py-1.5 text-center text-sm font-medium text-[#0c1a10]"
              onClick={() => setShowDiary(true)}
            >
              + 记一笔
            </button>
            <button
              className="block w-full rounded border border-[#40bcf4] px-3 py-1.5 text-center text-sm text-[#40bcf4]"
              onClick={() => setShowReview(true)}
            >
              + 写影评
            </button>
            <button
              className="block w-full rounded border border-[#456] px-3 py-1.5 text-center text-sm text-[#8899aa] hover:text-white"
              onClick={() => navigate(`/chats?new_movie=${m.tmdb_id}`)}
            >
              💬 讨论
            </button>
          </div>
        </div>
      </div>

      {/* Log 三板块竖排：Diary → Reviews → Chats，各板块内时间倒序，标题可折叠 */}
      <div className="mt-8 space-y-6">
        <LogSection
          label="DIARY"
          count={diaryLogs.length}
          toggle={() => setCollapsed((c) => ({ ...c, diary: !c.diary }))}
          isCollapsed={!!collapsed.diary}
        >
          <div className="border-t border-[#2c3440]">
            {diaryLogs.map((l, ri) => {
              const d = l.diary as unknown as DiaryRow | undefined;
              if (!d) return null;
              const entry: DiaryRow = { ...d, entry_id: l.id };
              const prev = diaryLogs[ri - 1];
              const firstOfMonth = !prev || prev.at.slice(0, 7) !== l.at.slice(0, 7);
              return (
                <DiaryEntryRow
                  key={l.kind + l.id}
                  entry={entry}
                  firstOfMonth={firstOfMonth}
                  onDelete={delEntry}
                  onEdit={setEditDiaryId}
                />
              );
            })}
          </div>
        </LogSection>
        <LogSection
          label="REVIEWS"
          count={reviewLogs.length}
          toggle={() => setCollapsed((c) => ({ ...c, reviews: !c.reviews }))}
          isCollapsed={!!collapsed.reviews}
        >
          <div className="space-y-4">
            {reviewLogs.map((l) =>
              l.review ? (
                <ReviewPosterCard
                  key={l.kind + l.id}
                  review={l.review as never}
                  movieId={m.tmdb_id}
                  movieTitle={m.title_main}
                  titleSub={m.title_sub || null}
                  onEdit={() => setEditReviewId(String(l.id))}
                />
              ) : null,
            )}
          </div>
        </LogSection>
        <LogSection
          label="CHATS"
          count={chatLogs.length}
          toggle={() => setCollapsed((c) => ({ ...c, chats: !c.chats }))}
          isCollapsed={!!collapsed.chats}
        >
          <div>
            {chatLogs.map((l) => (
              <ChatListRow
                key={l.kind + l.id}
                sessionId={l.id}
                title={l.brief}
                subtitle={l.at}
                tmdbId={m.tmdb_id}
                movieTitle={m.title_main}
                onClick={() => navigate(`/chats?open=${l.id}`)}
              />
            ))}
          </div>
        </LogSection>
      </div>
      {/* LISTS 板块（report v2：排在 CHATS 后，复用 Lists 列表卡样式） */}
      <div className="mt-8">
        <p className="mb-1.5 px-2 text-xs font-medium tracking-widest text-[#5a6b7c]">LISTS</p>
        <div className="border-t border-[#2c3440]">
          {detail.lists.map((l) => (
            <div
              key={l.list_id}
              className="flex cursor-pointer items-stretch gap-3 border-b border-[#1e2630] px-2 py-2 hover:bg-[#1b222b]"
              onClick={() => navigate(`/lists/${l.list_id}`)}
            >
              <div className="w-14 shrink-0 self-center text-right">
                <span
                  className={
                    "text-sm font-bold " +
                    (l.ranked && l.rank ? "text-[#ff8000]" : "text-[#8899aa]")
                  }
                >
                  {l.ranked && l.rank ? `No.${l.rank}` : "In"}
                </span>
              </div>
              <div className="w-[48px] shrink-0 self-stretch bg-[#14181c]">
                {l.cover_movie_id && (
                  <Poster
                    tmdbId={l.cover_movie_id}
                    title=""
                    size="grid"
                    className="h-full! w-[48px]! rounded object-cover"
                  />
                )}
              </div>
              <div className="flex min-w-0 flex-1 flex-col justify-center">
                <h3 className="truncate text-base font-semibold">{l.name}</h3>
                <p className="text-xs text-[#5a6b7c]">
                  {new Date(l.updated_at * 1000).toISOString().slice(0, 10).replace(/-/g, "/")} ·{" "}
                  {l.ranked === 1 ? "Ranked" : "Unranked"} · {l.item_count} 部
                </p>
              </div>
            </div>
          ))}
          {detail.lists.length === 0 && (
            <p className="px-2 py-2 text-sm text-[#5a6b7c]">尚未加入任何清单</p>
          )}
        </div>
        <span className="relative mt-2 inline-block">
          <button
            className="rounded border border-[#456] px-2 py-1 text-xs text-[#8899aa] hover:text-white"
            onClick={openListPicker}
          >
            + 加入 List
          </button>
          {showListPicker && (
            <div className="absolute left-0 top-8 z-40 w-60 rounded-lg border border-[#33414f] bg-[#1b222b] p-2 shadow-xl">
              {allLists.map((l) => {
                const joined = detail.lists.some((x) => x.list_id === l.id);
                return (
                  <button
                    key={l.id}
                    className="flex w-full items-center justify-between rounded px-2 py-1.5 text-left text-[#c8d2dc] hover:bg-[#2c3440] disabled:opacity-40"
                    disabled={joined || l.source === "letterboxd"}
                    onClick={() => addToList(l.id)}
                  >
                    <span className="truncate">
                      {l.name}
                      {l.ranked === 1 && (
                        <span className="ml-1 text-[10px] text-[#5a6b7c]">排名</span>
                      )}
                    </span>
                    {joined && <span className="text-[#00e054]">✓</span>}
                  </button>
                );
              })}
              <div className="mt-2 flex gap-1 border-t border-[#2c3440] pt-2">
                <input
                  placeholder="新清单名…"
                  className="min-w-0 flex-1 rounded border border-[#33414f] bg-[#14181c] px-2 py-1 text-xs"
                  value={newListName}
                  onChange={(e) => setNewListName(e.target.value)}
                  onKeyDown={(e) => e.key === "Enter" && createAndAdd()}
                />
                <button
                  className="rounded bg-[#00e054] px-2 py-1 text-xs font-medium text-[#0c1a10]"
                  onClick={createAndAdd}
                >
                  建
                </button>
              </div>
            </div>
          )}
        </span>
      </div>
      {showDiary && <DiaryModal movieId={m.tmdb_id} onClose={() => { setShowDiary(false); load(); }} onSaved={() => { setShowDiary(false); load(); }} />}
      {showReview && <ReviewModal movieId={m.tmdb_id} onClose={() => { setShowReview(false); load(); }} onSaved={() => { setShowReview(false); load(); }} />}
      {editDiaryId && (
        <EditDiaryModal entryId={editDiaryId} onClose={() => setEditDiaryId(null)} onSaved={load} />
      )}
      {editReviewId && (
        <EditReviewModal reviewId={editReviewId} onClose={() => setEditReviewId(null)} onSaved={load} />
      )}
    </div>
  );
}

// ============ Log 三板块（可折叠）============
function LogSection({
  label,
  count,
  children,
  toggle,
  isCollapsed,
}: {
  label: string;
  count: number;
  children: ReactNode;
  toggle: () => void;
  isCollapsed: boolean;
}) {
  return (
    <div>
      <button
        className="mb-1.5 flex w-full items-center gap-2 px-2 text-left"
        onClick={toggle}
      >
        <span className="text-xs font-medium tracking-widest text-[#5a6b7c]">{label}</span>
        <span className="text-xs text-[#40bcf4]">{count}</span>
        <span className="ml-auto text-xs text-[#5a6b7c]">
          {isCollapsed ? "展开 ▼" : "收起 ▲"}
        </span>
      </button>
      {/* 空板块不渲染内容（无竖线），与收起后一致 */}
      {!isCollapsed && count > 0 && children}
    </div>
  );
}

// ============ 弹出卡：添加 Diary（复用共享 DiaryForm：标签添加流 + Markdown 预览）============
function DiaryModal({
  movieId,
  onClose,
  onSaved,
}: {
  movieId: number;
  onClose: () => void;
  onSaved: () => void;
}) {
  const [data, setData] = useState<DiaryFormData>(emptyDiaryForm());
  const [showAdvanced, setShowAdvanced] = useState(false);
  const [err, setErr] = useState("");

  const submit = async () => {
    try {
      await sendJson("/api/diary", "POST", {
        action: "add",
        movie_id: movieId,
        ...data,
      });
      onSaved();
      onClose();
    } catch (e) {
      setErr((e as Error).message);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60">
      <div className="w-[460px] rounded-lg border border-[#33414f] bg-[#1b222b] p-4">
        <h2 className="mb-3 text-base font-medium">记一笔</h2>
        {err && <p className="mb-2 text-sm text-[#ff8000]">{err}</p>}
        <DiaryForm
          data={data}
          onChange={setData}
          showAdvanced={showAdvanced}
          onToggleAdvanced={() => setShowAdvanced(!showAdvanced)}
        />
        <div className="mt-4 flex justify-end gap-2">
          <button className="rounded px-3 py-1.5 text-sm text-[#8899aa]" onClick={onClose}>
            取消
          </button>
          <button
            className="rounded bg-[#00e054] px-4 py-1.5 text-sm font-medium text-[#0c1a10]"
            onClick={submit}
          >
            保存
          </button>
        </div>
      </div>
    </div>
  );
}

// ============ 弹出卡：添加 Review ============
function ReviewModal({
  movieId,
  onClose,
  onSaved,
}: {
  movieId: number;
  onClose: () => void;
  onSaved: () => void;
}) {
  const [title, setTitle] = useState("");
  const [body, setBody] = useState("");
  const [rating, setRating] = useState<number | null>(null);
  const [sigDate, setSigDate] = useState("");
  const [err, setErr] = useState("");

  const submit = async () => {
    try {
      await sendJson("/api/reviews", "POST", {
        action: "add",
        movie_id: movieId,
        title,
        body_md: body,
        rating,
        signature_date: sigDate || null,
      });
      onSaved();
      onClose();
    } catch (e) {
      setErr((e as Error).message);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60">
      <div className="w-[560px] rounded-lg border border-[#33414f] bg-[#1b222b] p-4">
        <h2 className="mb-3 text-base font-medium">写影评</h2>
        {err && <p className="mb-2 text-sm text-[#ff8000]">{err}</p>}
        <div className="space-y-3">
          <input
            placeholder="标题"
            className="w-full rounded border border-[#33414f] bg-[#14181c] px-2 py-1.5 text-sm"
            value={title}
            onChange={(e) => setTitle(e.target.value)}
          />
          <StarsEditor value={rating} onChange={setRating} />
          <label className="flex items-center gap-2 text-sm">
            <span className="text-[#8899aa]">署名日期（缺省=创建日期）</span>
            <input
              type="date"
              className="rounded border border-[#33414f] bg-[#14181c] px-2 py-1 text-sm"
              value={sigDate}
              onChange={(e) => setSigDate(e.target.value)}
            />
          </label>
          <TextWithPreview
            value={body}
            onChange={setBody}
            rows={8}
            placeholder="正文（Markdown）—— 公开长评，可对外导出"
          />
        </div>
        <div className="mt-4 flex justify-end gap-2">
          <button className="rounded px-3 py-1.5 text-sm text-[#8899aa]" onClick={onClose}>
            取消
          </button>
          <button
            className="rounded bg-[#00e054] px-4 py-1.5 text-sm font-medium text-[#0c1a10]"
            onClick={submit}
          >
            保存
          </button>
        </div>
      </div>
    </div>
  );
}

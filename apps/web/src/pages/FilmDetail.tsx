import { useEffect, useMemo, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { getJson, parseJsonArray, posterUrl, sendJson, type LogRow, type MovieRow } from "../api";
import Poster from "../components/Poster";
import { StarsEditor } from "../components/Stars";

type Detail = {
  movie: MovieRow;
  logs: LogRow[];
  lists: { list_id: string; name: string; rank: number | null; ranked: number }[];
};

const KIND_BADGE: Record<string, { label: string; color: string }> = {
  watch: { label: "看", color: "#00e054" },
  review: { label: "评", color: "#40bcf4" },
  chat: { label: "聊", color: "#8899aa" },
};

export default function FilmDetail() {
  const { id } = useParams<{ id: string }>();
  const navigate = useNavigate();
  const [detail, setDetail] = useState<Detail | null>(null);
  const [tab, setTab] = useState("all");
  const [showDiary, setShowDiary] = useState(false);
  const [showReview, setShowReview] = useState(false);

  const load = () => {
    getJson<Detail>(`/api/movie/${id}`)
      .then(setDetail)
      .catch((e) => alert(`加载失败: ${e.message}`));
  };
  useEffect(load, [id]);

  const setState = async (patch: Record<string, unknown>) => {
    await sendJson(`/api/movie/${id}/state`, "POST", patch);
    load();
  };

  const filtered = useMemo(() => {
    if (!detail) return [];
    return tab === "all" ? detail.logs : detail.logs.filter((l) => l.kind === tab);
  }, [detail, tab]);

  if (!detail) return <p className="p-6 text-[#5a6b7c]">加载中…</p>;
  const m = detail.movie;
  const directors = parseJsonArray(m.directors).join(" / ");
  const genres = parseJsonArray(m.genres);

  return (
    <div className="pb-10">
      {/* hero */}
      <div
        className="relative h-64 bg-cover bg-center"
        style={{
          backgroundImage: `linear-gradient(rgba(20,24,28,0.5), #14181c), url(${posterUrl(m.tmdb_id)})`,
        }}
      >
        <div className="absolute bottom-0 left-0 right-0 flex items-end gap-6 p-6">
          <Poster tmdbId={m.tmdb_id} title={m.title_main} size="large" />
          <div className="flex-1 pb-2">
            <h1 className="text-2xl font-bold">
              {m.title_main}
              <span className="ml-3 text-base font-normal text-[#8899aa]">{m.title_sub}</span>
            </h1>
            <p className="mt-2 text-sm text-[#8899aa]">
              {m.year} · {directors} · {m.runtime ?? "?"}min
              {genres.length > 0 && ` · ${genres.join(" / ")}`}
            </p>
            <p className="mt-1 text-sm text-[#5a6b7c]">
              TMDB {m.lb_rating ? `· Letterboxd ${m.lb_rating} (${m.lb_votes})` : ""}
            </p>
            {m.tagline && <p className="mt-2 text-sm italic text-[#40bcf4]">“{m.tagline}”</p>}
          </div>
        </div>
      </div>

      <div className="mx-auto max-w-[1100px] px-6">
        {/* 动作条 */}
        <div className="my-4 flex items-center gap-4 rounded-lg border border-[#2c3440] bg-[#1b222b] p-3">
          <button
            className={"rounded border px-3 py-1.5 text-sm " +
              (m.in_watchlist ? "border-[#ff8000] text-[#ff8000]" : "border-[#456] text-[#8899aa]")}
            onClick={() => setState({ in_watchlist: !m.in_watchlist })}
          >
            🔖 想看
          </button>
          <button
            className={"text-2xl " + (m.liked ? "text-[#00e054]" : "text-[#3a4653]")}
            onClick={() => setState({ liked: !m.liked })}
            title="喜欢"
          >
            ♥
          </button>
          <StarsEditor value={m.my_rating} onChange={(v) => setState({ my_rating: v })} />
          <span className="ml-auto text-xs text-[#5a6b7c]">
            {m.watched ? "已看" : "未看"}
          </span>
        </div>

        {/* Log Tab + 创建入口 */}
        <div className="mb-3 flex items-center justify-between">
          <div className="flex gap-1">
            {[["all", "全部"], ["watch", "看"], ["review", "评"], ["chat", "聊"]].map(([k, label]) => (
              <button
                key={k}
                className={"rounded px-3 py-1.5 text-sm " +
                  (tab === k ? "bg-[#2c3440] text-white" : "text-[#8899aa]")}
                onClick={() => setTab(k)}
              >
                {label}
              </button>
            ))}
          </div>
          <div className="flex gap-2">
            <button
              className="rounded bg-[#00e054] px-3 py-1.5 text-sm text-[#0c1a10]"
              onClick={() => setShowDiary(true)}
            >
              + 记一笔
            </button>
            <button
              className="rounded border border-[#40bcf4] px-3 py-1.5 text-sm text-[#40bcf4]"
              onClick={() => setShowReview(true)}
            >
              + 写影评
            </button>
          </div>
        </div>

        {/* Log 流 */}
        <div className="space-y-2">
          {filtered.length === 0 && <p className="text-sm text-[#5a6b7c]">暂无记录</p>}
          {filtered.map((l) => (
            <div key={l.kind + l.id} className="rounded border border-[#2c3440] bg-[#1b222b] px-3 py-2 text-sm">
              <span
                className="mr-2 rounded px-1 text-xs"
                style={{ color: KIND_BADGE[l.kind]?.color }}
              >
                {KIND_BADGE[l.kind]?.label}
              </span>
              <span className="text-[#8899aa]">{l.at}</span>
              <span className="ml-2">{l.brief}</span>
            </div>
          ))}
        </div>

        {/* List 归属 */}
        {detail.lists.length > 0 && (
          <div className="mt-4 text-sm">
            {detail.lists.map((l) => (
              <span key={l.list_id} className="mr-2 rounded bg-[#2c3440] px-2 py-1 text-xs">
                {l.ranked && l.rank ? `Rank #${l.rank} in ${l.name}` : `In ${l.name}`}
              </span>
            ))}
          </div>
        )}
      </div>

      {showDiary && <DiaryModal movieId={m.tmdb_id} onClose={() => { setShowDiary(false); load(); }} onSaved={() => { setShowDiary(false); load(); }} />}
      {showReview && (
        <ReviewModal movieId={m.tmdb_id} onClose={() => { setShowReview(false); load(); }} onSaved={() => { setShowReview(false); load(); }} />
      )}
      <button className="hidden" onClick={() => navigate("/")}> </button>
    </div>
  );
}

// ============ 弹出卡：添加 Diary（基础 + Advanced）============
function DiaryModal({
  movieId,
  onClose,
  onSaved,
}: {
  movieId: number;
  onClose: () => void;
  onSaved: () => void;
}) {
  const today = new Date().toISOString().slice(0, 10);
  const [date, setDate] = useState(today);
  const [rating, setRating] = useState<number | null>(null);
  const [theater, setTheater] = useState(false);
  const [price, setPrice] = useState("");
  const [liked, setLiked] = useState(false);
  const [note, setNote] = useState("");
  const [advanced, setAdvanced] = useState(false);
  const [dims, setDims] = useState<Record<string, string>>({});
  const [tags, setTags] = useState("");
  const [err, setErr] = useState("");

  const submit = async () => {
    const dimensions: Record<string, string[]> = {};
    for (const [dim, raw] of Object.entries(dims)) {
      const list = raw.split(/[,，]/).map((s) => s.trim()).filter(Boolean);
      if (list.length) dimensions[dim] = list;
    }
    try {
      await sendJson("/api/diary", "POST", {
        action: "add",
        movie_id: movieId,
        watched_date: date,
        rating,
        in_theater: theater,
        liked,
        ticket_price_cents: price === "" ? null : Math.round(Number(price) * 100),
        note,
        dimensions,
        tags: tags.split(/[,，]/).map((s) => s.trim()).filter(Boolean),
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
        <div className="space-y-3">
          <StarsEditor value={rating} onChange={setRating} />
          <input
            type="date"
            className="w-full rounded border border-[#33414f] bg-[#14181c] px-2 py-1.5 text-sm"
            value={date}
            onChange={(e) => setDate(e.target.value)}
          />
          <label className="flex items-center gap-4 text-sm">
            <span className="flex items-center gap-1">
              <input type="checkbox" checked={theater} onChange={(e) => setTheater(e.target.checked)} />
              影院观看
            </span>
            {theater && (
              <span className="flex items-center gap-1">
                票价 ¥
                <input
                  type="number"
                  className="w-20 rounded border border-[#33414f] bg-[#14181c] px-2 py-1"
                  value={price}
                  onChange={(e) => setPrice(e.target.value)}
                />
              </span>
            )}
            <span className="flex items-center gap-1">
              <input type="checkbox" checked={liked} onChange={(e) => setLiked(e.target.checked)} />
              喜欢
            </span>
          </label>
          <textarea
            rows={3}
            placeholder="随记（仅本地）"
            className="w-full rounded border border-[#33414f] bg-[#14181c] px-2 py-1.5 text-sm"
            value={note}
            onChange={(e) => setNote(e.target.value)}
          />
          <button
            className="text-xs text-[#40bcf4]"
            onClick={() => setAdvanced(!advanced)}
          >
            {advanced ? "收起 Advanced ▲" : "Advanced（维度与标签）▼"}
          </button>
          {advanced && (
            <div className="space-y-2 rounded border border-[#2c3440] p-2">
              {["地点", "同伴", "情绪", "场景"].map((dim) => (
                <label key={dim} className="block text-sm">
                  <span className="text-[#5a6b7c]">{dim}（逗号分隔，可新建） </span>
                  <input
                    className="mt-1 w-full rounded border border-[#33414f] bg-[#14181c] px-2 py-1"
                    value={dims[dim] ?? ""}
                    onChange={(e) => setDims({ ...dims, [dim]: e.target.value })}
                  />
                </label>
              ))}
              <label className="block text-sm">
                <span className="text-[#5a6b7c]">自由标签（逗号分隔） </span>
                <input
                  className="mt-1 w-full rounded border border-[#33414f] bg-[#14181c] px-2 py-1"
                  value={tags}
                  onChange={(e) => setTags(e.target.value)}
                />
              </label>
            </div>
          )}
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
  const [liked, setLiked] = useState(false);
  const [err, setErr] = useState("");

  const submit = async () => {
    try {
      await sendJson("/api/reviews", "POST", {
        action: "add",
        movie_id: movieId,
        title,
        body_md: body,
        rating,
        liked,
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
          <label className="flex items-center gap-1 text-sm">
            <input type="checkbox" checked={liked} onChange={(e) => setLiked(e.target.checked)} />
            喜欢
          </label>
          <textarea
            rows={8}
            placeholder="正文（Markdown）—— 公开长评，可对外导出"
            className="w-full rounded border border-[#33414f] bg-[#14181c] px-2 py-1.5 text-sm"
            value={body}
            onChange={(e) => setBody(e.target.value)}
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

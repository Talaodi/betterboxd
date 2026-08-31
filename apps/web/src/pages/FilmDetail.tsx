import { useEffect, useMemo, useRef, useState } from "react";
import { useParams } from "react-router-dom";
import { getJson, parseJsonArray, sendJson, type LogRow, type MovieRow } from "../api";
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
  const [detail, setDetail] = useState<Detail | null>(null);
  const [tab, setTab] = useState("all");
  const [showDiary, setShowDiary] = useState(false);
  const [showReview, setShowReview] = useState(false);
  const [chatOpen, setChatOpen] = useState(false);
  const [chatInput, setChatInput] = useState("");
  const [chatMsgs, setChatMsgs] = useState<Record<string, { role: string; text: string }[]>>({});
  const chatWsRef = useRef<WebSocket | null>(null);
  const movieIdStr = id ?? "";

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

  useEffect(() => {
    if (!chatOpen || !movieIdStr) return;
    const proto = location.protocol === "https:" ? "wss" : "ws";
    const ws = new WebSocket(`${proto}://${location.host}/ws/chat?movie_id=${movieIdStr}`);
    ws.onmessage = (ev) => {
      const f = JSON.parse(ev.data);
      if (f.type === "token") {
        setChatMsgs((prev) => {
          const arr = prev[movieIdStr] ?? [];
          const last = arr[arr.length - 1];
          if (last?.role === "assistant") {
            const copy = { ...prev, [movieIdStr]: [...arr.slice(0, -1), { ...last, text: last.text + (f.data ?? "") }] };
            return copy;
          }
          return { ...prev, [movieIdStr]: [...arr, { role: "assistant", text: f.data ?? "" }] };
        });
      } else if (f.type === "done" || f.type === "error") {
        setChatMsgs((prev) => {
          const arr = prev[movieIdStr] ?? [];
          return { ...prev, [movieIdStr]: [...arr, { role: "system", text: f.type === "error" ? f.message : "" }] };
        });
      }
    };
    chatWsRef.current = ws;
    return () => ws.close();
  }, [chatOpen, movieIdStr]);

  const sendChat = () => {
    const text = chatInput.trim();
    if (!text || !chatWsRef.current) return;
    setChatMsgs((prev) => ({
      ...prev,
      [movieIdStr]: [...(prev[movieIdStr] ?? []), { role: "user", text }],
    }));
    setChatInput("");
    chatWsRef.current.send(JSON.stringify({ type: "user", text }));
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

        {/* 右：动作面板（LB 式竖排） */}
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
                (m.liked ? "text-[#00e054]" : "text-[#8899aa]")}
              onClick={() => setState({ liked: !m.liked })}
            >
              <span className="text-xl">♥</span>
              喜欢
            </button>
          </div>
          <div className="border-t border-[#2c3440] pt-3">
            <p className="mb-1 text-center text-xs text-[#8899aa]">Rate</p>
            <StarsEditor value={m.my_rating} onChange={(v) => setState({ my_rating: v })} />
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
              className="rounded border border-[#456] px-3 py-1.5 text-sm text-[#8899aa]"
              onClick={() => setChatOpen(true)}
            >
              💬 讨论
            </button>
          </div>
        </div>
      </div>

      {/* Log Tab + 流 */}
      <div className="mt-8">
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
          {detail.lists.length > 0 && (
            <div className="text-xs">
              {detail.lists.map((l) => (
                <span key={l.list_id} className="mr-2 rounded bg-[#2c3440] px-2 py-1">
                  {l.ranked && l.rank ? `Rank #${l.rank} in ${l.name}` : `In ${l.name}`}
                </span>
              ))}
            </div>
          )}
        </div>
        <div className="space-y-2">
          {filtered.length === 0 && <p className="text-sm text-[#5a6b7c]">暂无记录</p>}
          {filtered.map((l: LogRow) => (
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
      </div>
      {chatOpen && (
        <div className="fixed inset-y-0 right-0 z-40 flex w-[400px] flex-col border-l border-[#33414f] bg-[#14181c]">
          <div className="flex items-center justify-between border-b border-[#2c3440] px-4 py-3">
            <span className="text-sm font-medium">💬 陪看讨论</span>
            <button className="text-[#5a6b7c] hover:text-white" onClick={() => setChatOpen(false)}>✕</button>
          </div>
          <div className="flex-1 space-y-3 overflow-y-auto p-4">
            {(chatMsgs[movieIdStr] ?? []).map((m: any, i: number) => (
              <div key={i} className={m.role === "user" ? "text-right" : "text-left"}>
                <span className={"inline-block max-w-[85%] whitespace-pre-wrap rounded-lg px-3 py-2 text-sm " +
                  (m.role === "user" ? "bg-[#2c3440]" : "bg-[#232b35]")}>{m.text}</span>
              </div>
            ))}
          </div>
          <div className="border-t border-[#2c3440] p-3">
            <div className="flex gap-2">
              <input className="flex-1 rounded border border-[#33414f] bg-[#14181c] px-3 py-2 text-sm"
                placeholder="聊聊这部电影…"
                value={chatInput} onChange={(e) => setChatInput(e.target.value)}
                onKeyDown={(e) => e.key === "Enter" && sendChat()} />
              <button className="rounded bg-[#00e054] px-3 py-2 text-sm text-[#0c1a10]"
                onClick={sendChat}>发送</button>
            </div>
          </div>
        </div>
      )}
      {showDiary && <DiaryModal movieId={m.tmdb_id} onClose={() => { setShowDiary(false); load(); }} onSaved={() => { setShowDiary(false); load(); }} />}
      {showReview && <ReviewModal movieId={m.tmdb_id} onClose={() => { setShowReview(false); load(); }} onSaved={() => { setShowReview(false); load(); }} />}
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
  const nowD = new Date();
  const today = `${nowD.getFullYear()}-${String(nowD.getMonth() + 1).padStart(2, "0")}-${String(nowD.getDate()).padStart(2, "0")}`;
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
          <button className="text-xs text-[#40bcf4]" onClick={() => setAdvanced(!advanced)}>
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

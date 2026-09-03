/** Lists（report v2）：/lists 列表页 + /lists/:id 详情页（子路由）。
 *  - 列表卡：海报撑满卡片高度、标题大字与小字竖向居中、小字=日期·(un)ranked·N 部（无描述）、整卡可点
 *  - 详情：小字（日期 · (un)ranked · N 部）+ 描述独立一行；ranked/unranked 完全相同竖排行卡，
 *    都可拖拽排序（ranked 显示名次，unranked 只是不显示数字）；整行可点进影片详情 */
import { useCallback, useEffect, useRef, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { getJson, sendJson } from "../api";
import Poster from "../components/Poster";
import { StarsDisplay } from "../components/Stars";

export type ListMeta = {
  id: string;
  name: string;
  description: string | null;
  source: string;
  ranked: number;
  created_at: number;
  updated_at: number;
  item_count: number;
  cover_movie_id: number | null;
};

export type ListItem = {
  rank: number | null;
  added_at: number;
  tmdb_id: number;
  title_main: string;
  title_sub: string;
  year: string | null;
  my_rating: number | null;
  liked: number;
  watched: number;
  posters: string;
};

const fmtDate = (unix: number) =>
  new Date(unix * 1000).toISOString().slice(0, 10).replace(/-/g, "/");

/** 新建/编辑清单弹窗（name+description+ranked） */
export function ListFormModal({
  initial,
  onClose,
  onSaved,
}: {
  initial?: ListMeta;
  onClose: () => void;
  onSaved: () => void;
}) {
  const [name, setName] = useState(initial?.name ?? "");
  const [description, setDescription] = useState(initial?.description ?? "");
  const [ranked, setRanked] = useState(initial?.ranked === 1);
  const [err, setErr] = useState("");
  const submit = async () => {
    try {
      if (initial) {
        await sendJson(`/api/lists/${initial.id}`, "PUT", { name, description, ranked });
      } else {
        await sendJson("/api/lists", "POST", { name, description, ranked });
      }
      onSaved();
      onClose();
    } catch (e) {
      setErr((e as Error).message);
    }
  };
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60">
      <div className="w-[400px] rounded-lg border border-[#33414f] bg-[#1b222b] p-4">
        <h2 className="mb-3 text-base font-medium">
          {initial ? "编辑清单" : "新建清单"}
        </h2>
        {err && <p className="mb-2 text-sm text-[#ff8000]">{err}</p>}
        <div className="space-y-3">
          <input
            autoFocus
            placeholder="清单名"
            className="w-full rounded border border-[#33414f] bg-[#14181c] px-2 py-1.5 text-sm"
            value={name}
            onChange={(e) => setName(e.target.value)}
          />
          <textarea
            rows={2}
            placeholder="描述（可选）"
            className="w-full rounded border border-[#33414f] bg-[#14181c] px-2 py-1.5 text-sm"
            value={description}
            onChange={(e) => setDescription(e.target.value)}
          />
          <label className="flex items-center gap-1 text-sm">
            <input
              type="checkbox"
              checked={ranked}
              onChange={(e) => setRanked(e.target.checked)}
            />
            排名清单（显示名次，顺序即排名）
          </label>
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

/** ============ /lists 列表页 ============ */
export default function Lists() {
  const [lists, setLists] = useState<ListMeta[] | null>(null);
  const [creating, setCreating] = useState(false);
  const [editing, setEditing] = useState<ListMeta | null>(null);
  const navigate = useNavigate();

  const load = useCallback(() => {
    getJson<{ lists: ListMeta[] }>("/api/lists")
      .then((d) => setLists(d.lists))
      .catch((e) => alert(`加载失败: ${e.message}`));
  }, []);
  useEffect(load, [load]);

  const removeList = async (l: ListMeta) => {
    if (!window.confirm(`删除清单「${l.name}」？（清单内影片本体不受影响）`)) return;
    try {
      await sendJson(`/api/lists/${l.id}`, "DELETE");
      load();
    } catch (e) {
      alert(`删除失败: ${(e as Error).message}`);
    }
  };

  return (
    <div className="mx-auto max-w-[860px] px-6 pt-6 pb-10">
      <div className="mb-4 flex items-center justify-between">
        <h1 className="text-lg tracking-wide text-[#8899aa]">Lists</h1>
        <div className="flex items-center gap-3">
          <span className="text-xs text-[#5a6b7c]">{lists?.length ?? 0} 个清单</span>
          <button
            className="rounded bg-[#00e054] px-3 py-1 text-sm font-medium text-[#0c1a10]"
            onClick={() => setCreating(true)}
          >
            + 新建清单
          </button>
        </div>
      </div>
      {!lists && <p className="text-[#5a6b7c]">加载中…</p>}
      {lists && lists.length === 0 && (
        <p className="text-sm text-[#5a6b7c]">
          还没有清单。点「+ 新建清单」，或对助手说"建一个清单"。
        </p>
      )}
      <div className="space-y-2">
        {lists?.map((l) => (
          /* 整卡可点（✎/🗑 阻止冒泡）；海报撑满卡片高度 */
          <div
            key={l.id}
            className="flex items-stretch overflow-hidden rounded-lg border border-[#2c3440] bg-[#1b222b] hover:border-[#40bcf4]"
            onClick={() => navigate(`/lists/${l.id}`)}
          >
            <div className="w-[60px] shrink-0 bg-[#14181c]">
              {l.cover_movie_id && (
                <Poster
                  tmdbId={l.cover_movie_id}
                  title=""
                  size="grid"
                  className="h-full! w-[60px]! object-cover"
                />
              )}
            </div>
            <div className="flex min-w-0 flex-1 flex-col justify-center px-4 py-3">
              <h2 className="truncate text-lg font-semibold">{l.name}</h2>
              <p className="mt-0.5 text-xs text-[#5a6b7c]">
                {fmtDate(l.updated_at)} · {l.ranked === 1 ? "Ranked" : "Unranked"} · {l.item_count} 部
              </p>
            </div>
            {l.source === "manual" && (
              <div className="flex shrink-0 flex-col justify-center gap-3 px-4 text-sm text-[#5a6b7c]">
                <button
                  className="hover:text-[#40bcf4]"
                  title="编辑"
                  onClick={(e) => {
                    e.stopPropagation();
                    setEditing(l);
                  }}
                >
                  ✎
                </button>
                <button
                  className="hover:text-[#ff8000]"
                  title="删除"
                  onClick={(e) => {
                    e.stopPropagation();
                    removeList(l);
                  }}
                >
                  🗑
                </button>
              </div>
            )}
          </div>
        ))}
      </div>
      {creating && <ListFormModal onClose={() => setCreating(false)} onSaved={load} />}
      {editing && (
        <ListFormModal initial={editing} onClose={() => setEditing(null)} onSaved={load} />
      )}
    </div>
  );
}

/** ============ /lists/:id 详情页 ============ */
export function ListDetail() {
  const { id = "" } = useParams<{ id: string }>();
  const navigate = useNavigate();
  const [detail, setDetail] = useState<{ list: ListMeta; items: ListItem[] } | null>(null);
  const [editing, setEditing] = useState(false);
  const dragFrom = useRef<number | null>(null);
  const [dragOver, setDragOver] = useState<number | null>(null);

  const loadDetail = useCallback(() => {
    if (!id) return;
    getJson<{ list: ListMeta; items: ListItem[] }>(`/api/lists/${id}`)
      .then(setDetail)
      .catch((e) => alert(`加载失败: ${e.message}`));
  }, [id]);
  useEffect(loadDetail, [loadDetail]);

  const removeItem = async (movieId: number) => {
    try {
      await sendJson(`/api/lists/${id}/items/${movieId}`, "DELETE");
      loadDetail();
    } catch (e) {
      alert(`移除失败: ${(e as Error).message}`);
    }
  };

  /** 拖拽落定：新位置顺序批量写 rank（1..n） */
  const reorder = async (from: number, to: number) => {
    if (!detail || from === to) return;
    const items = [...detail.items];
    const [moved] = items.splice(from, 1);
    items.splice(to, 0, moved);
    setDetail({ ...detail, items });
    try {
      await Promise.all(
        items.map((it, i) =>
          sendJson(`/api/lists/${id}/items/${it.tmdb_id}`, "PUT", { rank: i + 1 }),
        ),
      );
    } catch (e) {
      alert(`排序失败: ${(e as Error).message}`);
    }
    loadDetail();
  };

  if (!detail) return <p className="p-6 text-[#5a6b7c]">加载中…</p>;
  const l = detail.list;
  const editable = l.source === "manual";

  return (
    <div className="mx-auto max-w-[1100px] px-6 pt-6 pb-10">
      <button className="mb-4 text-sm text-[#40bcf4]" onClick={() => navigate("/lists")}>
        ← 返回清单列表
      </button>
      <div className="flex items-baseline justify-between">
        <h1 className="text-2xl font-bold">{l.name}</h1>
        {editable && (
          <button className="text-sm text-[#40bcf4] hover:underline" onClick={() => setEditing(true)}>
            ✎ 编辑清单
          </button>
        )}
      </div>
      {/* 元信息行 + 描述独立行 */}
      <p className="mt-1 text-xs text-[#5a6b7c]">
        {fmtDate(l.updated_at)} · {l.ranked === 1 ? "Ranked" : "Unranked"} · {detail.items.length} 部影片
        {l.source === "letterboxd" ? " · 来自 Letterboxd（只读镜像）" : ""}
      </p>
      {l.description && <p className="mt-1 text-sm text-[#8899aa]">{l.description}</p>}

      <div className="mt-6 border-t border-[#2c3440]">
        {detail.items.map((it, i) => (
          <div
            key={it.tmdb_id}
            draggable={editable}
            onDragStart={() => (dragFrom.current = i)}
            onDragOver={(e) => {
              e.preventDefault();
              setDragOver(i);
            }}
            onDragEnd={() => {
              dragFrom.current = null;
              setDragOver(null);
            }}
            onDrop={() => {
              if (dragFrom.current !== null) reorder(dragFrom.current, i);
              dragFrom.current = null;
              setDragOver(null);
            }}
            onClick={() => navigate(`/film/${it.tmdb_id}`)}
            className={
              "flex items-center gap-4 border-b border-[#1e2630] px-2 py-2 " +
              (editable ? "cursor-grab active:cursor-grabbing hover:bg-[#1b222b] " : "") +
              (dragOver === i && dragFrom.current !== null && dragFrom.current !== i
                ? "border-t-2 border-t-[#00e054]"
                : "")
            }
          >
            {l.ranked === 1 ? (
              <span className="w-10 shrink-0 text-center text-xl font-bold text-[#ff8000]">
                {i + 1}
              </span>
            ) : (
              <span className="w-10 shrink-0 text-center text-xl text-[#3a4653]">·</span>
            )}
            {editable && (
              <span className="shrink-0 text-[#5a6b7c]" title="拖拽排序">
                ⠿
              </span>
            )}
            <img
              src={`/api/poster/${it.tmdb_id}`}
              onError={(e) => ((e.target as HTMLImageElement).style.visibility = "hidden")}
              className="h-[63px] w-[42px] shrink-0 rounded bg-[#14181c] object-cover"
              alt=""
            />
            <div className="min-w-0 flex-1">
              <div className="truncate text-base font-semibold">
                {it.title_main}
                {it.title_sub && it.title_sub !== it.title_main && (
                  <span className="ml-2 font-normal italic text-[#5a6b7c]">({it.title_sub})</span>
                )}
              </div>
              <div className="mt-0.5 flex items-center gap-3 text-xs text-[#5a6b7c]">
                <span>{it.year}</span>
                <StarsDisplay value={it.my_rating} />
                {it.liked === 1 && <span className="text-[#00e054]">♥</span>}
              </div>
            </div>
            {editable && (
              <button
                className="shrink-0 pr-2 text-[#5a6b7c] hover:text-[#ff8000]"
                title="移出清单"
                onClick={(e) => {
                  e.stopPropagation();
                  removeItem(it.tmdb_id);
                }}
              >
                ×
              </button>
            )}
          </div>
        ))}
      </div>
      {detail.items.length === 0 && (
        <p className="mt-6 text-sm text-[#5a6b7c]">
          清单为空。可在影片详情页点「+ 加入 List」，或对助手说。
        </p>
      )}
      {editing && (
        <ListFormModal
          initial={l}
          onClose={() => setEditing(false)}
          onSaved={() => {
            loadDetail();
          }}
        />
      )}
    </div>
  );
}

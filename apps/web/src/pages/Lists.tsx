/** Lists 页（P2）：清单 CRUD + 详情条目管理（rank 编辑/移除）。
 *  manual 清单可编辑；letterboxd 镜像只读（无编辑控件）。 */
import { useCallback, useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { getJson, sendJson } from "../api";
import Poster from "../components/Poster";
import { StarsDisplay } from "../components/Stars";

type ListMeta = {
  id: string;
  name: string;
  description: string | null;
  source: string;
  ranked: number;
  created_at: number;
  updated_at: number;
  item_count: number;
};

type ListItem = {
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

/** 新建/编辑清单弹窗（name+description+ranked） */
function ListFormModal({
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
            排名清单（Rank #N）
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

export default function Lists() {
  const [lists, setLists] = useState<ListMeta[] | null>(null);
  const [openId, setOpenId] = useState<string | null>(null);
  const [detail, setDetail] = useState<{ list: ListMeta; items: ListItem[] } | null>(null);
  const [creating, setCreating] = useState(false);
  const [editing, setEditing] = useState<ListMeta | null>(null);

  const loadLists = useCallback(() => {
    getJson<{ lists: ListMeta[] }>("/api/lists")
      .then((d) => setLists(d.lists))
      .catch((e) => alert(`加载失败: ${e.message}`));
  }, []);
  useEffect(loadLists, [loadLists]);

  const loadDetail = useCallback((id: string) => {
    setDetail(null);
    getJson<{ list: ListMeta; items: ListItem[] }>(`/api/lists/${id}`)
      .then(setDetail)
      .catch((e) => alert(`加载失败: ${e.message}`));
  }, []);
  useEffect(() => {
    if (openId) loadDetail(openId);
  }, [openId, loadDetail]);

  const removeList = async (l: ListMeta) => {
    if (!window.confirm(`删除清单「${l.name}」？（清单内影片本体不受影响）`)) return;
    try {
      await sendJson(`/api/lists/${l.id}`, "DELETE");
      loadLists();
    } catch (e) {
      alert(`删除失败: ${(e as Error).message}`);
    }
  };

  const removeItem = async (movieId: number) => {
    if (!openId) return;
    try {
      await sendJson(`/api/lists/${openId}/items/${movieId}`, "DELETE");
      loadDetail(openId);
    } catch (e) {
      alert(`移除失败: ${(e as Error).message}`);
    }
  };

  const changeRank = async (movieId: number, rank: number | null) => {
    if (!openId) return;
    try {
      await sendJson(`/api/lists/${openId}/items/${movieId}`, "PUT", { rank });
      loadDetail(openId);
    } catch (e) {
      alert(`排名修改失败: ${(e as Error).message}`);
    }
  };

  if (openId && detail) {
    const l = detail.list;
    const editable = l.source === "manual";
    return (
      <div className="mx-auto max-w-[1100px] px-6 pt-6 pb-10">
        <button className="mb-4 text-sm text-[#40bcf4]" onClick={() => setOpenId(null)}>
          ← 返回清单列表
        </button>
        <div className="flex items-baseline justify-between">
          <h1 className="text-2xl font-bold">{l.name}</h1>
          {editable && (
            <button
              className="text-sm text-[#40bcf4] hover:underline"
              onClick={() => setEditing(l)}
            >
              ✎ 编辑清单
            </button>
          )}
        </div>
        {l.description && <p className="mt-1 text-sm text-[#8899aa]">{l.description}</p>}
        <p className="mt-1 text-xs text-[#5a6b7c]">
          {detail.items.length} 部影片
          {l.ranked === 1 ? " · 排名清单" : ""}
          {l.source === "letterboxd" ? " · 来自 Letterboxd（只读镜像）" : ""}
        </p>
        <div className="mt-6 grid grid-cols-[repeat(auto-fill,minmax(130px,1fr))] gap-4">
          {detail.items.map((it) => (
            <div key={it.tmdb_id} className="relative">
              {l.ranked === 1 && it.rank !== null && (
                <span className="absolute left-1 top-1 z-10 rounded bg-black/70 px-1.5 py-0.5 text-xs font-bold text-[#ff8000]">
                  #{it.rank}
                </span>
              )}
              {editable && (
                <button
                  className="absolute right-1 top-1 z-10 rounded bg-black/70 px-1.5 py-0.5 text-xs text-[#8899aa] hover:text-[#ff8000]"
                  title="移出清单"
                  onClick={() => removeItem(it.tmdb_id)}
                >
                  ×
                </button>
              )}
              <Link to={`/film/${it.tmdb_id}`}>
                <Poster tmdbId={it.tmdb_id} title={it.title_main} size="grid" className="rounded" />
              </Link>
              {editable && l.ranked === 1 && (
                <div className="mt-1 flex items-center gap-1 text-[10px] text-[#5a6b7c]">
                  名次
                  <input
                    type="number"
                    className="w-14 rounded border border-[#33414f] bg-[#14181c] px-1 py-0.5 text-xs"
                    defaultValue={it.rank ?? ""}
                    onBlur={(e) => {
                      const v = e.target.value.trim();
                      const next = v === "" ? null : Number(v);
                      if (next !== it.rank) changeRank(it.tmdb_id, next);
                    }}
                  />
                </div>
              )}
              <p className="mt-1 truncate text-xs" title={it.title_main}>{it.title_main}</p>
              <div className="flex items-center justify-between">
                <span className="text-[10px] text-[#5a6b7c]">{it.year}</span>
                {it.liked === 1 && <span className="text-[10px] text-[#00e054]">♥</span>}
              </div>
              <StarsDisplay value={it.my_rating} />
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
            initial={editing}
            onClose={() => setEditing(null)}
            onSaved={() => {
              loadLists();
              loadDetail(l.id);
            }}
          />
        )}
      </div>
    );
  }

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
      <div className="space-y-3">
        {lists?.map((l) => (
          <div
            key={l.id}
            className="flex items-start rounded-lg border border-[#2c3440] bg-[#1b222b] p-4 hover:border-[#40bcf4]"
          >
            <button className="min-w-0 flex-1 text-left" onClick={() => setOpenId(l.id)}>
              <div className="flex items-baseline justify-between">
                <h2 className="text-base font-medium">{l.name}</h2>
                <span className="text-xs text-[#5a6b7c]">{l.item_count} 部</span>
              </div>
              {l.description && <p className="mt-1 text-sm text-[#8899aa]">{l.description}</p>}
              <p className="mt-2 flex gap-2 text-[10px] text-[#5a6b7c]">
                {l.ranked === 1 && <span className="rounded bg-[#2c3440] px-1.5 py-0.5">排名</span>}
                {l.source === "letterboxd" && (
                  <span className="rounded bg-[#2c3440] px-1.5 py-0.5">Letterboxd</span>
                )}
                <span className="rounded bg-[#2c3440] px-1.5 py-0.5">
                  {new Date(l.updated_at * 1000).toISOString().slice(0, 10)}
                </span>
              </p>
            </button>
            {l.source === "manual" && (
              <div className="ml-3 flex shrink-0 gap-3 text-sm text-[#5a6b7c]">
                <button className="hover:text-[#40bcf4]" title="编辑" onClick={() => setEditing(l)}>
                  ✎
                </button>
                <button className="hover:text-[#ff8000]" title="删除" onClick={() => removeList(l)}>
                  🗑
                </button>
              </div>
            )}
          </div>
        ))}
      </div>
      {creating && (
        <ListFormModal onClose={() => setCreating(false)} onSaved={loadLists} />
      )}
      {editing && (
        <ListFormModal initial={editing} onClose={() => setEditing(null)} onSaved={loadLists} />
      )}
    </div>
  );
}

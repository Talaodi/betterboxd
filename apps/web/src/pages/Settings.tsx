import { useEffect, useState } from "react";
import { getJson, sendJson } from "../api";

type Pricing = {
  input_cached: number;
  input_uncached: number;
  output_cached: number;
  output_uncached: number;
};
type Profile = {
  name: string; endpoint: string; api_key: string; model: string;
  context_length: number; thinking_mode: string;
  temperature: number | null; top_p: number | null;
  max_output_tokens: number | null; extra_body_json: string | null;
  pricing: Pricing; currency: string; budget: number | null;
};
type Config = {
  profiles: Profile[];
  active_profile: string | null;
  tmdb: { key: string; proxy: string | null; language: string };
  billing: { display_currency: string; fx_rates: Record<string, number>; budget_monthly: number | null; budget_total: number | null };
  display: { title_main: string; title_sub: string };
};
type UsageRow = {
  profile_name: string; total: number; cache_cost: number;
  cache_currency: string; budget: number | null; currency: string; pricing: Pricing;
};

export default function Settings() {
  const [config, setConfig] = useState<Config | null>(null);
  const [usage, setUsage] = useState<UsageRow[]>([]);
  const [editing, setEditing] = useState<{ index: number; isNew: boolean } | null>(null);
  const [draft, setDraft] = useState<Profile | null>(null);
  const [saving, setSaving] = useState(false);

  const reload = () => {
    getJson<Config>("/api/config").then(setConfig).catch((e) => alert(e.message));
    getJson<{ profiles: UsageRow[] }>("/api/usage/profiles")
      .then((d) => setUsage(d.profiles))
      .catch(() => {});
  };
  useEffect(reload, []);

  const save = async (cfg: Config) => {
    setSaving(true);
    try {
      await sendJson("/api/config", "POST", cfg);
      reload();
    } catch (e) {
      alert(`保存失败: ${(e as Error).message}`);
    }
    setSaving(false);
  };

  if (!config) return <p className="p-6 text-[#5a6b7c]">加载中…</p>;

  const usg = (n: string) => usage.find((u) => u.profile_name === n);

  const openEdit = (i: number) => {
    setEditing({ index: i, isNew: false });
    setDraft({ ...config.profiles[i] });
  };
  const addProfile = () => {
    // 直接弹出编辑界面（report：不要先加空条目再手动点编辑）
    setEditing({ index: config.profiles.length, isNew: true });
    setDraft({
      name: `配置${config.profiles.length + 1}`, endpoint: "", api_key: "", model: "",
      context_length: 8192, thinking_mode: "off", temperature: null, top_p: null,
      max_output_tokens: null, extra_body_json: null,
      pricing: { input_cached: 0, input_uncached: 0, output_cached: 0, output_uncached: 0 },
      currency: "CNY", budget: null,
    });
  };
  const saveDraft = () => {
    if (!draft || !editing) return;
    const ps = editing.isNew ? [...config.profiles, draft] : config.profiles.map((p, j) => (j === editing.index ? draft : p));
    setEditing(null);
    setDraft(null);
    // 首次创建配置自动启用（无活动配置时），避免保存后忘记切换
    const active = editing.isNew && !config.active_profile ? draft.name : config.active_profile;
    save({ ...config, profiles: ps, active_profile: active });
  };

  const resetUsage = async (name: string) => {
    try {
      await sendJson("/api/usage/reset", "POST", { profile_name: name });
      reload();
    } catch (e) {
      alert(`Reset 失败: ${(e as Error).message}`);
    }
  };
  const setActive = (name: string) => save({ ...config, active_profile: name });
  const delProfile = (i: number) => {
    if (!window.confirm(`删除配置「${config.profiles[i].name}」？`)) return;
    save({ ...config, profiles: config.profiles.filter((_, j) => j !== i) });
  };

  const maskKey = (k: string) => (k.length <= 7 ? "****" : `${k.slice(0, 4)}****${k.slice(-3)}`);

  return (
    <div className="mx-auto max-w-[900px] p-4">
      <h1 className="mb-6 text-xl font-bold">设置</h1>

      {/* 模型配置（卡片式） */}
      <section className="mb-6">
        <h2 className="mb-3 text-sm font-medium text-[#8899aa]">模型配置</h2>
        <div className="space-y-3">
          {config.profiles.map((p, i) => {
            const u = usg(p.name);
            return (
              <div key={i} className="rounded-lg border border-[#33414f] bg-[#1b222b] p-3">
                <div className="mb-2 flex items-center justify-between gap-2">
                  <span className="flex items-center gap-2">
                    <span className="font-medium text-white">{p.name}</span>
                    <span className="text-xs text-[#5a6b7c]">{p.model || "未配置模型"}</span>
                  </span>
                  <span className="flex items-center gap-2">
                    <button
                      className={"rounded px-2 py-0.5 text-xs " + (config.active_profile === p.name ? "bg-[#00e054] text-[#0c1a10]" : "text-[#5a6b7c] hover:text-white")}
                      onClick={() => setActive(p.name)}
                    >
                      {config.active_profile === p.name ? "✓ 当前" : "切换"}
                    </button>
                    <button className="text-xs text-[#40bcf4] hover:underline" onClick={() => openEdit(i)}>编辑</button>
                    <button className="text-xs text-[#ff8000] hover:underline" onClick={() => delProfile(i)}>删除</button>
                  </span>
                </div>
                <div className="grid grid-cols-2 gap-x-4 gap-y-1 text-xs text-[#8899aa]">
                  <span>API Key：<span className="font-mono">{maskKey(p.api_key)}</span></span>
                  <span>Endpoint：{p.endpoint || "—"}</span>
                  <span>
                    总用量：{u ? `${fmt(u.total)} ${u.currency}` : "—"}
                    <span className="text-[#5a6b7c]">（历史累计）</span>
                  </span>
                  <span>
                    缓存用量：{u ? `${fmt(u.cache_cost)} ${u.cache_currency}` : "—"}
                    <span className="text-[#5a6b7c]">（预算判断；{p.budget ? `预算 ${fmt(p.budget)}` : "未设预算"}）
                    </span>
                    <button className="ml-1 text-[#40bcf4] hover:underline" onClick={() => resetUsage(p.name)}>Reset</button>
                  </span>
                </div>
              </div>
            );
          })}
        </div>
        <button className="mt-2 text-xs text-[#40bcf4]" onClick={addProfile}>+ 添加配置</button>
      </section>

      {/* TMDB API Key：与 AI 配置分离，独立一行 */}
      <section className="mb-6">
        <h2 className="mb-3 text-sm font-medium text-[#8899aa]">TMDB</h2>
        <div className="rounded-lg border border-[#33414f] bg-[#1b222b] p-3 text-sm">
          <label className="text-xs text-[#8899aa]">API Key（影片搜索/元数据用，与 AI Key 分离）
            <div className="mt-1 flex gap-2">
              <input
                className="flex-1 rounded border border-[#33414f] bg-[#14181c] px-2 py-1"
                placeholder="TMDB API Key"
                value={config.tmdb.key}
                onChange={(e) => setConfig({ ...config, tmdb: { ...config.tmdb, key: e.target.value } })}
              />
              <button className="shrink-0 rounded bg-[#00e054] px-3 py-1 text-xs font-medium text-[#0c1a10]" onClick={() => save(config)}>
                保存
              </button>
            </div>
          </label>
        </div>
      </section>

      {/* 编辑弹窗 */}
      {editing && draft && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4" onClick={() => setEditing(null)}>
          <div className="max-h-[85vh] w-full max-w-[560px] overflow-y-auto rounded-xl border border-[#33414f] bg-[#11151a] p-5" onClick={(e) => e.stopPropagation()}>
            <h3 className="mb-3 font-medium">编辑配置</h3>
            <div className="grid grid-cols-2 gap-2 text-sm">
              <label className="text-xs text-[#8899aa]">名称
                <input className="mt-1 w-full rounded border border-[#33414f] bg-[#14181c] px-2 py-1 text-sm" value={draft.name}
                  onChange={(e) => setDraft({ ...draft, name: e.target.value })} />
              </label>
              <label className="text-xs text-[#8899aa]">Model
                <input className="mt-1 w-full rounded border border-[#33414f] bg-[#14181c] px-2 py-1 text-sm" value={draft.model}
                  onChange={(e) => setDraft({ ...draft, model: e.target.value })} />
              </label>
              <label className="col-span-2 text-xs text-[#8899aa]">Endpoint
                <input className="mt-1 w-full rounded border border-[#33414f] bg-[#14181c] px-2 py-1 text-sm" value={draft.endpoint}
                  onChange={(e) => setDraft({ ...draft, endpoint: e.target.value })} />
              </label>
              <label className="col-span-2 text-xs text-[#8899aa]">API Key
                <input className="mt-1 w-full rounded border border-[#33414f] bg-[#14181c] px-2 py-1 text-sm" value={draft.api_key}
                  onChange={(e) => setDraft({ ...draft, api_key: e.target.value })} placeholder={maskKey(draft.api_key)} />
              </label>
              <label className="text-xs text-[#8899aa]">Context Length
                <input type="number" className="mt-1 w-full rounded border border-[#33414f] bg-[#14181c] px-2 py-1 text-sm" value={draft.context_length}
                  onChange={(e) => setDraft({ ...draft, context_length: Number(e.target.value) })} />
              </label>
              <label className="text-xs text-[#8899aa]">Max Tokens
                <input type="number" className="mt-1 w-full rounded border border-[#33414f] bg-[#14181c] px-2 py-1 text-sm" value={draft.max_output_tokens ?? ""}
                  onChange={(e) => setDraft({ ...draft, max_output_tokens: e.target.value === "" ? null : Number(e.target.value) })} />
              </label>
              <label className="text-xs text-[#8899aa]">温度 Temperature
                <input type="number" step="0.1" className="mt-1 w-full rounded border border-[#33414f] bg-[#14181c] px-2 py-1 text-sm" value={draft.temperature ?? ""}
                  onChange={(e) => setDraft({ ...draft, temperature: e.target.value === "" ? null : Number(e.target.value) })} />
              </label>
              <label className="text-xs text-[#8899aa]">Top P
                <input type="number" step="0.1" className="mt-1 w-full rounded border border-[#33414f] bg-[#14181c] px-2 py-1 text-sm" value={draft.top_p ?? ""}
                  onChange={(e) => setDraft({ ...draft, top_p: e.target.value === "" ? null : Number(e.target.value) })} />
              </label>
            </div>

            {/* 价格与预算 */}
            <h4 className="mt-4 mb-2 text-xs font-medium text-[#8899aa]">计费价格（每 1M token）与预算</h4>
            <div className="grid grid-cols-2 gap-2 text-xs">
              <label className="text-[#8899aa]">输入（缓存命中）
                <input type="number" step="0.0001" className="mt-1 w-full rounded border border-[#33414f] bg-[#14181c] px-2 py-1 text-sm" value={draft.pricing.input_cached}
                  onChange={(e) => setDraft({ ...draft, pricing: { ...draft.pricing, input_cached: Number(e.target.value) } })} />
              </label>
              <label className="text-[#8899aa]">输入（未命中）
                <input type="number" step="0.0001" className="mt-1 w-full rounded border border-[#33414f] bg-[#14181c] px-2 py-1 text-sm" value={draft.pricing.input_uncached}
                  onChange={(e) => setDraft({ ...draft, pricing: { ...draft.pricing, input_uncached: Number(e.target.value) } })} />
              </label>
              <label className="text-[#8899aa]">输出（缓存命中，极少见，默认 0）
                <input type="number" step="0.0001" className="mt-1 w-full rounded border border-[#33414f] bg-[#14181c] px-2 py-1 text-sm" value={draft.pricing.output_cached}
                  onChange={(e) => setDraft({ ...draft, pricing: { ...draft.pricing, output_cached: Number(e.target.value) } })} />
              </label>
              <label className="text-[#8899aa]">输出（未命中）
                <input type="number" step="0.0001" className="mt-1 w-full rounded border border-[#33414f] bg-[#14181c] px-2 py-1 text-sm" value={draft.pricing.output_uncached}
                  onChange={(e) => setDraft({ ...draft, pricing: { ...draft.pricing, output_uncached: Number(e.target.value) } })} />
              </label>
              <label className="text-[#8899aa]">货币
                <select className="mt-1 w-full rounded border border-[#33414f] bg-[#14181c] px-2 py-1 text-sm" value={draft.currency}
                  onChange={(e) => setDraft({ ...draft, currency: e.target.value })}>
                  <option>CNY</option><option>USD</option>
                </select>
              </label>
              <label className="text-[#8899aa]">用量上限（钱；按缓存用量判断，超即停）
                <input type="number" step="0.01" className="mt-1 w-full rounded border border-[#33414f] bg-[#14181c] px-2 py-1 text-sm" value={draft.budget ?? ""}
                  onChange={(e) => setDraft({ ...draft, budget: e.target.value === "" ? null : Number(e.target.value) })} placeholder="不填=不限" />
              </label>
            </div>
            <p className="mt-2 text-[10px] text-[#5a6b7c]">
              提示：缓存命中输出极少见（多数厂商不提供），保留字段默认 0；价格与预算直接在存档目录 config.toml 编辑同样生效。
            </p>

            <div className="mt-4 flex justify-end gap-2">
              <button className="rounded border border-[#33414f] px-4 py-1.5 text-sm text-[#5a6b7c]" onClick={() => setEditing(null)}>取消</button>
              <button className="rounded bg-[#00e054] px-4 py-1.5 text-sm font-medium text-[#0c1a10]" onClick={saveDraft} disabled={saving}>保存</button>
            </div>
          </div>
        </div>
      )}

    </div>
  );
}

function fmt(v: number) {
  if (v <= 0) return "0";
  return v < 0.01 ? v.toFixed(4) : v.toFixed(3);
}

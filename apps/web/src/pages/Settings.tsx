import { useEffect, useState } from "react";
import { getJson, sendJson } from "../api";

type Profile = {
  name: string; endpoint: string; api_key: string; model: string;
  context_length: number; thinking_mode: string;
  temperature: number | null; top_p: number | null;
  max_output_tokens: number | null; extra_body_json: string | null;
};

type Config = {
  profiles: Profile[];
  active_profile: string | null;
  tmdb: { key: string; proxy: string | null; language: string };
  billing: { display_currency: string; fx_rates: Record<string, number>; budget_monthly: number | null; budget_total: number | null };
  display: { title_main: string; title_sub: string };
};

export default function Settings() {
  const [config, setConfig] = useState<Config | null>(null);
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);

  useEffect(() => {
    getJson<Config>("/api/config").then(setConfig).catch((e) => alert(e.message));
  }, []);

  const save = async () => {
    if (!config) return;
    setSaving(true);
    try {
      await sendJson("/api/config", "POST", config);
      setSaved(true);
      setTimeout(() => setSaved(false), 2000);
    } catch (e) {
      alert(`保存失败: ${(e as Error).message}`);
    }
    setSaving(false);
  };

  if (!config) return <p className="p-6 text-[#5a6b7c]">加载中…</p>;

  const updateProfile = (idx: number, patch: Partial<Profile>) => {
    const profiles = [...config.profiles];
    profiles[idx] = { ...profiles[idx], ...patch };
    setConfig({ ...config, profiles });
  };

  return (
    <div className="mx-auto max-w-[900px] p-4">
      <h1 className="mb-6 text-xl font-bold">设置</h1>

      {/* 模型档案 */}
      <section className="mb-6">
        <h2 className="mb-3 text-sm font-medium text-[#8899aa]">模型档案</h2>
        {config.profiles.map((p, i) => (
          <div key={i} className="mb-3 rounded-lg border border-[#33414f] bg-[#1b222b] p-3">
            <div className="mb-2 flex items-center justify-between">
              <input
                className="w-40 rounded bg-[#14181c] px-2 py-1 text-sm font-medium"
                value={p.name}
                onChange={(e) => updateProfile(i, { name: e.target.value })}
              />
              <button
                className={"rounded px-2 py-0.5 text-xs " +
                  (config.active_profile === p.name ? "bg-[#00e054] text-[#0c1a10]" : "text-[#5a6b7c]")}
                onClick={() => setConfig({ ...config, active_profile: p.name })}
              >
                {config.active_profile === p.name ? "✓ 当前" : "切换"}
              </button>
            </div>
            <div className="grid grid-cols-2 gap-2 text-sm">
              <input placeholder="Endpoint" className="rounded border border-[#33414f] bg-[#14181c] px-2 py-1"
                value={p.endpoint} onChange={(e) => updateProfile(i, { endpoint: e.target.value })} />
              <input placeholder="API Key" className="rounded border border-[#33414f] bg-[#14181c] px-2 py-1"
                value={p.api_key} onChange={(e) => updateProfile(i, { api_key: e.target.value })} />
              <input placeholder="Model" className="rounded border border-[#33414f] bg-[#14181c] px-2 py-1"
                value={p.model} onChange={(e) => updateProfile(i, { model: e.target.value })} />
              <input placeholder="Context Length" type="number" className="rounded border border-[#33414f] bg-[#14181c] px-2 py-1"
                value={p.context_length} onChange={(e) => updateProfile(i, { context_length: Number(e.target.value) })} />
              <input placeholder="Temperature" type="number" step="0.1" className="rounded border border-[#33414f] bg-[#14181c] px-2 py-1"
                value={p.temperature ?? ""} onChange={(e) => updateProfile(i, { temperature: e.target.value === "" ? null : Number(e.target.value) })} />
              <input placeholder="Max Tokens" type="number" className="rounded border border-[#33414f] bg-[#14181c] px-2 py-1"
                value={p.max_output_tokens ?? ""} onChange={(e) => updateProfile(i, { max_output_tokens: e.target.value === "" ? null : Number(e.target.value) })} />
            </div>
          </div>
        ))}
        <button
          className="text-xs text-[#40bcf4]"
          onClick={() => setConfig({
            ...config,
            profiles: [...config.profiles, {
              name: `档案${config.profiles.length + 1}`, endpoint: "", api_key: "", model: "",
              context_length: 8192, thinking_mode: "off", temperature: null, top_p: null,
              max_output_tokens: null, extra_body_json: null,
            }],
          })}
        >
          + 添加档案
        </button>
      </section>

      {/* 预算 */}
      <section className="mb-6">
        <h2 className="mb-3 text-sm font-medium text-[#8899aa]">预算</h2>
        <div className="rounded-lg border border-[#33414f] bg-[#1b222b] p-3 text-sm">
          <div className="grid grid-cols-2 gap-2">
            <label>月度预算
              <input type="number" className="mt-1 w-full rounded border border-[#33414f] bg-[#14181c] px-2 py-1"
                value={config.billing.budget_monthly ?? ""}
                onChange={(e) => setConfig({ ...config, billing: { ...config.billing, budget_monthly: e.target.value === "" ? null : Number(e.target.value) } })} />
            </label>
            <label>累计预算
              <input type="number" className="mt-1 w-full rounded border border-[#33414f] bg-[#14181c] px-2 py-1"
                value={config.billing.budget_total ?? ""}
                onChange={(e) => setConfig({ ...config, billing: { ...config.billing, budget_total: e.target.value === "" ? null : Number(e.target.value) } })} />
            </label>
          </div>
        </div>
      </section>

      {/* 保存 */}
      <button
        className="rounded bg-[#00e054] px-6 py-2 text-sm font-medium text-[#0c1a10] disabled:opacity-40"
        onClick={save}
        disabled={saving}
      >
        {saving ? "保存中…" : saved ? "✓ 已保存" : "保存设置"}
      </button>
    </div>
  );
}

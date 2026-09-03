/** 多行文本 + 预览切换（P3）：编辑态 textarea / 预览态 Markdown 渲染。 */
import { useState } from "react";
import { Markdown } from "./ChatPanel";

export default function TextWithPreview({
  value,
  onChange,
  rows = 3,
  placeholder,
}: {
  value: string;
  onChange: (v: string) => void;
  rows?: number;
  placeholder?: string;
}) {
  const [preview, setPreview] = useState(false);
  return (
    <div>
      <div className="mb-1 flex items-center justify-end gap-2 text-xs">
        <button
          className={!preview ? "text-[#40bcf4]" : "text-[#5a6b7c]"}
          onClick={() => setPreview(false)}
        >
          编辑
        </button>
        <span className="text-[#3a4653]">|</span>
        <button
          className={preview ? "text-[#40bcf4]" : "text-[#5a6b7c]"}
          onClick={() => setPreview(true)}
        >
          预览
        </button>
      </div>
      {preview ? (
        <div className="min-h-[80px] rounded border border-[#33414f] bg-[#14181c] px-2 py-1.5 text-sm">
          {value.trim() ? (
            <Markdown>{value}</Markdown>
          ) : (
            <p className="text-[#5a6b7c]">（空）</p>
          )}
        </div>
      ) : (
        <textarea
          rows={rows}
          placeholder={placeholder}
          className="w-full rounded border border-[#33414f] bg-[#14181c] px-2 py-1.5 text-sm"
          value={value}
          onChange={(e) => onChange(e.target.value)}
        />
      )}
    </div>
  );
}

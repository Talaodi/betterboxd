import { useState } from "react";
export function StarsDisplay({ value }: { value: number | null }) {
  if (value === null) {
    return <span className="text-xs text-[#5a6b7c]">未评分</span>;
  }
  const stars = value / 20; // 0-5，0.5 步进
  return (
    <span className="relative inline-block whitespace-nowrap text-[#00e054]">
      <span className="text-[#3a4653]">★★★★★</span>
      <span
        className="absolute left-0 top-0 overflow-hidden"
        style={{ width: `${(stars / 5) * 100}%` }}
      >
        ★★★★★
      </span>
    </span>
  );
}

/** 交互式评分器：半星步进，可清除（再次点击同值清除）。 */
export function StarsEditor({
  value,
  onChange,
}: {
  value: number | null;
  onChange: (v: number | null) => void;
}) {
  const [hover, setHover] = useState<number | null>(null);
  const shown = hover ?? (value === null ? 0 : value);
  const ticks = Array.from({ length: 10 }, (_, i) => (i + 1) * 10); // 10..100
  return (
    <div className="flex items-center gap-1">
      <div
        className="relative inline-block cursor-pointer whitespace-nowrap text-2xl"
        onMouseLeave={() => setHover(null)}
      >
        <span className="text-[#3a4653]">★★★★★</span>
        <span
          className="absolute left-0 top-0 overflow-hidden text-[#00e054]"
          style={{ width: `${(shown / 5) * 100}%` }}
        >
          ★★★★★
        </span>
        {ticks.map((t, i) => (
          <span
            key={t}
            className="absolute top-0 h-full"
            style={{ left: 0, width: `${((i + 1) / 10) * 100}%` }}
            onClick={() => onChange(value === t ? null : t)}
            onMouseEnter={() => setHover(t)}
          />
        ))}
      </div>
      <span className="ml-1 text-xs text-[#5a6b7c]">
        {value === null ? "未评分" : value}
      </span>
    </div>
  );
}

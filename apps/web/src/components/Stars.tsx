import { useState } from "react";
function StarGlyphs({ value }: { value: number }) {
  // 逐星渲染：每颗星独立判断填充（0/半/满），不依赖容器宽度裁切
  return (
    <>
      {[0, 1, 2, 3, 4].map((i) => {
        const fill = value - i * 20; // 0..20
        return (
          <span key={i} className="relative inline-block w-[1em] text-center">
            <span className="text-[#3a4653]">★</span>
            <span
              className="absolute left-0 top-0 inline-block overflow-hidden text-[#00e054]"
              style={{
                width: fill >= 20 ? "100%" : fill >= 10 ? "50%" : "0%",
              }}
            >
              ★
            </span>
          </span>
        );
      })}
    </>
  );
}

export function StarsDisplay({ value }: { value: number | null }) {
  if (value === null) {
    return <span className="text-xs text-[#5a6b7c]">未评分</span>;
  }
  return (
    <span className="whitespace-nowrap">
      <StarGlyphs value={value} />
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
        <StarGlyphs value={shown} />
        {ticks.map((t, i) => (
          <span
            key={t}
            className="absolute top-0 h-full"
            style={{ left: `${(i / 10) * 100}%`, width: "10%" }}
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

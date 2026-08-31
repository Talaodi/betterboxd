import { useId, useState } from "react";

/** 五角星路径（24×24 viewBox） */
const STAR_D =
  "M12 2.4l2.92 5.92 6.53.95-4.72 4.6 1.11 6.5L12 17.35l-5.84 3.02 1.11-6.5-4.72-4.6 6.53-.95L12 2.4z";

/** 单颗星：灰底 + 绿色按比例裁切（SVG 裁切边缘干净，无字体半切毛刺） */
function StarSvg({ fill, uid, idx }: { fill: number; uid: string; idx: number }) {
  const clipId = `${uid}-s${idx}`;
  return (
    <svg width="1em" height="1em" viewBox="0 0 24 24" className="inline-block align-baseline">
      <path d={STAR_D} fill="#3a4653" />
      {fill > 0 && (
        <>
          <defs>
            <clipPath id={clipId}>
              <rect x="0" y="0" width={fill * 24} height="24" />
            </clipPath>
          </defs>
          <path d={STAR_D} fill="#00e054" clipPath={`url(#${clipId})`} />
        </>
      )}
    </svg>
  );
}

function StarGlyphs({ value }: { value: number }) {
  const uid = useId();
  return (
    <span className="whitespace-nowrap leading-none">
      {[0, 1, 2, 3, 4].map((i) => {
        const fill = Math.max(0, Math.min(1, (value - i * 20) / 20));
        return <StarSvg key={i} fill={fill} uid={uid} idx={i} />;
      })}
    </span>
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
    <div className="flex items-center gap-1 text-2xl">
      <div
        className="relative inline-block cursor-pointer"
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
      <input
        type="number"
        min={0}
        max={100}
        step={10}
        className="ml-2 w-16 rounded border border-[#33414f] bg-[#14181c] px-2 py-0.5 text-sm text-center"
        value={value === null ? "" : value}
        placeholder="未评分"
        onChange={(e) => {
          const n = Number(e.target.value);
          onChange(e.target.value === "" || isNaN(n) ? null : Math.min(100, Math.max(0, n)));
        }}
      />
    </div>
  );
}

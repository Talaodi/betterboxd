import { useState } from "react";
import { posterUrl } from "../api";

/** 海报卡：缺失时用"字母占位块"（片名首字 + 片名 hash 取色，LB 同款思路）。 */
export default function Poster({
  tmdbId,
  title,
  size = "thumb", // thumb(48×72) | grid(~150×225) | large(300×450)
  className = "",
}: {
  tmdbId: number;
  title: string;
  size?: "thumb" | "grid" | "large";
  className?: string;
}) {
  const [failed, setFailed] = useState(false);
  const dims =
    size === "thumb"
      ? "w-12 h-[72px]"
      : size === "grid"
        ? "w-full aspect-[2/3]"
        : "w-[300px] h-[450px]";
  const hue = [...title].reduce((a, c) => a + c.charCodeAt(0), 0) % 360;
  if (failed) {
    return (
      <div
        className={`${dims} ${className} flex items-center justify-center rounded text-2xl font-bold text-white/80`}
        style={{ background: `hsl(${hue} 30% 28%)` }}
      >
        {[...title][0] ?? "?"}
      </div>
    );
  }
  return (
    <img
      src={posterUrl(tmdbId)}
      alt={title}
      loading="lazy"
      onError={() => setFailed(true)}
      className={`${dims} ${className} rounded object-cover`}
    />
  );
}

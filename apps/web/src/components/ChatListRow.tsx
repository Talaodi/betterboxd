/** 会话列表行（Chats 页列表与影片 Log 聊段共用）。
 *  海报缩略（或控制台 B 头像）+ 标题 + 副题，点击打开对应会话。 */
import { useNavigate } from "react-router-dom";
import Poster from "./Poster";

export default function ChatListRow({
  sessionId,
  title,
  subtitle,
  tmdbId,
  movieTitle,
  selected = false,
  onClick,
}: {
  sessionId: string;
  title: string;
  /** 副题（Chats 页：主题+日期；Log 段：日期） */
  subtitle?: string;
  tmdbId?: number | null;
  movieTitle?: string | null;
  selected?: boolean;
  /** 默认行为：跳转 /chats?open=<id> */
  onClick?: () => void;
}) {
  const navigate = useNavigate();
  return (
    <button
      className={
        "flex w-full items-center gap-3 border-l-2 px-3 py-2.5 text-left hover:bg-[#1b222b] " +
        (selected ? "border-[#00e054] bg-[#1b222b]" : "border-transparent")
      }
      onClick={() => (onClick ? onClick() : navigate(`/chats?open=${sessionId}`))}
    >
      {tmdbId ? (
        <Poster tmdbId={tmdbId} title={movieTitle ?? ""} size="thumb" className="h-[42px] w-7 rounded" />
      ) : (
        <span className="inline-flex h-7 w-7 shrink-0 items-center justify-center rounded-full bg-[#00e054] text-[10px] font-bold text-[#0c1a10]">
          B
        </span>
      )}
      <span className="min-w-0 flex-1">
        <span className="block truncate text-sm">{title || "（无标题）"}</span>
        {subtitle && <span className="block truncate text-xs text-[#5a6b7c]">{subtitle}</span>}
      </span>
      <span className="shrink-0 text-xs text-[#40bcf4]">→</span>
    </button>
  );
}

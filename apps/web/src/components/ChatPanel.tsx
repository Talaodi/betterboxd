/** 共享聊天面板：控制台与详情页侧聊共用同一 UI（消息流/工具轨迹/确认卡/markdown）。
 *  传入 movieId 时挂到 movie 作用域 WS（服务端注入影片上下文），否则用 global。 */
import { useCallback, useEffect, useRef, useState } from "react";
import { useChat } from "../chatStore";
import ConfirmCard from "./ConfirmCard";
import ReactMarkdown from "react-markdown";
import remarkBreaks from "remark-breaks";
import remarkGfm from "remark-gfm";

/** markdown 渲染容器：GFM 表格 + 暗色主题样式（表格/代码块/链接）。
 *  remark-breaks：单换行即换行——存量纯文本随记（手写换行排版）不折叠。 */
export function Markdown({ children }: { children: string }) {
  const clean = children.replace(/\n{3,}/g, "\n\n");
  return (
    <div className="prose prose-invert prose-sm max-w-none [&_a]:text-[#40bcf4] [&_code]:rounded [&_code]:bg-[#14181c] [&_code]:px-1 [&_li]:mb-0.5 [&_p]:mb-1 [&_strong]:text-white [&_table]:my-2 [&_table]:w-full [&_table]:text-xs [&_table]:leading-5 [&_td]:border [&_td]:border-[#2c3440] [&_td]:px-2 [&_td]:py-1 [&_th]:border [&_th]:border-[#2c3440] [&_th]:bg-[#1b222b] [&_th]:px-2 [&_th]:py-1">
      <ReactMarkdown remarkPlugins={[remarkGfm, remarkBreaks]}>{clean}</ReactMarkdown>
    </div>
  );
}

/** 成本展示：0 计价时不显示数额曲线，直接 4 位小数。 */
function fmtCost(c: { value: number; currency: string }) {
  if (c.value <= 0) return `${c.currency} 0`;
  const v = c.value < 0.01 ? c.value.toFixed(4) : c.value.toFixed(3);
  return `${c.currency} ${v}`;
}

/** 工具小图标映射（1c：Calling 前加表示工具的小图标）。 */
const TOOL_ICONS: Record<string, string> = {
  search_movies: "🔍",
  search_person_movies: "🔍",
  get_movie_details: "🎬",
  get_movie_logs: "📋",
  lookup_diary: "📝",
  lookup_reviews: "📖",
  lookup_taxonomy: "🏷",
  lookup_chats: "💬",
  lookup_lists: "📚",
  run_stats: "📊",
  list_saved_queries: "📊",
  get_profile_snapshot: "🎯",
  manage_diary: "✏️",
  manage_reviews: "✏️",
  set_movie_state: "♥",
  manage_lists: "🗂",
  manage_saved_queries: "📌",
};

export default function ChatPanel({
  movieId,
  entryId,
  reviewId,
  movieTitle,
  sessionId,
  freshKey,
  onSettled,
  onDelete,
  header,
  placeholder = "和影迷助手聊聊…",
  hint,
  onHello,
}: {
  movieId?: number;
  entryId?: string;
  reviewId?: string;
  movieTitle?: string;
  /** 精确恢复指定会话（Chats 页打开历史会话） */
  sessionId?: string;
  /** 新建会话（Chats 页「新建控制台会话」）：连接 ?fresh=1 */
  freshKey?: string;
  /** 一轮流式结束（done/error）后回调（携带当前会话 id；Chats 页刷新列表/切换 open 模式） */
  onSettled?: (sessionId: string) => void;
  /** WS hello 帧后触发（会话已落盘）：Chats 页借此立即刷新列表（新会话即时出现） */
  onHello?: (sessionId: string) => void;
  /** 删除当前会话（Chats 页标题行渲染删除按钮） */
  onDelete?: () => void;
  /** 自定义标题行文案（默认 控制台/💬片名） */
  header?: string;
  placeholder?: string;
  hint?: string;
}) {
  const chat = useChat(
    movieId || entryId || reviewId || sessionId || freshKey
      ? { movieId, entryId, reviewId, sessionId, freshKey }
      : undefined,
  );
  const { messages, connected, streaming, opening, pendingConfirm, sendUser, interrupt, resolveConfirm } =
    chat;
  const [input, setInput] = useState("");

  // 流结束 → onSettled（ref 防回调身份变化触发 effect）
  const settledRef = useRef(onSettled);
  settledRef.current = onSettled;
  const helloRef = useRef(onHello);
  helloRef.current = onHello;
  const prevSid = useRef("");
  useEffect(() => {
    if (chat.sessionId && chat.sessionId !== prevSid.current) {
      prevSid.current = chat.sessionId;
      setTimeout(() => helloRef.current?.(chat.sessionId), 50);
    }
  }, [chat.sessionId]);
  const sidRef = useRef("");
  sidRef.current = chat.sessionId;
  const prevStreaming = useRef(false);
  useEffect(() => {
    if (prevStreaming.current && !streaming) settledRef.current?.(sidRef.current);
    prevStreaming.current = streaming;
  }, [streaming]);

  const send = useCallback(() => {
    const text = input.trim();
    if (!text || streaming || !connected || opening) return;
    // 未配置 AI：发出信息时弹窗报错（report b80caa5）
    fetch("/api/config")
      .then((r) => (r.ok ? r.json() : null))
      .then((cfg) => {
        const p = cfg?.profiles?.find((x: { name?: string }) => x.name === cfg.active_profile);
        if (!p || !p.endpoint || !p.api_key || !p.model) {
          alert("尚未配置 AI 模型：请先到「设置」配置端点和 API Key（新存档默认未配置）");
          return;
        }
        sendUser(text);
        setInput("");
      })
      .catch(() => {
        sendUser(text);
        setInput("");
      });
  }, [input, streaming, connected, opening, sendUser]);

  const bottomRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages.length]);

  return (
    <div className="flex h-full min-h-0 flex-col">
      {/* 标题行 */}
      <div className="flex items-baseline justify-between px-1 pb-2">
        <span className="text-sm font-medium tracking-wide text-[#8899aa]">
          {header ?? (movieTitle ? `💬 ${movieTitle}` : "控制台")}
        </span>
        <span className="flex items-baseline gap-3">
          {onDelete && (
            <button
              className="text-xs text-[#5a6b7c] hover:text-[#ff8000]"
              onClick={onDelete}
              title="删除此会话"
            >
              🗑 删除
            </button>
          )}
          <span className={"text-xs " + (connected ? "text-[#00e054]" : "text-[#ff8000]")}>
            {connected ? "● 已连接" : "○ 连接中"}
          </span>
        </span>
      </div>

      {/* 消息流 */}
      <div className="min-h-0 flex-1 space-y-3 overflow-y-auto rounded-lg border border-[#33414f] bg-[#1b222b] p-4">
        {messages.length === 0 && !opening && (
          <p className="text-sm text-[#5a6b7c]">
            {hint ?? '对助手说点什么："帮我搜一下盗梦空间"、"今年我看了多少电影"、"记一下：刚看了XXX，X分"。'}
          </p>
        )}
        {opening && messages.length === 0 && (
          <div className="animate-pulse pl-10 text-xs text-[#5a6b7c]">✱ Thinking…</div>
        )}
        {messages.map((m, i) => {
          // 工具调用进度行（无 B logo）
          if (m.role === "tool") {
            const icon = TOOL_ICONS[m.name] ?? "🛠";
            return (
              <div key={i} className="pl-10 text-xs text-[#5a6b7c]">
                {icon} {m.name} {m.ok === false ? "✗" : m.ok ? "✓" : "…"}
              </div>
            );
          }
          // 中间步骤：空文本=Thinking 占位行；有文本=直接按正式气泡渲染（流式增长，done 时不再刷新样式）
          if (m.role === "assistant" && m.final === false && !m.text.trim()) {
            const isLast = i === messages.length - 1;
            return (
              <div key={i} className={"pl-10 text-xs text-[#5a6b7c]" + (isLast && streaming ? " animate-pulse" : "")}>
                ✱ Thinking…
              </div>
            );
          }
          return (
            <div key={i} className={"flex " + (m.role === "user" ? "justify-end" : "items-start gap-2")}>
              {m.role === "assistant" && (
                <span className="mt-1 inline-flex h-6 w-6 shrink-0 items-center justify-center rounded-full bg-[#00e054] text-[11px] font-bold text-[#0c1a10]">B</span>
              )}
              <span
                className={
                  "inline-block max-w-[85%] whitespace-pre-wrap rounded-lg px-3 py-2 text-sm " +
                  (m.role === "user" ? "bg-[#2c3440]" : "bg-[#232b35]")
                }
              >
                {m.role === "assistant" ? (
                  <Markdown>{m.text}</Markdown>
                ) : (
                  m.text
                )}
                {m.role === "assistant" && m.usage && (
                  <span className="ml-2 text-[10px] text-[#5a6b7c]">
                    [{m.usage.prompt}+{m.usage.completion} tok{m.cost ? ` · ${fmtCost(m.cost)}` : ""}]
                  </span>
                )}
              </span>
            </div>
          );
        })}
        <div ref={bottomRef} />
      </div>

      {/* 确认卡 */}
      {pendingConfirm && (
        <div className="mt-2">
          <ConfirmCard
            key={pendingConfirm.call_id}
            pending={pendingConfirm}
            onConfirm={(args) => resolveConfirm("confirm", args)}
            onReject={() => resolveConfirm("reject")}
          />
        </div>
      )}

      {/* 输入行：Enter 发送, Shift+Enter 手动换行 */}
      {opening && (
        <p className="mt-1 text-xs text-[#5a6b7c]">助手正在开场…（可点「停止」打断）</p>
      )}
      <div className="mt-3 flex gap-2">
        <textarea
          className="flex-1 resize-none rounded-lg border border-[#33414f] bg-[#14181c] px-3 py-2 text-sm outline-none focus:border-[#40bcf4]"
          rows={Math.min(6, Math.max(2, input.split("\n").length))}
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && !e.shiftKey) {
              e.preventDefault();
              send();
            }
          }}
          placeholder={placeholder}
          disabled={streaming || !connected || opening}
        />
        {opening ? (
          <button
            className="rounded-lg bg-[#ff8000] px-4 py-2 text-sm font-medium text-[#1a1206]"
            onClick={interrupt}
          >
            停止
          </button>
        ) : streaming ? (
          <button
            className="rounded-lg bg-[#ff8000] px-4 py-2 text-sm font-medium text-[#1a1206]"
            onClick={interrupt}
          >
            停止
          </button>
        ) : (
          <button
            className="rounded-lg bg-[#00e054] px-4 py-2 text-sm font-medium text-[#0c1a10] disabled:opacity-40"
            onClick={send}
            disabled={!connected}
          >
            发送
          </button>
        )}
      </div>
    </div>
  );
}

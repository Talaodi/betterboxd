/** 共享聊天面板：控制台与详情页侧聊共用同一 UI（消息流/工具轨迹/确认卡/markdown）。
 *  传入 movieId 时挂到 movie 作用域 WS（服务端注入影片上下文），否则用 global。 */
import { useCallback, useEffect, useRef, useState } from "react";
import { useChat } from "../chatStore";
import ConfirmCard from "./ConfirmCard";
import ReactMarkdown from "react-markdown";

export default function ChatPanel({
  movieId,
  movieTitle,
  placeholder = "和影迷助手聊聊…",
  hint,
}: {
  movieId?: number;
  movieTitle?: string;
  placeholder?: string;
  hint?: string;
}) {
  const { messages, connected, streaming, pendingConfirm, sendUser, interrupt, resolveConfirm, newChat } =
    useChat(movieId ? { movieId } : undefined);
  const [input, setInput] = useState("");

  const send = useCallback(() => {
    const text = input.trim();
    if (!text || streaming || !connected) return;
    sendUser(text);
    setInput("");
  }, [input, streaming, connected, sendUser]);

  const bottomRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages.length]);

  return (
    <div className="flex h-full min-h-0 flex-col">
      {/* 标题行 */}
      <div className="flex items-baseline justify-between px-1 pb-2">
        <span className="text-sm font-medium tracking-wide text-[#8899aa]">
          {movieTitle ? `💬 ${movieTitle}` : "控制台"}
        </span>
        <span className="flex items-baseline gap-3">
          {!movieId && (
            <button
              className="text-xs text-[#5a6b7c] hover:text-[#40bcf4] disabled:opacity-40"
              onClick={newChat}
              disabled={streaming}
            >
              新对话
            </button>
          )}
          <span className={"text-xs " + (connected ? "text-[#00e054]" : "text-[#ff8000]")}>
            {connected ? "● 已连接" : "○ 连接中"}
          </span>
        </span>
      </div>

      {/* 消息流 */}
      <div className="min-h-0 flex-1 space-y-3 overflow-y-auto rounded-lg border border-[#33414f] bg-[#1b222b] p-4">
        {messages.length === 0 && (
          <p className="text-sm text-[#5a6b7c]">
            {hint ?? '和影迷助手聊聊："帮我搜一下盗梦空间"、"今年我看了多少电影"、"记一下：刚看了XXX，X分"。'}
          </p>
        )}
        {messages.map((m, i) =>
          m.role === "tool" ? (
            <div key={i} className="flex items-center gap-2 pl-10 text-xs text-[#5a6b7c]">
              <span className="inline-flex h-5 w-5 items-center justify-center rounded bg-[#00e054] text-[10px] font-bold text-[#0c1a10]">B</span>
              🔧 {m.name} {m.ok === false ? "✗" : m.ok ? "✓" : "…"}
            </div>
          ) : (
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
                  <div className="prose prose-invert prose-sm max-w-none [&_p]:mb-1 [&_strong]:text-white">
                    <ReactMarkdown>{m.text}</ReactMarkdown>
                  </div>
                ) : (
                  m.text
                )}
                {m.role === "assistant" && m.usage && (
                  <span className="ml-2 text-[10px] text-[#5a6b7c]">
                    [{m.usage.prompt}+{m.usage.completion} tok]
                  </span>
                )}
              </span>
            </div>
          ),
        )}
        <div ref={bottomRef} />
      </div>

      {/* 确认卡 */}
      {pendingConfirm && (
        <div className="mt-2">
          <ConfirmCard
            pending={pendingConfirm}
            onConfirm={(args) => resolveConfirm("confirm", args)}
            onReject={() => resolveConfirm("reject")}
          />
        </div>
      )}

      {/* 输入行 */}
      <div className="mt-3 flex gap-2">
        <input
          className="flex-1 rounded-lg border border-[#33414f] bg-[#14181c] px-3 py-2 text-sm outline-none focus:border-[#40bcf4]"
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && send()}
          placeholder={placeholder}
          disabled={streaming || !connected}
        />
        {streaming ? (
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

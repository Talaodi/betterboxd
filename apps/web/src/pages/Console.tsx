import { useCallback, useEffect, useRef, useState } from "react";
import { useChat } from "../chatStore";
import ConfirmCard from "../components/ConfirmCard";

/** 控制台（默认页；AI 对话页的 global 实例）。
 *  WS 与会话状态由 chatStore 单例持有——切页不断连接、不丢确认卡。 */
export default function Console() {
  const { messages, connected, streaming, pendingConfirm, sendUser, interrupt, resolveConfirm } =
    useChat();
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
    <div className="flex h-full flex-col items-center px-6 pt-6">
      <div className="flex w-full max-w-[760px] flex-1 flex-col">
        <div className="mb-4 flex items-baseline justify-between">
          <h1 className="text-lg tracking-wide text-[#8899aa]">控制台</h1>
          <span className={"text-xs " + (connected ? "text-[#00e054]" : "text-[#ff8000]")}>
            {connected ? "● 已连接" : "○ 连接中"}
          </span>
        </div>
        <div className="mb-4 flex-1 space-y-3 overflow-y-auto rounded-lg border border-[#33414f] bg-[#1b222b] p-4">
          {messages.length === 0 && (
            <p className="text-sm text-[#5a6b7c]">
              和影迷助手聊聊："帮我搜一下盗梦空间"、"今年我看了多少电影"、"记一下：刚看了XXX，X分"。
            </p>
          )}
          {messages.map((m, i) =>
            m.role === "tool" ? (
              <div key={i} className="text-xs text-[#5a6b7c]">
                🔧 {m.name} {m.ok === false ? "✗" : m.ok ? "✓" : "…"}
              </div>
            ) : (
              <div key={i} className={m.role === "user" ? "text-right" : "text-left"}>
                <span
                  className={
                    "inline-block max-w-[85%] whitespace-pre-wrap rounded-lg px-3 py-2 text-sm " +
                    (m.role === "user" ? "bg-[#2c3440]" : "bg-[#232b35]")
                  }
                >
                  {m.text}
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
        {pendingConfirm && (
          <ConfirmCard
            pending={pendingConfirm}
            onConfirm={(args) => resolveConfirm("confirm", args)}
            onReject={() => resolveConfirm("reject")}
          />
        )}
        <div className="mb-6 mt-4 flex gap-2">
          <input
            className="flex-1 rounded-lg border border-[#33414f] bg-[#14181c] px-3 py-2 text-sm outline-none focus:border-[#40bcf4]"
            value={input}
            onChange={(e) => setInput(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && send()}
            placeholder="和影迷助手聊聊…"
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
    </div>
  );
}

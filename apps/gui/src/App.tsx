import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

/**
 * M0 验收：控制台发出一条消息，经 core 处理后以"假流式"逐字回显。
 * 完整链路：前端 invoke → core(异步任务) → emit 事件 → 前端 listen 渲染。
 */
export default function App() {
  const [messages, setMessages] = useState<{ role: string; text: string }[]>([]);
  const [input, setInput] = useState("");
  const [streaming, setStreaming] = useState(false);
  const streamRef = useRef<string>("");
  const unlistenRef = useRef<UnlistenFn | null>(null);

  useEffect(() => {
    let mounted = true;
    listen<string>("chat://token", (ev) => {
      if (!mounted) return;
      streamRef.current += ev.payload;
      setMessages((ms) => {
        const last = ms[ms.length - 1];
        if (last?.role === "assistant") {
          return [...ms.slice(0, -1), { role: "assistant", text: streamRef.current }];
        }
        return [...ms, { role: "assistant", text: streamRef.current }];
      });
    }).then((un) => {
      unlistenRef.current = un;
    });
    return () => {
      mounted = false;
      unlistenRef.current?.();
    };
  }, []);

  const send = useCallback(() => {
    const text = input.trim();
    if (!text || streaming) return;
    setMessages((ms) => [...ms, { role: "user", text }]);
    setInput("");
    streamRef.current = "";
    setStreaming(true);
    invoke("chat_echo", { message: text })
      .catch((e) => console.error(e))
      .finally(() => setStreaming(false));
  }, [input, streaming]);

  return (
    <div className="flex h-full flex-col items-center justify-center px-6">
      <div className="w-full max-w-[760px]">
        <h1 className="mb-4 text-center text-lg tracking-wide text-[#8899aa]">
          Betterboxd · 控制台
        </h1>
        <div className="mb-4 h-80 space-y-3 overflow-y-auto rounded-lg border border-[#33414f] bg-[#1b222b] p-4">
          {messages.length === 0 && (
            <p className="text-sm text-[#5a6b7c]">
              输入任意消息，M0 验收将逐字回显（假流式，验证 IPC 链路）。
            </p>
          )}
          {messages.map((m, i) => (
            <div key={i} className={m.role === "user" ? "text-right" : "text-left"}>
              <span
                className={
                  "inline-block max-w-[85%] rounded-lg px-3 py-2 text-sm " +
                  (m.role === "user"
                    ? "bg-[#2c3440]"
                    : "bg-[#232b35] text-[#cfe8d8]")
                }
              >
                {m.text}
              </span>
            </div>
          ))}
        </div>
        <div className="flex gap-2">
          <input
            className="flex-1 rounded-lg border border-[#33414f] bg-[#14181c] px-3 py-2 text-sm outline-none focus:border-[#40bcf4]"
            value={input}
            onChange={(e) => setInput(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && send()}
            placeholder="随便说点什么…"
            disabled={streaming}
          />
          <button
            className="rounded-lg bg-[#00e054] px-4 py-2 text-sm font-medium text-[#0c1a10] disabled:opacity-40"
            onClick={send}
            disabled={streaming}
          >
            {streaming ? "…" : "发送"}
          </button>
        </div>
      </div>
    </div>
  );
}

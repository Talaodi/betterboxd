import { useCallback, useEffect, useRef, useState } from "react";

type Frame = {
  type: "hello" | "token" | "tool" | "tool_done" | "done" | "error";
  data?: string;
  name?: string;
  ok?: boolean;
  message?: string;
  session_id?: string;
  interrupted?: boolean;
  steps?: number;
  tokens?: { prompt: number; completion: number };
  aborted?: string | null;
};

type Msg =
  | { role: "user"; text: string }
  | { role: "assistant"; text: string; usage?: { prompt: number; completion: number } }
  | { role: "tool"; name: string; ok?: boolean };

/**
 * M1：控制台接真实模型（课程平台）。
 * WS 帧协议：token 流式文本 / tool 工具调用 / done 结束+用量 / error。
 */
export default function App() {
  const [messages, setMessages] = useState<Msg[]>([]);
  const [input, setInput] = useState("");
  const [connected, setConnected] = useState(false);
  const [streaming, setStreaming] = useState(false);
  const wsRef = useRef<WebSocket | null>(null);
  const accRef = useRef("");

  const flushAssistant = useCallback(() => {
    setMessages((ms) => {
      const last = ms[ms.length - 1];
      if (last?.role === "assistant") {
        return [...ms.slice(0, -1), { role: "assistant", text: accRef.current }];
      }
      return [...ms, { role: "assistant", text: accRef.current }];
    });
  }, []);

  useEffect(() => {
    const proto = location.protocol === "https:" ? "wss" : "ws";
    const ws = new WebSocket(`${proto}://${location.host}/ws/chat`);
    ws.onopen = () => setConnected(true);
    ws.onmessage = (ev) => {
      const f: Frame = JSON.parse(ev.data);
      switch (f.type) {
        case "token":
          accRef.current += f.data ?? "";
          flushAssistant();
          break;
        case "tool":
          setMessages((ms) => [...ms, { role: "tool", name: f.name ?? "?" }]);
          break;
        case "tool_done":
          setMessages((ms) => {
            for (let i = ms.length - 1; i >= 0; i--) {
              const m = ms[i];
              if (m.role === "tool" && m.name === f.name && !("ok" in m)) {
                const copy = [...ms];
                copy[i] = { ...m, ok: f.ok };
                return copy;
              }
            }
            return ms;
          });
          break;
        case "done":
          setStreaming(false);
          setMessages((ms) => {
            const last = ms[ms.length - 1];
            if (last?.role === "assistant" && f.tokens) {
              return [
                ...ms.slice(0, -1),
                { role: "assistant", text: accRef.current, usage: f.tokens },
              ];
            }
            return ms;
          });
          break;
        case "error":
          setStreaming(false);
          setMessages((ms) => [
            ...ms,
            { role: "assistant", text: `⚠ ${f.message}` },
          ]);
          break;
      }
    };
    ws.onclose = () => setConnected(false);
    wsRef.current = ws;
    return () => ws.close();
  }, [flushAssistant]);

  const send = useCallback(() => {
    const text = input.trim();
    if (!text || streaming || !connected) return;
    setMessages((ms) => [...ms, { role: "user", text }]);
    setInput("");
    accRef.current = "";
    setStreaming(true);
    wsRef.current?.send(JSON.stringify({ type: "user", text }));
  }, [input, streaming, connected]);

  const interrupt = useCallback(() => {
    wsRef.current?.send(JSON.stringify({ type: "interrupt" }));
  }, []);

  return (
    <div className="flex h-full flex-col items-center px-6 pt-6">
      <div className="flex w-full max-w-[760px] flex-1 flex-col">
        <div className="mb-4 flex items-baseline justify-between">
          <h1 className="text-lg tracking-wide text-[#8899aa]">Betterboxd · 控制台</h1>
          <span className={"text-xs " + (connected ? "text-[#00e054]" : "text-[#ff8000]")}>
            {connected ? "● 已连接" : "○ 连接中"}
          </span>
        </div>
        <div className="mb-4 flex-1 space-y-3 overflow-y-auto rounded-lg border border-[#33414f] bg-[#1b222b] p-4">
          {messages.length === 0 && (
            <p className="text-sm text-[#5a6b7c]">
              M1：对话已接入真实模型。试试“我看过哪些王家卫的电影”（需先有记录）。
            </p>
          )}
          {messages.map((m, i) =>
            m.role === "tool" ? (
              <div key={i} className="text-xs text-[#5a6b7c]">
                🔧 {m.name} {m.ok === false ? "✗" : "…"}
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
        </div>
        <div className="mb-6 flex gap-2">
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

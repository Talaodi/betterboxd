import { useCallback, useEffect, useRef, useState } from "react";

/**
 * M0 验收（WebUI 版）：控制台发消息 → 后端 WebSocket 逐块推送 → 逐字回显。
 * 完整链路：浏览器 WS → axum → 异步任务 → WS 帧 → 浏览器渲染。
 * M1 换成真实 Agent Loop 时，WS 帧协议（token/done）保持不变。
 */
export default function App() {
  const [messages, setMessages] = useState<{ role: string; text: string }[]>([]);
  const [input, setInput] = useState("");
  const [connected, setConnected] = useState(false);
  const [streaming, setStreaming] = useState(false);
  const wsRef = useRef<WebSocket | null>(null);
  const streamRef = useRef("");

  const appendAssistant = useCallback((piece: string) => {
    streamRef.current += piece;
    setMessages((ms) => {
      const last = ms[ms.length - 1];
      if (last?.role === "assistant") {
        return [...ms.slice(0, -1), { role: "assistant", text: streamRef.current }];
      }
      return [...ms, { role: "assistant", text: streamRef.current }];
    });
  }, []);

  useEffect(() => {
    const proto = location.protocol === "https:" ? "wss" : "ws";
    const ws = new WebSocket(`${proto}://${location.host}/ws/echo`);
    ws.onopen = () => setConnected(true);
    ws.onmessage = (ev) => {
      const frame = JSON.parse(ev.data);
      if (frame.type === "token") appendAssistant(frame.data);
      if (frame.type === "done") setStreaming(false);
    };
    ws.onclose = () => setConnected(false);
    wsRef.current = ws;
    return () => ws.close();
  }, [appendAssistant]);

  const send = useCallback(() => {
    const text = input.trim();
    if (!text || streaming || !connected) return;
    setMessages((ms) => [...ms, { role: "user", text }]);
    setInput("");
    streamRef.current = "";
    setStreaming(true);
    wsRef.current?.send(text);
  }, [input, streaming, connected]);

  return (
    <div className="flex h-full flex-col items-center justify-center px-6">
      <div className="w-full max-w-[760px]">
        <div className="mb-4 flex items-baseline justify-between">
          <h1 className="text-lg tracking-wide text-[#8899aa]">Betterboxd · 控制台</h1>
          <span
            className={"text-xs " + (connected ? "text-[#00e054]" : "text-[#ff8000]")}
          >
            {connected ? "● 已连接" : "○ 连接中"}
          </span>
        </div>
        <div className="mb-4 h-80 space-y-3 overflow-y-auto rounded-lg border border-[#33414f] bg-[#1b222b] p-4">
          {messages.length === 0 && (
            <p className="text-sm text-[#5a6b7c]">
              输入任意消息，M0 验收将逐字回显（假流式，验证 WebSocket 链路）。
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
            disabled={streaming || !connected}
          />
          <button
            className="rounded-lg bg-[#00e054] px-4 py-2 text-sm font-medium text-[#0c1a10] disabled:opacity-40"
            onClick={send}
            disabled={streaming || !connected}
          >
            {streaming ? "…" : "发送"}
          </button>
        </div>
      </div>
    </div>
  );
}

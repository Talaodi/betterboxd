/**
 * 聊天连接单例：WS 生命周期与页面路由解耦（重构 3）。
 * Console 组件只是订阅者——切页不断连接、不丢确认卡、不丢进行中的流。
 */
import { useCallback, useEffect, useState } from "react";

type Frame = {
  type: "hello" | "token" | "tool" | "tool_done" | "confirm" | "done" | "error";
  data?: string;
  name?: string;
  ok?: boolean;
  call_id?: string;
  args?: Record<string, unknown>;
  message?: string;
  session_id?: string;
  interrupted?: boolean;
  steps?: number;
  tokens?: { prompt: number; completion: number };
};

export type Msg =
  | { role: "user"; text: string }
  | { role: "assistant"; text: string; usage?: { prompt: number; completion: number } }
  | { role: "tool"; name: string; ok?: boolean };

export type Pending = {
  call_id: string;
  name: string;
  args: Record<string, unknown>;
  movieTitle?: string;
};

// ===== 模块级单例状态 =====
let ws: WebSocket | null = null;
let connected = false;
let streaming = false;
let messages: Msg[] = [];
let acc = "";
let pendingConfirm: Pending | null = null;
let helloSessionId = "";

type Listener = () => void;
const listeners = new Set<Listener>();
const notify = () => listeners.forEach((f) => f());

function handleFrame(f: Frame) {
  switch (f.type) {
    case "hello":
      helloSessionId = f.session_id ?? "";
      break;
    case "token":
      acc += f.data ?? "";
      flushAssistant();
      break;
    case "tool":
      messages = [...messages, { role: "tool", name: f.name ?? "?" }];
      break;
    case "tool_done":
      for (let i = messages.length - 1; i >= 0; i--) {
        const m = messages[i];
        if (m.role === "tool" && m.name === f.name && m.ok === undefined) {
          const copy = [...messages];
          const mm = copy[i] as { role: "tool"; name: string; ok?: boolean };
          copy[i] = { ...mm, ok: f.ok };
          messages = copy;
          break;
        }
      }
      break;
    case "confirm": {
      const args = (f.args ?? {}) as Record<string, unknown>;
      pendingConfirm = { call_id: f.call_id ?? "", name: f.name ?? "", args };
      const mid = args["movie_id"];
      if (typeof mid === "number") {
        fetch(`/api/movie/${mid}`)
          .then((r) => (r.ok ? r.json() : null))
          .then((d) => {
            const t = d?.movie?.title_main;
            if (t && pendingConfirm) {
              pendingConfirm = { ...pendingConfirm, movieTitle: t };
              notify();
            }
          })
          .catch(() => {});
      }
      break;
    }
    case "done":
      streaming = false;
      if (f.tokens) {
        const last = messages[messages.length - 1];
        if (last?.role === "assistant") {
          messages = [
            ...messages.slice(0, -1),
            { role: "assistant", text: acc, usage: f.tokens },
          ];
        }
      }
      break;
    case "error":
      streaming = false;
      messages = [...messages, { role: "assistant", text: `⚠ ${f.message}` }];
      break;
  }
  notify();
}

function flushAssistant() {
  const last = messages[messages.length - 1];
  if (last?.role === "assistant") {
    messages = [...messages.slice(0, -1), { role: "assistant", text: acc }];
  } else {
    messages = [...messages, { role: "assistant", text: acc }];
  }
}

function connect() {
  if (ws && ws.readyState <= WebSocket.OPEN) return;
  const proto = location.protocol === "https:" ? "wss" : "ws";
  ws = new WebSocket(`${proto}://${location.host}/ws/chat`);
  ws.onopen = () => {
    connected = true;
    notify();
  };
  ws.onmessage = (ev) => handleFrame(JSON.parse(ev.data));
  ws.onclose = () => {
    connected = false;
    streaming = false;
    notify();
    // 断线自动重连（页面刷新后恢复）
    setTimeout(connect, 1500);
  };
}

// ===== 对外 API =====
export function useChat() {
  const [, force] = useState(0);
  useEffect(() => {
    const l = () => force((n) => n + 1);
    listeners.add(l);
    connect();
    return () => {
      listeners.delete(l);
    };
  }, []);

  const sendUser = useCallback((text: string) => {
    if (!ws || ws.readyState !== WebSocket.OPEN || streaming) return;
    messages = [...messages, { role: "user", text }];
    acc = "";
    streaming = true;
    pendingConfirm = null;
    ws.send(JSON.stringify({ type: "user", text }));
    notify();
  }, [streaming]);

  const interrupt = useCallback(() => {
    ws?.send(JSON.stringify({ type: "interrupt" }));
    pendingConfirm = null;
    notify();
  }, []);

  const resolveConfirm = useCallback(
    (decision: "confirm" | "reject", args?: Record<string, unknown>) => {
      if (!pendingConfirm) return;
      ws?.send(
        JSON.stringify({
          type: "confirm",
          call_id: pendingConfirm.call_id,
          decision,
          args,
        }),
      );
      pendingConfirm = null;
      notify();
    },
    [],
  );

  return {
    messages,
    connected,
    streaming,
    pendingConfirm,
    sessionId: helloSessionId,
    sendUser,
    interrupt,
    resolveConfirm,
  };
}

/**
 * 聊天连接单例：WS 生命周期与页面路由解耦（重构 3 + M2d 扩展）。
 * - global 作用域：Console 使用，切页不断连接、不丢确认卡、不丢进行中的流。
 * - movie:N 作用域：详情页侧聊使用，按影片缓存消息；无人订阅且空闲时断开 WS。
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

type Store = {
  ws: WebSocket | null;
  connected: boolean;
  streaming: boolean;
  messages: Msg[];
  acc: string;
  pendingConfirm: Pending | null;
  sessionId: string;
  listeners: Set<() => void>;
};

const stores = new Map<string, Store>();
const refCount = new Map<string, number>();

function storeKey(movieId?: number) {
  return movieId ? `movie:${movieId}` : "global";
}

function wsUrl(movieId?: number) {
  const proto = location.protocol === "https:" ? "wss" : "ws";
  return `${proto}://${location.host}/ws/chat${movieId ? `?movie_id=${movieId}` : ""}`;
}

function getStore(key: string): Store {
  let s = stores.get(key);
  if (!s) {
    s = {
      ws: null,
      connected: false,
      streaming: false,
      messages: [],
      acc: "",
      pendingConfirm: null,
      sessionId: "",
      listeners: new Set(),
    };
    stores.set(key, s);
  }
  return s;
}

const notify = (s: Store) => s.listeners.forEach((f) => f());

function handleFrame(s: Store, key: string, f: Frame) {
  switch (f.type) {
    case "hello":
      s.sessionId = f.session_id ?? "";
      break;
    case "token":
      s.acc += f.data ?? "";
      flushAssistant(s);
      break;
    case "tool":
      s.messages = [...s.messages, { role: "tool", name: f.name ?? "?" }];
      break;
    case "tool_done":
      for (let i = s.messages.length - 1; i >= 0; i--) {
        const m = s.messages[i];
        if (m.role === "tool" && m.name === f.name && m.ok === undefined) {
          const copy = [...s.messages];
          const mm = copy[i] as { role: "tool"; name: string; ok?: boolean };
          copy[i] = { ...mm, ok: f.ok };
          s.messages = copy;
          break;
        }
      }
      break;
    case "confirm": {
      const args = (f.args ?? {}) as Record<string, unknown>;
      s.pendingConfirm = { call_id: f.call_id ?? "", name: f.name ?? "", args };
      const mid = args["movie_id"];
      if (typeof mid === "number") {
        fetch(`/api/movie/${mid}`)
          .then((r) => (r.ok ? r.json() : null))
          .then((d) => {
            const t = d?.movie?.title_main;
            if (t && s.pendingConfirm) {
              s.pendingConfirm = { ...s.pendingConfirm, movieTitle: t };
              notify(s);
            }
          })
          .catch(() => {});
      }
      break;
    }
    case "done":
      s.streaming = false;
      if (f.tokens) {
        const last = s.messages[s.messages.length - 1];
        if (last?.role === "assistant") {
          s.messages = [
            ...s.messages.slice(0, -1),
            { role: "assistant", text: s.acc, usage: f.tokens },
          ];
        }
      }
      maybeClose(key);
      break;
    case "error":
      s.streaming = false;
      s.messages = [...s.messages, { role: "assistant", text: `⚠ ${f.message}` }];
      maybeClose(key);
      break;
  }
  notify(s);
}

function flushAssistant(s: Store) {
  const last = s.messages[s.messages.length - 1];
  if (last?.role === "assistant") {
    s.messages = [...s.messages.slice(0, -1), { role: "assistant", text: s.acc }];
  } else {
    s.messages = [...s.messages, { role: "assistant", text: s.acc }];
  }
}

function connect(key: string, movieId?: number) {
  const s = getStore(key);
  if (s.ws && s.ws.readyState <= WebSocket.OPEN) return;
  const ws = new WebSocket(wsUrl(movieId));
  s.ws = ws;
  ws.onopen = () => {
    s.connected = true;
    notify(s);
  };
  ws.onmessage = (ev) => handleFrame(s, key, JSON.parse(ev.data));
  ws.onclose = () => {
    s.connected = false;
    s.streaming = false;
    notify(s);
    // 仍有人订阅时断线自动重连；movie 作用域无人订阅则不再连
    if ((refCount.get(key) ?? 0) > 0) setTimeout(() => connect(key, movieId), 1500);
    else s.ws = null;
  };
}

/** 无人订阅且不在流式中时关闭 WS（movie 作用域防连接泄漏）。 */
function maybeClose(key: string) {
  const s = stores.get(key);
  if (!s) return;
  if ((refCount.get(key) ?? 0) > 0 || s.streaming) return;
  if (key === "global") return; // global 常驻（切页不断连）
  s.ws?.close();
  s.ws = null;
}

// ===== 对外 API =====
export function useChat(scope?: { movieId?: number }) {
  const key = storeKey(scope?.movieId);
  const [, force] = useState(0);
  useEffect(() => {
    refCount.set(key, (refCount.get(key) ?? 0) + 1);
    const l = () => force((n) => n + 1);
    const s = getStore(key);
    s.listeners.add(l);
    connect(key, scope?.movieId);
    return () => {
      refCount.set(key, Math.max(0, (refCount.get(key) ?? 1) - 1));
      s.listeners.delete(l);
      maybeClose(key);
    };
  }, [key]);

  const sendUser = useCallback(
    (text: string) => {
      const s = getStore(key);
      if (!s.ws || s.ws.readyState !== WebSocket.OPEN || s.streaming) return;
      s.messages = [...s.messages, { role: "user", text }];
      s.acc = "";
      s.streaming = true;
      s.pendingConfirm = null;
      s.ws.send(JSON.stringify({ type: "user", text }));
      notify(s);
    },
    [key],
  );

  const interrupt = useCallback(() => {
    const s = getStore(key);
    s.ws?.send(JSON.stringify({ type: "interrupt" }));
    s.pendingConfirm = null;
    notify(s);
  }, [key]);

  const resolveConfirm = useCallback(
    (decision: "confirm" | "reject", args?: Record<string, unknown>) => {
      const s = getStore(key);
      if (!s.pendingConfirm) return;
      s.ws?.send(
        JSON.stringify({
          type: "confirm",
          call_id: s.pendingConfirm.call_id,
          decision,
          args,
        }),
      );
      s.pendingConfirm = null;
      notify(s);
    },
    [key],
  );

  const s = getStore(key);
  return {
    messages: s.messages,
    connected: s.connected,
    streaming: s.streaming,
    pendingConfirm: s.pendingConfirm,
    sessionId: s.sessionId,
    sendUser,
    interrupt,
    resolveConfirm,
  };
}

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
  tokens?: { prompt: number; completion: number; hit?: number; miss?: number };
  cost?: { value: number; currency: string };
  opening?: boolean;
};

export type Msg =
  | { role: "user"; text: string }
  | {
      role: "assistant";
      text: string;
      usage?: { prompt: number; completion: number; hit?: number; miss?: number };
      cost?: { value: number; currency: string };
      /** false=流式/中间步骤（渲染为进度行），true/undefined=正式回复（气泡+B logo） */
      final?: boolean;
    }
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
  /** fresh 重连标记：newChat 后首次重连仍走 fresh=1，hello 后复位 */
  freshPending: boolean;
  /** 最近一次连接使用的 scope（freshPending 重连时复用） */
  lastScope?: ChatScope;
  /** 开场白阶段（P4）：fresh 连接 hello 后、首个 done 前；期间禁发消息防丢 */
  opening: boolean;
  /** 最后一轮 done 的成本信息（done 帧 cost 字段） */
  lastCost?: { value: number; currency: string };
  listeners: Set<() => void>;
};

const stores = new Map<string, Store>();
const refCount = new Map<string, number>();

export type ChatScope = {
  /** movie 作用域：连接 /ws/chat?movie_id=N（恢复该影片最近会话） */
  movieId?: number;
  /** 讨论某 Diary 条目（P4.1）：新建注入条目上下文的会话 */
  entryId?: string;
  /** 讨论某影评（P4.1）：新建注入影评上下文的会话 */
  reviewId?: string;
  /** 精确恢复指定会话（Chats 页打开历史会话） */
  sessionId?: string;
  /** 新建会话的临时存储键（Chats 页「新建控制台会话」） */
  freshKey?: string;
};

function storeKey(scope?: ChatScope) {
  if (!scope) return "global";
  if (scope.freshKey) return `fresh:${scope.freshKey}`;
  if (scope.sessionId) return `session:${scope.sessionId}`;
  if (scope.entryId) return `entry:${scope.entryId}`;
  if (scope.reviewId) return `review:${scope.reviewId}`;
  if (scope.movieId) return `movie:${scope.movieId}`;
  return "global";
}

function getStore(key: string): Store {
  let s: Store | undefined = stores.get(key);
  if (!s) {
    s = {
      ws: null,
      connected: false,
      streaming: false,
      messages: [],
      acc: "",
      pendingConfirm: null,
      sessionId: "",
      freshPending: false,
      opening: false,
      lastCost: undefined,
      listeners: new Set(),
    };
    stores.set(key, s);
  }
  return s;
}

const notify = (s: Store) => s.listeners.forEach((f) => f());

function handleFrame(s: Store, key: string, f: Frame) {
  switch (f.type) {
    case "hello": {
      s.sessionId = f.session_id ?? "";
      s.freshPending = false;
      // 开场白状态以服务端标记为准（fresh 新建=会发；恢复=不发）
      s.opening = f.opening ?? false;
      // 历史恢复：内存为空时拉取该会话消息（刷新/重开抽屉后回填）
      if (s.sessionId && s.messages.length === 0) {
        fetch(`/api/chats/${s.sessionId}`)
          .then((r) => (r.ok ? r.json() : null))
          .then((d) => {
            if (!d || s.messages.length > 0 || s.streaming) return;
            const hist: Msg[] = (d.messages ?? [])
              .map((m: { role: string; text?: string; name?: string; usage?: Msg & { usage?: unknown }; cost?: Msg }) =>
                m.role === "tool"
                  ? ({ role: "tool", name: m.name ?? "tool" } as Msg)
                  : m.role === "user"
                    ? ({ role: "user", text: m.text ?? "" } as Msg)
                    : ({
                        role: "assistant",
                        text: m.text ?? "",
                        final: true,
                        usage: m.usage as never,
                        cost: m.cost as never,
                      } as Msg),
              );
            if (hist.length) {
              s.messages = hist;
              notify(s);
            }
          })
          .catch(() => {});
      }
      break;
    }
    case "token":
      s.acc += f.data ?? "";
      flushAssistant(s);
      break;
    case "tool": {
      // 关键：跨 agent 步骤清空累积缓冲——否则下一步 token 会把上一步文本再拼一遍
      s.acc = "";
      // 移除尾部残留的 Thinking 空占位（tool_done 时插入；有 tool 帧说明进入下一步）
      let msgs = s.messages;
      while (msgs.length > 0) {
        const last = msgs[msgs.length - 1];
        if (last.role === "assistant" && last.final === false && !last.text.trim()) {
          msgs = msgs.slice(0, -1);
        } else break;
      }
      s.messages = [...msgs, { role: "tool", name: f.name ?? "?" }];
      break;
    }
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
      // 工具完成后模型还需思考下一步：插入 Thinking 占位（500ms 后不首 token 也持续显示）
      {
        const last = s.messages[s.messages.length - 1];
        if (!(last?.role === "assistant" && last.final === false)) {
          s.messages = [...s.messages, { role: "assistant", text: "", final: false }];
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
      s.opening = false;
      s.lastCost = f.cost;
      {
        // 评审缺陷 24：打断/无文本时也要收口——空进度行丢弃，有文本转正式气泡
        const last = s.messages[s.messages.length - 1];
        if (last?.role === "assistant" && last.final === false) {
          if (s.acc) {
            s.messages = [
              ...s.messages.slice(0, -1),
              { role: "assistant", text: s.acc, usage: f.tokens, cost: f.cost, final: true },
            ];
          } else {
            s.messages = s.messages.slice(0, -1);
          }
        }
      }
      maybeClose(key);
      break;
    case "error":
      s.streaming = false;
      s.opening = false;
      s.messages = [
        ...s.messages,
        { role: "assistant", text: `⚠ ${f.message}`, final: true },
      ];
      maybeClose(key);
      break;
  }
  notify(s);
}

function flushAssistant(s: Store) {
  const last = s.messages[s.messages.length - 1];
  if (last?.role === "assistant" && last.final === false) {
    // 当前步骤的流式消息，就地增长
    s.messages = [...s.messages.slice(0, -1), { role: "assistant", text: s.acc, final: false }];
  } else if (last?.role !== "assistant") {
    // 新步骤：开一条新的流式消息
    s.messages = [...s.messages, { role: "assistant", text: s.acc, final: false }];
  }
}

function connect(key: string, scope?: ChatScope) {
  const s = getStore(key);
  if (s.ws && s.ws.readyState <= WebSocket.OPEN) return;
  s.lastScope = scope;
  const proto = location.protocol === "https:" ? "wss" : "ws";
  let qs = "";
  if (scope?.sessionId) {
    qs = `session_id=${scope.sessionId}`;
  } else {
    // movie_id/entry_id/review_id 与 fresh=1 可组合（新建 movie 系会话服务端主动开场白）
    const p: string[] = [];
    if (scope?.movieId) p.push(`movie_id=${scope.movieId}`);
    if (scope?.entryId) p.push(`entry_id=${encodeURIComponent(scope.entryId)}`);
    if (scope?.reviewId) p.push(`review_id=${encodeURIComponent(scope.reviewId)}`);
    if (scope?.freshKey) p.push("fresh=1");
    qs = p.join("&");
  }
  const ws = new WebSocket(`${proto}://${location.host}/ws/chat${qs ? `?${qs}` : ""}`);
  s.ws = ws;
  ws.onopen = () => {
    s.connected = true;
    notify(s);
  };
  ws.onmessage = (ev) => handleFrame(s, key, JSON.parse(ev.data));
  ws.onclose = () => {
    if (s.ws !== ws) return; // 已被新连接替换（新对话等），旧 socket 事件忽略
    s.ws = null;
    s.connected = false;
    s.streaming = false;
    notify(s);
    // 仍有人订阅时断线自动重连
    if ((refCount.get(key) ?? 0) > 0) {
      setTimeout(() => connect(key, scopeForReconnect(key, s)), 1500);
    }
  };
}

/** 断线重连用的作用域：hello 过就直接按 sessionId 恢复；否则从 key 反推。 */
function scopeForReconnect(key: string, s: Store): ChatScope | undefined {
  if (s.sessionId) return { sessionId: s.sessionId };
  if (s.freshPending && s.lastScope) return s.lastScope; // fresh 建连阶段重试
  const [kind, ...rest] = key.split(":");
  const id = rest.join(":");
  if (kind === "movie") return { movieId: Number(id) };
  if (kind === "entry") return { entryId: id };
  if (kind === "review") return { reviewId: id };
  if (kind === "session") return { sessionId: id };
  return undefined;
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
export function useChat(scope?: ChatScope) {
  const key = storeKey(scope);
  const scopeRef = scope;
  const [, force] = useState(0);
  useEffect(() => {
    refCount.set(key, (refCount.get(key) ?? 0) + 1);
    const l = () => force((n) => n + 1);
    const s = getStore(key);
    s.listeners.add(l);
    connect(key, scopeRef);
    return () => {
      refCount.set(key, Math.max(0, (refCount.get(key) ?? 1) - 1));
      s.listeners.delete(l);
      maybeClose(key);
    };
  }, [key]);

  const sendUser = useCallback(
    (text: string) => {
      const s = getStore(key);
      if (!s.ws || s.ws.readyState !== WebSocket.OPEN || s.streaming || s.opening) return;
      s.messages = [...s.messages, { role: "user", text }, { role: "assistant", text: "", final: false }];
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
    opening: s.opening,
    lastCost: s.lastCost,
    pendingConfirm: s.pendingConfirm,
    sessionId: s.sessionId,
    sendUser,
    interrupt,
    resolveConfirm,
  };
}

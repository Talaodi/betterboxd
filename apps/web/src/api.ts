/** 后端 REST 客户端（前端零业务逻辑，数据均由 Rust 端产出）。 */

export async function getJson<T>(url: string): Promise<T> {
  const r = await fetch(url);
  if (!r.ok) throw new Error(`${r.status} ${await r.text()}`);
  return r.json();
}

export async function sendJson<T>(
  url: string,
  method: string,
  body?: unknown,
): Promise<T> {
  const r = await fetch(url, {
    method,
    headers: body ? { "Content-Type": "application/json" } : undefined,
    body: body ? JSON.stringify(body) : undefined,
  });
  if (!r.ok) throw new Error(`${r.status} ${await r.text()}`);
  return r.json();
}

export type MovieRow = {
  tmdb_id: number;
  title_main: string;
  title_sub: string;
  year: string | null;
  my_rating: number | null;
  liked: number;
  in_watchlist: number;
  watched: number;
  posters: string;
  overview?: string;
  tagline?: string;
  runtime?: number | null;
  genres?: string;
  directors?: string;
  release_date?: string;
  lb_rating?: number | null;
  lb_votes?: number | null;
};

export type DiaryRow = {
  entry_id: string;
  movie_id: number;
  title_main: string;
  title_sub: string;
  watched_date: string;
  rating: number | null;
  liked: number;
  in_theater: number;
  ticket_price_cents: number | null;
  private_note: string;
  rewatch_index: number;
  tags: string; // JSON 数组
  dimensions_flat: string; // JSON 数组 [{dimension,name}]
};

export type LogRow = { kind: string; id: string; at: string; brief: string };

export const posterUrl = (tmdbId: number) => `/api/poster/${tmdbId}`;

/** 解析 JSON 字符串列（解析失败返回空数组） */
export function parseJsonArray(s: string | null | undefined): unknown[] {
  if (!s) return [];
  try {
    const v = JSON.parse(s);
    return Array.isArray(v) ? v : [];
  } catch {
    return [];
  }
}

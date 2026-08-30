//! TMDB v3 客户端：api_key 查询参数鉴权（spike③ 结论）、环境代理自动生效、
//! 实例内 250ms 限速。

use serde_json::Value;

#[derive(Debug, Clone)]
pub struct TmdbClient {
    http: reqwest::Client,
    key: String,
    language: String,
    last_request: std::sync::Arc<std::sync::Mutex<Option<std::time::Instant>>>,
}

impl TmdbClient {
    pub fn new(key: String, proxy: Option<String>, language: String) -> Self {
        let mut builder = reqwest::Client::builder();
        if let Some(p) = proxy {
            builder = builder.proxy(reqwest::Proxy::all(&p).expect("代理地址无效"));
        }
        Self {
            http: builder.build().expect("HTTP 客户端构建失败"),
            key,
            language,
            last_request: std::sync::Arc::new(std::sync::Mutex::new(None)),
        }
    }

    /// 实例内 250ms 限速（design.md §6）。
    async fn throttle(&self) {
        const INTERVAL: std::time::Duration = std::time::Duration::from_millis(250);
        let wait = {
            let mut last = self.last_request.lock().unwrap();
            let wait = match *last {
                Some(t) => INTERVAL.checked_sub(t.elapsed()).unwrap_or_default(),
                None => std::time::Duration::ZERO,
            };
            *last = Some(std::time::Instant::now());
            wait
        };
        if !wait.is_zero() {
            tokio::time::sleep(wait).await;
        }
    }

    async fn get(&self, path: &str, query: &str) -> Result<Value, String> {
        self.throttle().await;
        let url = format!(
            "https://api.themoviedb.org/3{path}?api_key={}&language={}&{}",
            self.key, self.language, query
        );
        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("TMDB 网络错误: {e}"))?;
        let status = resp.status();
        let body: Value = resp
            .json()
            .await
            .map_err(|e| format!("TMDB 响应解析失败: {e}"))?;
        if !status.is_success() {
            // v3 兼容：状态信息可能在 body 里
            let msg = body["status_message"].as_str().unwrap_or("");
            return Err(format!("TMDB HTTP {status}: {msg}"));
        }
        Ok(body)
    }

    /// 搜索影片：返回前 5 条 {tmdb_id, title, original_title, year, overview, poster_path}
    pub async fn search_movie(&self, query: &str, year: Option<i64>) -> Result<Vec<Value>, String> {
        let q = urlencode(query);
        let mut query_str = format!("query={q}&include_adult=false");
        if let Some(y) = year {
            query_str.push_str(&format!("&primary_release_year={y}"));
        }
        let body = self.get("/search/movie", &query_str).await?;
        let mut out = Vec::new();
        for r in body["results"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .take(5)
        {
            out.push(serde_json::json!({
                "tmdb_id": r["id"],
                "title": r["title"],
                "original_title": r["original_title"],
                "year": r["release_date"].as_str().map(|s| &s[..4.min(s.len())]),
                "overview": r["overview"],
                "poster_path": r["poster_path"],
            }));
        }
        Ok(out)
    }

    /// 详情 + credits（一次请求拿导演/类型/时长等）。
    pub async fn movie_details(&self, tmdb_id: i64) -> Result<Value, String> {
        self.get(&format!("/movie/{tmdb_id}"), "append_to_response=credits")
            .await
    }
}

/// 最小 URL 编码（保留字母数字与 -_.~）。
pub fn urlencode(s: &str) -> String {
    let mut out = String::new();
    for b in s.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

//! TMDB v3 客户端：api_key 查询参数鉴权（spike③ 结论）、环境代理自动生效、
//! 实例内 250ms 限速。

use serde_json::Value;

#[derive(Debug, Clone)]
pub struct TmdbClient {
    http: reqwest::Client,
    key: String,
    language: String,
    /// 测试隔离（评审缺陷 6）：false 时所有请求直接报错，不发网络
    enabled: bool,
    last_request: std::sync::Arc<std::sync::Mutex<Option<std::time::Instant>>>,
}

impl TmdbClient {
    pub fn new(key: String, proxy: Option<String>, language: String) -> Self {
        let mut builder = reqwest::Client::builder()
            // 评审缺陷 6：JSON API 总超时 30s（无 SSE 需求）
            .connect_timeout(std::time::Duration::from_secs(10))
            .timeout(std::time::Duration::from_secs(30));
        if let Some(p) = proxy {
            builder = builder.proxy(reqwest::Proxy::all(&p).expect("代理地址无效"));
        }
        Self {
            http: builder.build().expect("HTTP 客户端构建失败"),
            key,
            language,
            enabled: true,
            last_request: std::sync::Arc::new(std::sync::Mutex::new(None)),
        }
    }

    /// 测试用禁用客户端：任何请求立即 Err。
    pub fn disabled(language: String) -> Self {
        Self {
            http: reqwest::Client::new(),
            key: String::new(),
            language,
            enabled: false,
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
        if !self.enabled {
            return Err("TMDB 已禁用（测试模式）".into());
        }
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

    /// 搜索影片：返回指定页全部结果（≤20 条）{tmdb_id, title, original_title, year,
    /// overview, poster_path}。page 从 1 起；前端「更多」= 页内切片，用尽再 page+1。
    pub async fn search_movie(&self, query: &str, year: Option<i64>, page: u32) -> Result<(Vec<Value>, u32), String> {
        let q = urlencode(query);
        let mut query_str = format!("query={q}&include_adult=false&page={page}");
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
        let total_pages = body["total_pages"].as_u64().unwrap_or(1).min(500) as u32;
        Ok((out, total_pages))
    }

    /// 人物搜索：返回最匹配的一个 (person_id, name)（无人命中 None）。
    pub async fn person_search(&self, query: &str) -> Result<Option<(i64, String)>, String> {
        let q = urlencode(query);
        let body = self.get("/search/person", &format!("query={q}&include_adult=false")).await?;
        Ok(body["results"].as_array()
            .and_then(|a| a.first())
            .map(|r| (r["id"].as_i64().unwrap_or(0), r["name"].as_str().unwrap_or("").to_string())))
    }

    /// 人物电影作品（cast+directing 合并去重，按 popularity 降序）：
    /// {tmdb_id, title, year, rating(10 分制), popularity, job}。
    pub async fn person_movie_credits(&self, person_id: i64) -> Result<Vec<Value>, String> {
        let body = self.get(&format!("/person/{person_id}/movie_credits"), "").await?;
        let mut out = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for (key, job) in [("cast", "出演"), ("crew", "导演")] {
            for r in body[key].as_array().cloned().unwrap_or_default() {
                if key == "crew" && r["job"].as_str() != Some("Director") {
                    continue;
                }
                let id = r["id"].as_i64().unwrap_or(0);
                if !seen.insert(id) {
                    continue;
                }
                out.push(serde_json::json!({
                    "tmdb_id": id,
                    "title": r["title"],
                    "year": r["release_date"].as_str().map(|s| &s[..4.min(s.len())]),
                    "rating": r["vote_average"],
                    "popularity": r["popularity"],
                    "job": job,
                }));
            }
        }
        out.sort_by(|a, b| {
            let pa = a["popularity"].as_f64().unwrap_or(0.0);
            let pb = b["popularity"].as_f64().unwrap_or(0.0);
            pb.partial_cmp(&pa).unwrap_or(std::cmp::Ordering::Equal)
        });
        Ok(out)
    }

    /// 详情 + credits（一次请求拿导演/类型/时长等）。
    pub async fn movie_details(&self, tmdb_id: i64) -> Result<Value, String> {
        self.get(&format!("/movie/{tmdb_id}"), "append_to_response=credits")
            .await
    }

    /// 指定语言的详情（简介统一英文时用 en-US）。走 get() 恢复 250ms 限速（评审缺陷 6）。
    pub async fn movie_details_in(&self, tmdb_id: i64, language: &str) -> Result<Value, String> {
        if !self.enabled {
            return Err("TMDB 已禁用（测试模式）".into());
        }
        self.throttle().await;
        let url = format!(
            "https://api.themoviedb.org/3/movie/{tmdb_id}?api_key={}&language={language}&append_to_response=credits",
            self.key
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
            let msg = body["status_message"].as_str().unwrap_or("");
            return Err(format!("TMDB HTTP {status}: {msg}"));
        }
        Ok(body)
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

impl TmdbClient {
    /// 下载海报原图（w500 档），返回字节（由调用方落盘 posters/）。
    pub async fn download_poster(&self, poster_path: &str) -> Result<Vec<u8>, String> {
        let url = format!("https://image.tmdb.org/t/p/w500{poster_path}");
        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("海报下载失败: {e}"))?;
        if !resp.status().is_success() {
            return Err(format!("海报下载 HTTP {}", resp.status()));
        }
        resp.bytes()
            .await
            .map(|b| b.to_vec())
            .map_err(|e| e.to_string())
    }
}

impl TmdbClient {
    /// 下载任意 TMDB 图片 URL（海报/剧照共用）。
    pub async fn download_image(&self, url: &str) -> Result<Vec<u8>, String> {
        let resp = self
            .http
            .get(url)
            .send()
            .await
            .map_err(|e| format!("图片下载失败: {e}"))?;
        if !resp.status().is_success() {
            return Err(format!("图片下载 HTTP {}", resp.status()));
        }
        resp.bytes()
            .await
            .map(|b| b.to_vec())
            .map_err(|e| e.to_string())
    }
}

//! spike③：TMDB 可达性 + Key 有效性。
//! 用法：`set -a; source spikes/local.env; cargo run -p betterboxd-core --bin spike_tmdb`

#[tokio::main]
async fn main() {
    let key = std::env::var("TMDB_KEY").expect("缺少 TMDB_KEY 环境变量");
    let url = "https://api.themoviedb.org/3/search/movie";
    let resp = reqwest::Client::new()
        .get(url)
        .query(&[("query", "Inception"), ("language", "zh-CN")])
        .header("Authorization", format!("Bearer {key}"))
        .send()
        .await
        .expect("请求失败（网络/代理）");
    let status = resp.status();
    let body: serde_json::Value = resp.json().await.unwrap_or_default();
    let first = body["results"][0]["title"].as_str().unwrap_or("(无结果)");
    println!("TMDB spike: HTTP {status}, 首条结果 = {first}");
    println!(
        "结论: {}",
        if status.is_success() {
            "✅ Key 有效且可达（注意：当前走代理则代理字段必要）"
        } else {
            "❌ Key 或网络问题，检查后重试"
        }
    );
}

//! spike③：TMDB 可达性 + Key 有效性。
//! 用法：`set -a; source spikes/local.env; cargo run -p betterboxd-core --bin spike_tmdb`

#[tokio::main]
async fn main() {
    let key = std::env::var("TMDB_KEY").expect("缺少 TMDB_KEY 环境变量");
    let base = "https://api.themoviedb.org/3/search/movie?query=Inception&language=zh-CN";

    let client = reqwest::Client::new();

    // v3 风格：api_key 查询参数
    let r1 = client
        .get(format!("{base}&api_key={key}"))
        .send()
        .await
        .expect("请求失败（网络/代理）");
    let s1 = r1.status();
    let v3_ok = s1.is_success();
    let first_v3 = r1.json::<serde_json::Value>().await.unwrap_or_default()["results"][0]["title"]
        .as_str()
        .unwrap_or("(无结果)")
        .to_string();

    // v4 风格：Bearer read access token
    let r2 = client
        .get(base)
        .header("Authorization", format!("Bearer {key}"))
        .send()
        .await
        .expect("请求失败（网络/代理）");
    let s2 = r2.status();
    let v4_ok = s2.is_success();
    let first_v4 = r2.json::<serde_json::Value>().await.unwrap_or_default()["results"][0]["title"]
        .as_str()
        .unwrap_or("(无结果)")
        .to_string();

    println!(
        "v3 api_key 参数: {} HTTP {s1} → {first_v3}",
        if v3_ok { "✅" } else { "❌" }
    );
    println!(
        "v4 Bearer token: {} HTTP {s2} → {first_v4}",
        if v4_ok { "✅" } else { "❌" }
    );
    println!(
        "结论: {}",
        if v3_ok {
            "✅ Key 有效（v3 鉴权：api_key 查询参数，客户端将按此实现）"
        } else if v4_ok {
            "✅ Key 有效（v4 鉴权：Bearer）"
        } else {
            "❌ 两种鉴权均失败，请核对 Key"
        }
    );
}

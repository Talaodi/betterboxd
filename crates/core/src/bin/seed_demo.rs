//! 演示数据种子：半年观测史（真实 TMDB 元数据 + 按账目规则构造的 Action 流）。
//!
//! 用法：`cargo run -p betterboxd-core --bin seed_demo`（在 betterboxd/ 目录）。
//! 幂等：movies 表非空即跳过。结束后对全部影片执行状态重算。

use betterboxd_core::config::Config;
use betterboxd_core::db::apply_migrations;
use betterboxd_core::tmdb::TmdbClient;
use chrono::TimeZone;
use rusqlite::{Connection, params};
use serde_json::json;

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

fn date_ts(date: &str, shift_days: i64) -> i64 {
    let d = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").expect("日期格式错误");
    let dt = d.and_hms_opt(20, 0, 0).expect("hms") + chrono::Duration::days(shift_days);
    chrono::Utc.from_utc_datetime(&dt).timestamp()
}

/// (搜索名, 年份, 只进想看)
const FILMS: &[(&str, i64, bool)] = &[
    ("Inception", 2010, false),
    ("In the Mood for Love", 2000, false),
    ("Interstellar", 2014, false),
    ("Spirited Away", 2001, false),
    ("Parasite", 2019, false),
    ("Everything Everywhere All at Once", 2022, false),
    ("Oppenheimer", 2023, false),
    ("Past Lives", 2023, false),
    ("Perfect Days", 2023, false),
    ("Chungking Express", 1994, false),
    ("Arrival", 2016, false),
    ("Your Name", 2016, false),
    ("Days of Being Wild", 1990, true), // 阿飞正传
    ("Fallen Angels", 1995, true),      // 堕落天使
    ("Ashes of Time", 1994, true),      // 东邪西毒
];

#[derive(Clone)]
struct EntryPlan {
    date: &'static str,
    film: usize,
    rating: Option<i64>,
    theater: bool,
    price: Option<i64>,
    liked: bool,
    dims: &'static [(&'static str, &'static str)],
    tags: &'static [&'static str],
    note: &'static str,
    created_shift_days: i64,
    source: &'static str, // edit | agent | import
}

const E: &[EntryPlan] = &[
    EntryPlan {
        date: "2026-03-06",
        film: 0,
        rating: Some(88),
        theater: true,
        price: Some(4500),
        liked: false,
        dims: &[
            ("地点", "美嘉影城"),
            ("同伴", "老张"),
            ("情绪", "烧脑"),
            ("场景", "重映"),
        ],
        tags: &["诺兰补票"],
        note: "第三层梦醒时心跳全在鼓点上。",
        created_shift_days: 1,
        source: "edit",
    },
    EntryPlan {
        date: "2026-03-14",
        film: 3,
        rating: Some(92),
        theater: true,
        price: Some(4000),
        liked: false,
        dims: &[
            ("地点", "电影资料馆"),
            ("同伴", "独看"),
            ("情绪", "治愈"),
            ("场景", "资料馆修复版"),
        ],
        tags: &["宫崎骏"],
        note: "修复版的水彩质感，汤屋的蒸汽几乎扑面。",
        created_shift_days: 0,
        source: "edit",
    },
    EntryPlan {
        date: "2026-03-22",
        film: 9,
        rating: Some(85),
        theater: false,
        price: None,
        liked: false,
        dims: &[("地点", "家"), ("同伴", "独看"), ("情绪", "怅然")],
        tags: &["墨镜王"],
        note: "加州梦与凤梨罐头，两段都爱。",
        created_shift_days: 0,
        source: "edit",
    },
    EntryPlan {
        date: "2026-04-02",
        film: 11,
        rating: Some(78),
        theater: true,
        price: Some(4000),
        liked: false,
        dims: &[("地点", "美嘉影城"), ("同伴", "小王"), ("情绪", "泪崩")],
        tags: &["补课"],
        note: "补 2016 年欠下的一张票。",
        created_shift_days: 2,
        source: "edit",
    },
    EntryPlan {
        date: "2026-04-11",
        film: 4,
        rating: Some(90),
        theater: true,
        price: Some(4500),
        liked: false,
        dims: &[("地点", "美嘉影城"), ("同伴", "独看"), ("情绪", "烧脑")],
        tags: &["奉俊昊"],
        note: "楼梯那场戏的结构感太强了。",
        created_shift_days: 0,
        source: "edit",
    },
    EntryPlan {
        date: "2026-04-19",
        film: 2,
        rating: Some(95),
        theater: true,
        price: Some(5000),
        liked: true,
        dims: &[
            ("地点", "万达IMAX"),
            ("同伴", "独看"),
            ("情绪", "泪崩"),
            ("场景", "IMAX"),
        ],
        tags: &["重看"],
        note: "第二次看还是被五维书架击中。",
        created_shift_days: 0,
        source: "edit",
    },
    EntryPlan {
        date: "2026-05-01",
        film: 5,
        rating: Some(82),
        theater: true,
        price: Some(4500),
        liked: false,
        dims: &[("地点", "美嘉影城"), ("同伴", "老张"), ("情绪", "爽")],
        tags: &["多元宇宙"],
        note: "石头宇宙的对话意外地动人。",
        created_shift_days: 0,
        source: "edit",
    },
    EntryPlan {
        date: "2026-05-09",
        film: 9,
        rating: Some(88),
        theater: false,
        price: None,
        liked: false,
        dims: &[("地点", "家"), ("同伴", "独看"), ("情绪", "治愈")],
        tags: &["深夜场"],
        note: "二刷：这次注意到逗号与雨。",
        created_shift_days: 0,
        source: "edit",
    },
    EntryPlan {
        date: "2026-05-17",
        film: 10,
        rating: Some(91),
        theater: true,
        price: Some(4000),
        liked: false,
        dims: &[("地点", "美嘉影城"), ("同伴", "独看"), ("情绪", "烧脑")],
        tags: &["语言学"],
        note: "非线性时间观与中年的和解。",
        created_shift_days: 0,
        source: "edit",
    },
    EntryPlan {
        date: "2026-05-30",
        film: 1,
        rating: Some(90),
        theater: false,
        price: None,
        liked: false,
        dims: &[("地点", "家"), ("同伴", "独看"), ("情绪", "怅然")],
        tags: &["王家卫"],
        note: "重看：花样的时间从来不属于他们自己。",
        created_shift_days: 1,
        source: "edit",
    },
    EntryPlan {
        date: "2026-06-07",
        film: 6,
        rating: Some(93),
        theater: true,
        price: Some(6000),
        liked: true,
        dims: &[
            ("地点", "美嘉影城"),
            ("同伴", "独看"),
            ("情绪", "烧脑"),
            ("场景", "IMAX"),
        ],
        tags: &["传记"],
        note: "三小时的听证会结构，尾声像核爆余波。",
        created_shift_days: 0,
        source: "edit",
    },
    EntryPlan {
        date: "2026-06-15",
        film: 11,
        rating: Some(75),
        theater: false,
        price: None,
        liked: false,
        dims: &[("地点", "家"), ("同伴", "独看")],
        tags: &[],
        note: "二刷感受衰减，果然是季节限定。",
        created_shift_days: 0,
        source: "edit",
    },
    EntryPlan {
        date: "2026-06-28",
        film: 7,
        rating: Some(89),
        theater: true,
        price: Some(4000),
        liked: false,
        dims: &[("地点", "电影资料馆"), ("同伴", "老张"), ("情绪", "怅然")],
        tags: &["A24"],
        note: "云淡风轻，后劲三天。",
        created_shift_days: 0,
        source: "agent",
    },
    EntryPlan {
        date: "2026-07-05",
        film: 3,
        rating: Some(94),
        theater: false,
        price: None,
        liked: false,
        dims: &[("地点", "家"), ("同伴", "独看"), ("情绪", "治愈")],
        tags: &[],
        note: "三刷。铁道那场仍然是全片最柔软的地方。",
        created_shift_days: 0,
        source: "agent",
    },
    EntryPlan {
        date: "2026-07-12",
        film: 5,
        rating: Some(80),
        theater: false,
        price: None,
        liked: false,
        dims: &[("地点", "老张家"), ("同伴", "老张"), ("情绪", "爽")],
        tags: &[],
        note: "在老张家投影重看，笑点依旧密集。",
        created_shift_days: 0,
        source: "agent",
    },
    EntryPlan {
        date: "2026-07-19",
        film: 2,
        rating: Some(95),
        theater: true,
        price: Some(5500),
        liked: true,
        dims: &[
            ("地点", "万达IMAX"),
            ("同伴", "老张"),
            ("情绪", "泪崩"),
            ("场景", "IMAX"),
        ],
        tags: &["三刷"],
        note: "IMAX 三刷，灯围物玩梗全场大笑。",
        created_shift_days: 0,
        source: "agent",
    },
    EntryPlan {
        date: "2026-08-02",
        film: 4,
        rating: Some(92),
        theater: false,
        price: None,
        liked: false,
        dims: &[("地点", "家"), ("同伴", "独看"), ("情绪", "烧脑")],
        tags: &["二刷细节"],
        note: "二刷注意到了石头汉堡和进口水蜜桃。",
        created_shift_days: 0,
        source: "agent",
    },
    EntryPlan {
        date: "2026-08-09",
        film: 8,
        rating: Some(91),
        theater: true,
        price: Some(3800),
        liked: false,
        dims: &[
            ("地点", "电影资料馆"),
            ("同伴", "独看"),
            ("情绪", "治愈"),
            ("场景", "电影节展映"),
        ],
        tags: &["役所广司"],
        note: "清理公厕时抬头看树影，是全片的呼吸。",
        created_shift_days: 2,
        source: "agent",
    },
    EntryPlan {
        date: "2026-08-16",
        film: 7,
        rating: Some(90),
        theater: false,
        price: None,
        liked: false,
        dims: &[("地点", "家"), ("同伴", "独看"), ("情绪", "怅然")],
        tags: &[],
        note: "二刷：移民局的走廊比记忆里更长。",
        created_shift_days: 0,
        source: "agent",
    },
    EntryPlan {
        date: "2026-08-23",
        film: 1,
        rating: Some(90),
        theater: true,
        price: Some(4500),
        liked: true,
        dims: &[
            ("地点", "美嘉影城"),
            ("同伴", "老张"),
            ("情绪", "怅然"),
            ("场景", "重映"),
        ],
        tags: &["胶片"],
        note: "大银幕的霓虹色完全不一样。",
        created_shift_days: 0,
        source: "agent",
    },
];

/// 旧档回填（source=import：审计可见但不计入影片级断言）。
const IMPORTS: &[(&str, usize, Option<i64>)] = &[
    ("2019-08-20", 1, Some(70)),
    ("2003-07-12", 3, Some(85)),
    ("2015-01-02", 0, Some(80)),
];

/// (片下标, 标题, 正文, 评分可空, 点赞)
const REVIEWS: &[(usize, &str, &str, Option<i64>, bool)] = &[
    (
        1,
        "重看霓虹",
        "第三次看《花样年华》，才看懂那些没有对白的擦肩。苏丽珍的旗袍是日历，周慕云的领带是钟摆，两人在2046房间排练告别，却在真正的告别面前失语。王家卫把偷情拍成了考古，把遗憾拍成了考古现场里那只没有塞进树洞的耳朵。",
        Some(95),
        true,
    ),
    (
        6,
        "三小时的三枚回声",
        "诺兰用听证会的三线剪辑让三个年份互相回声：种子在1943年埋下，在1945年爆炸，在1949年燎原。基里安·墨菲的眼睛是全片的核反应堆，而小唐尼把政治动物的圆滑演出了悲剧的底色。",
        Some(88),
        false,
    ),
    (
        7,
        "云淡风轻的重量",
        "《过往人生》把『如果当初』拍成一门统计学：12年、24年，两个平行人生的差值最终收敛于一句『我们那时分手了』。它不控诉移民也不歌颂重逢，只是把遗憾摊开在长椅上，让你自己决定哭不哭。",
        Some(91),
        true,
    ),
];

fn ensure_pool(conn: &Connection, dim: &str, name: &str) -> String {
    let existing: Option<String> = conn
        .query_row(
            "SELECT id FROM dimension_values WHERE dimension=?1 AND name=?2",
            params![dim, name],
            |r| r.get(0),
        )
        .ok();
    existing.unwrap_or_else(|| {
        let id = uuid::Uuid::now_v7().to_string();
        conn.execute(
            "INSERT INTO dimension_values (id, dimension, name) VALUES (?1,?2,?3)",
            params![id, dim, name],
        )
        .unwrap();
        id
    })
}

fn ensure_tag(conn: &Connection, name: &str) -> String {
    let existing: Option<String> = conn
        .query_row("SELECT id FROM tags WHERE name=?1", params![name], |r| {
            r.get(0)
        })
        .ok();
    existing.unwrap_or_else(|| {
        let id = uuid::Uuid::now_v7().to_string();
        conn.execute(
            "INSERT INTO tags (id, name) VALUES (?1,?2)",
            params![id, name],
        )
        .unwrap();
        id
    })
}

fn insert_action(
    conn: &Connection,
    movie_id: i64,
    at: i64,
    source: &str,
    ref_id: Option<&str>,
    changes: serde_json::Value,
) {
    conn.execute(
        "INSERT INTO actions (id, movie_id, target, target_id, at, source, ref_id, changes_json)
         VALUES (?1,?2,'movie',?2,?3,?4,?5,?6)",
        params![
            uuid::Uuid::now_v7().to_string(),
            movie_id,
            at,
            source,
            ref_id,
            changes.to_string()
        ],
    )
    .unwrap();
}

fn insert_entry(conn: &Connection, movie_id: i64, e: &EntryPlan) -> String {
    let id = uuid::Uuid::now_v7().to_string();
    let at = date_ts(e.date, 0);
    let created = at + e.created_shift_days * 86400;
    conn.execute(
        "INSERT INTO diary_entries (id, movie_id, watched_date, rating, in_theater, liked,
           ticket_price_cents, private_note, created_at, updated_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?9)",
        params![
            id,
            movie_id,
            e.date,
            e.rating,
            e.theater as i64,
            e.liked as i64,
            e.price,
            e.note,
            created
        ],
    )
    .unwrap();
    for (dim, name) in e.dims {
        let vid = ensure_pool(conn, dim, name);
        conn.execute(
            "INSERT INTO entry_dimensions (entry_id, value_id) VALUES (?1,?2)",
            params![id, vid],
        )
        .unwrap();
    }
    for tag in e.tags {
        let tid = ensure_tag(conn, tag);
        conn.execute(
            "INSERT INTO entry_tags (entry_id, tag_id) VALUES (?1,?2)",
            params![id, tid],
        )
        .unwrap();
    }
    // 断言 Action（import 不计入；created 偏移体现为 at 略晚于观看时刻）
    let src_at = created + 60;
    if let Some(r) = e.rating {
        insert_action(
            conn,
            movie_id,
            src_at,
            e.source,
            Some(&id),
            json!({"my_rating": [null, r]}),
        );
    }
    if e.liked && e.source != "import" {
        insert_action(
            conn,
            movie_id,
            src_at + 1,
            e.source,
            Some(&id),
            json!({"liked": [0, 1]}),
        );
    }
    id
}

#[tokio::main]
async fn main() {
    let lib = std::path::Path::new("data/.betterboxd");
    std::fs::create_dir_all(lib).unwrap();
    if !lib.join("config.toml").exists() {
        // 兜底：直接从 spikes/local.env 合成最小配置（与服务器 ensure_config 一致）
        let mut kv = std::collections::HashMap::new();
        if let Ok(raw) = std::fs::read_to_string("spikes/local.env") {
            for line in raw.lines() {
                if let Some((k, v)) = line.split_once('=') {
                    let mut v = v.trim().to_string();
                    if (v.starts_with('"') && v.ends_with('"') && v.len() >= 2)
                        || (v.starts_with('\'') && v.ends_with('\'') && v.len() >= 2)
                    {
                        v = v[1..v.len() - 1].to_string();
                    }
                    if !v.is_empty() {
                        kv.insert(k.trim().to_string(), v);
                    }
                }
            }
        }
        let cfg = Config {
            profiles: vec![],
            active_profile: None,
            tmdb: betterboxd_core::config::TmdbCfg {
                key: kv.get("TMDB_KEY").cloned().unwrap_or_default(),
                proxy: None,
                language: "zh-CN".into(),
            },
            billing: Default::default(),
            display: Default::default(),
        };
        cfg.save(&lib.join("config.toml"))
            .expect("写入 config 失败");
    }
    let config = Config::load(&lib.join("config.toml")).expect("config 解析失败");
    let db_path = lib.join("data.db");

    {
        let conn = Connection::open(&db_path).expect("打开库失败");
        apply_migrations(&conn).expect("迁移失败");
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM movies", [], |r| r.get(0))
            .unwrap();
        if n > 0 {
            println!("movies 表已有 {n} 行，跳过种子（幂等）");
            return;
        }
    }

    let tmdb = TmdbClient::new(
        config.tmdb.key.clone(),
        config.tmdb.proxy.clone(),
        config.tmdb.language.clone(),
    );

    let conn = Connection::open(&db_path).expect("打开库失败");
    apply_migrations(&conn).expect("迁移失败");
    conn.pragma_update(None, "foreign_keys", "ON").unwrap();

    println!("拉取 {} 部影片元数据（250ms 限速）…", FILMS.len());
    let mut film_ids: Vec<i64> = vec![0; FILMS.len()];
    for (i, (title, year, watchlist)) in FILMS.iter().enumerate() {
        let results = match tmdb.search_movie(title, Some(*year)).await {
            Ok(r) => r,
            Err(e) => {
                eprintln!("  ⚠ {title} 搜索失败: {e}");
                continue;
            }
        };
        let Some(first) = results.first().cloned() else {
            eprintln!("  ⚠ {title} 无结果");
            continue;
        };
        let tmdb_id = first["tmdb_id"].as_i64().unwrap_or(0);
        let details = match tmdb.movie_details(tmdb_id).await {
            Ok(d) => d,
            Err(e) => {
                eprintln!("  ⚠ {title} 详情失败: {e}");
                continue;
            }
        };
        let directors: Vec<String> = details["credits"]["crew"]
            .as_array()
            .map(|crew| {
                crew.iter()
                    .filter(|c| c["job"] == "Director")
                    .filter_map(|c| c["name"].as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        let genres: Vec<String> = details["genres"]
            .as_array()
            .map(|g| {
                g.iter()
                    .filter_map(|g| g["name"].as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        let poster = details["poster_path"].as_str().unwrap_or("").to_string();
        let inserted = conn
            .execute(
                "INSERT INTO movies (tmdb_id, title_zh, title_en, title_original, release_date,
               runtime, original_language, directors, genres, posters, tagline, overview,
               lb_rating, lb_votes, in_watchlist, fetched_at, updated_at)
             VALUES (?1, ?2, NULL, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?15)",
                params![
                    tmdb_id,
                    first["title"].as_str().unwrap_or(""),
                    details["original_title"].as_str().unwrap_or(""),
                    details["release_date"].as_str().unwrap_or(""),
                    details["runtime"].as_i64(),
                    details["original_language"].as_str().unwrap_or(""),
                    serde_json::to_string(&directors).unwrap(),
                    serde_json::to_string(&genres).unwrap(),
                    serde_json::to_string(&vec![poster]).unwrap(),
                    details["tagline"].as_str().unwrap_or(""),
                    details["overview"].as_str().unwrap_or(""),
                    details["vote_average"].as_f64(),
                    details["vote_count"].as_i64(),
                    *watchlist as i64,
                    now(),
                ],
            )
            .unwrap_or_else(|e| {
                eprintln!("  插入 {title} 失败: {e}");
                0usize
            });
        if inserted > 0 {
            film_ids[i] = tmdb_id;
        }
    }
    println!(
        "影片完成：{}/{}",
        film_ids.iter().filter(|&&i| i != 0).count(),
        FILMS.len()
    );

    // —— 观影条目 + 断言流 ——
    for e in E {
        let movie_id = film_ids[e.film];
        if movie_id == 0 {
            continue;
        }
        let _ = insert_entry(&conn, movie_id, e);
    }
    // 想看自动消除演示：Perfect Days（8 号）创建条目时 in_watchlist 1→0
    if film_ids[8] != 0 {
        insert_action(
            &conn,
            film_ids[8],
            date_ts("2026-08-09", 2) + 30,
            "system",
            None,
            json!({"in_watchlist": [1, 0]}),
        );
    }
    // 旧档回填（import 断言：审计可见但不参与重算）
    for (date, film, rating) in IMPORTS {
        let movie_id = film_ids[*film];
        if movie_id == 0 {
            continue;
        }
        let plan = EntryPlan {
            date,
            film: *film,
            rating: *rating,
            theater: false,
            price: None,
            liked: false,
            dims: &[("地点", "家")],
            tags: &[],
            note: "旧档回填",
            created_shift_days: 0,
            source: "import",
        };
        let _ = insert_entry(&conn, movie_id, &plan);
    }
    // 独立改分演示：盗梦空间 6 月改分 88→85
    if film_ids[0] != 0 {
        insert_action(
            &conn,
            film_ids[0],
            date_ts("2026-06-20", 0),
            "standalone",
            None,
            json!({"my_rating": [88, 85]}),
        );
    }
    // 删除演示：EEAAO 重复条目 7-25（断言→删除→撤销生效）
    if film_ids[5] != 0 {
        let dup = EntryPlan {
            date: "2026-07-25",
            film: 5,
            rating: Some(82),
            theater: false,
            price: None,
            liked: false,
            dims: &[("地点", "家")],
            tags: &[],
            note: "重复录入（演示）",
            created_shift_days: 0,
            source: "edit",
        };
        let dup_id = insert_entry(&conn, film_ids[5], &dup);
        conn.execute(
            "INSERT INTO actions (id, movie_id, target, target_id, at, source, changes_json)
             VALUES (?1,?2,'diary_entry',?3,?4,'edit','{\"deleted\":[false,true]}')",
            params![
                uuid::Uuid::now_v7().to_string(),
                film_ids[5],
                dup_id,
                date_ts("2026-07-26", 0)
            ],
        )
        .unwrap();
        conn.execute("DELETE FROM diary_entries WHERE id=?1", params![dup_id])
            .unwrap();
    }
    // —— 影评 + 断言 ——
    for (film, title, body, rating, liked) in REVIEWS {
        let movie_id = film_ids[*film];
        if movie_id == 0 {
            continue;
        }
        let rid = uuid::Uuid::now_v7().to_string();
        let created = now() - 3 * 86400;
        conn.execute(
            "INSERT INTO reviews (id, movie_id, title, body_md, rating, liked, created_at, updated_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?7)",
            params![rid, movie_id, title, body, rating, *liked as i64, created],
        ).unwrap();
        if let Some(r) = rating {
            insert_action(
                &conn,
                movie_id,
                created + 30,
                "edit",
                Some(&rid),
                json!({"my_rating": [null, r]}),
            );
        }
        if *liked {
            insert_action(
                &conn,
                movie_id,
                created + 31,
                "edit",
                Some(&rid),
                json!({"liked": [0, 1]}),
            );
        }
    }
    // —— 清单 ——
    let mk_list = |name: &str, ranked: i64| -> String {
        let id = uuid::Uuid::now_v7().to_string();
        conn.execute(
            "INSERT INTO lists (id, name, source, ranked, created_at, updated_at)
             VALUES (?1,?2,'manual',?3,?4,?4)",
            params![id, name, ranked, now()],
        )
        .unwrap();
        id
    };
    let top_list = mk_list("2026 十佳候选", 1);
    for (rank, film) in [(1i64, 1usize), (2, 8), (3, 7), (4, 3), (5, 4)] {
        if film_ids[film] == 0 {
            continue;
        }
        conn.execute(
            "INSERT INTO list_items (list_id, movie_id, rank, added_at) VALUES (?1,?2,?3,?4)",
            params![top_list, film_ids[film], rank, now()],
        )
        .unwrap();
    }
    let re_list = mk_list("想二刷", 0);
    for film in [0usize, 2] {
        if film_ids[film] == 0 {
            continue;
        }
        conn.execute(
            "INSERT INTO list_items (list_id, movie_id, rank, added_at) VALUES (?1,?2,NULL,?3)",
            params![re_list, film_ids[film], now()],
        )
        .unwrap();
    }
    // —— 收藏统计示例 ——
    for (name, sql, chart) in [
        (
            "今年各月观影量",
            "SELECT strftime('%Y-%m', watched_date) AS 月, COUNT(*) AS 部数, ROUND(AVG(rating),1) AS 均分 FROM v_diary_full WHERE watched_date >= date('now','start of year') GROUP BY 1 ORDER BY 1",
            json!({"type":"bar","title":"今年各月观影量"}),
        ),
        (
            "各地点平均票价",
            "SELECT j.value AS 地点, ROUND(AVG(d.ticket_price_cents)/100.0,1) AS 均价 FROM v_diary_full d, json_each(d.dimensions_flat) j WHERE json_extract(j.value,'$.dimension')='地点' AND d.ticket_price_cents IS NOT NULL GROUP BY 1 ORDER BY 2 DESC",
            json!({"type":"bar","title":"各地点平均票价(元)"}),
        ),
    ] {
        conn.execute(
            "INSERT INTO saved_queries (id, name, payload_json, sort_order, created_at)
             VALUES (?1,?2,?3,?4,?5)",
            params![
                uuid::Uuid::now_v7().to_string(),
                name,
                json!({"sql": sql, "chart": chart}).to_string(),
                0,
                now()
            ],
        )
        .unwrap();
    }
    // —— 状态重算 ——
    betterboxd_core::db::recompute_all_movie_states(&conn).unwrap();

    // —— 汇总 ——
    let (n_movies, n_entries, n_reviews, n_actions): (i64, i64, i64, i64) = conn
        .query_row(
            "SELECT (SELECT COUNT(*) FROM movies), (SELECT COUNT(*) FROM diary_entries),
                (SELECT COUNT(*) FROM reviews), (SELECT COUNT(*) FROM actions)",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();
    println!("种子完成：影片 {n_movies}，条目 {n_entries}，影评 {n_reviews}，Action {n_actions}");
    println!("--- 影片级状态（v_movies 抽样）---");
    let mut st = conn
        .prepare(
            "SELECT title_main, my_rating, liked, watched, in_watchlist FROM v_movies
         WHERE watched=1 OR in_watchlist=1 ORDER BY tmdb_id",
        )
        .unwrap();
    let rows = st
        .query_map([], |r| {
            Ok(format!(
                "{:<20} my={:?} liked={} watched={} 想看={}",
                r.get::<_, String>(0)?,
                r.get::<_, Option<i64>>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, i64>(4)?
            ))
        })
        .unwrap();
    for row in rows {
        println!("{}", row.unwrap());
    }
}

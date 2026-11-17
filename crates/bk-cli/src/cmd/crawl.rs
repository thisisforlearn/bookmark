// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

//! `bookmark crawl` — fetch and index bookmarks.

use bk_db::bookmarks;

use crate::cmd::common;
use crate::init_tracing;

pub async fn run(all: bool, limit: Option<usize>) -> i32 {
    init_tracing();
    let cfg = common::load_config();
    let state = match common::open_state(cfg) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: {e}");
            return 1;
        }
    };
    let (user_id, _) = match common::primary_user(&state.db) {
        Ok(u) => u,
        Err(_) => {
            eprintln!("error: no users yet — run `bookmark init`");
            return 1;
        }
    };

    let max = limit.unwrap_or(usize::MAX);
    let mut targets: Vec<(i64, String)> = Vec::new();
    if all {
        let mut offset = 0;
        loop {
            let page = state
                .db
                .with_conn_sync(|c| bookmarks::list(c, user_id, 200, offset))
                .unwrap_or_default();
            if page.is_empty() {
                break;
            }
            offset += page.len();
            let done = page.len() < 200;
            for b in page {
                if !b.url.is_empty() {
                    targets.push((b.id.inner(), b.url));
                }
                if targets.len() >= max {
                    break;
                }
            }
            if done || targets.len() >= max {
                break;
            }
        }
    } else {
        let ids = state
            .db
            .with_conn_sync(|c| bk_db::crawls::due(c, Some(user_id), max))
            .unwrap_or_default();
        for id in ids {
            if let Ok(Some(b)) = state.db.with_conn_sync(|c| bookmarks::get(c, user_id, id))
                && !b.url.is_empty()
            {
                targets.push((b.id.inner(), b.url));
            }
        }
    }

    if targets.is_empty() {
        println!("nothing to crawl");
        return 0;
    }

    let pb = indicatif::ProgressBar::new(targets.len() as u64);
    pb.set_style(
        indicatif::ProgressStyle::with_template("{bar:40} {pos}/{len} {msg}")
            .unwrap_or_else(|_| indicatif::ProgressStyle::default_bar()),
    );
    let mut ok = 0usize;
    for (id, url) in targets {
        pb.set_message(url.clone());
        bk_serve::ingest::crawl_bookmark(&state, bk_core::Id::from_i64(id), &url).await;
        ok += 1;
        pb.inc(1);
    }
    pb.finish_with_message("done");
    state.search.refresh_vocab(user_id);
    println!("crawled {ok} pages");
    0
}

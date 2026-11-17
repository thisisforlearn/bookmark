// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

//! `bookmark index` — rebuild the FTS index and re-embed bookmarks.

use bk_db::{bookmarks, fts};

use crate::cmd::common;
use crate::init_tracing;

pub async fn run(rebuild: bool) -> i32 {
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
            eprintln!("error: no users yet");
            return 1;
        }
    };

    if rebuild {
        match state.db.with_conn_sync(fts::rebuild) {
            Ok(()) => println!("rebuilt full-text index"),
            Err(e) => {
                eprintln!("error: {e}");
                return 1;
            }
        }
    }

    // Re-embed every bookmark if a model is present.
    if state.models.embed.is_some() {
        let total = state
            .db
            .with_conn_sync(|c| bookmarks::count(c, user_id))
            .unwrap_or(0);
        let pb = indicatif::ProgressBar::new(total.max(0) as u64);
        let mut offset = 0;
        let mut n = 0;
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
                bk_serve::ingest::embed_bookmark(&state, b.id).await;
                n += 1;
                pb.inc(1);
            }
            if done {
                break;
            }
        }
        pb.finish_and_clear();
        println!("embedded {n} bookmarks");
    } else {
        println!("no embedding model installed; skipped vectors");
    }

    state.search.refresh_vocab(user_id);
    println!("index up to date");
    0
}

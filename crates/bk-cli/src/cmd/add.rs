// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

//! `bookmark add` — save a URL or a text note.

use crate::cmd::common;
use crate::init_tracing;

pub async fn run(target: &str) -> i32 {
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
    match bk_serve::ingest::create(&state, user_id, target, &[]).await {
        Ok(id) => {
            println!("saved bookmark {id}");
            if bk_crawl::is_http_url(target) {
                println!("crawling in the background…");
            }
            0
        }
        Err(e) => {
            eprintln!("error: {e}");
            1
        }
    }
}

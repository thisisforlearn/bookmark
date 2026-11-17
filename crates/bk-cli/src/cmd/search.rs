// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

//! `bookmark search` — query the index from the terminal.

use bk_search::SearchRequest;

use crate::cmd::common;
use crate::init_tracing;

pub async fn run(query: &str) -> i32 {
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
    let req = SearchRequest {
        q: query.to_string(),
        user_id,
        limit: 20,
        offset: 0,
    };
    let resp = match state.search.search(&req) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {e}");
            return 1;
        }
    };
    if let Some(c) = &resp.corrected_query {
        println!("did you mean: {c}");
    }
    if resp.hits.is_empty() {
        println!("no results");
        return 0;
    }
    println!(
        "{} results in {} ms{}\n",
        resp.hits.len(),
        resp.took_ms,
        if resp.degraded { " (degraded)" } else { "" }
    );
    for (i, h) in resp.hits.iter().enumerate() {
        let title = if h.bookmark.title.is_empty() {
            h.bookmark.url.clone()
        } else {
            h.bookmark.title.clone()
        };
        println!("{:>2}. {}", i + 1, title);
        if !h.bookmark.url.is_empty() {
            println!("    {}", h.bookmark.url);
        }
        let plain = strip_tags(&h.snippet);
        if !plain.is_empty() {
            println!("    {plain}");
        }
        if !h.tags.is_empty() {
            println!("    tags: {}", h.tags.join(", "));
        }
        println!();
    }
    0
}

fn strip_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
}

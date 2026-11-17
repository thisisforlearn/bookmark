// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

//! `bookmark export` — dump bookmarks to JSON.

use std::io::Write;

use serde::{Deserialize, Serialize};

use bk_db::{bookmarks, tags};

use crate::cmd::common;
use crate::init_tracing;

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct ExportDoc {
    pub version: u32,
    pub exported_at: i64,
    pub bookmarks: Vec<ExportBookmark>,
}

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct ExportBookmark {
    pub url: String,
    pub url_norm: String,
    pub title: String,
    pub description: String,
    pub text: String,
    pub note: String,
    pub pinned: bool,
    pub tags: Vec<String>,
    pub created_at: i64,
}

pub async fn run(out: &str) -> i32 {
    init_tracing();
    let db = match common::open_db() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("error: {e}");
            return 1;
        }
    };
    let (user_id, _) = match common::primary_user(&db) {
        Ok(u) => u,
        Err(_) => {
            eprintln!("error: no users yet");
            return 1;
        }
    };

    let mut doc = ExportDoc {
        version: 1,
        exported_at: bk_core::now(),
        bookmarks: Vec::new(),
    };
    let mut offset = 0;
    loop {
        let page = db
            .with_conn_sync(|c| bookmarks::list(c, user_id, 200, offset))
            .unwrap_or_default();
        if page.is_empty() {
            break;
        }
        offset += page.len();
        let done = page.len() < 200;
        for b in page {
            let tg = db
                .with_conn_sync(|c| tags::for_bookmark(c, b.id))
                .unwrap_or_default();
            doc.bookmarks.push(ExportBookmark {
                url: b.url,
                url_norm: b.url_norm,
                title: b.title,
                description: b.description,
                text: b.text,
                note: b.note,
                pinned: b.pinned,
                tags: tg,
                created_at: b.created_at,
            });
        }
        if done {
            break;
        }
    }

    let json = match serde_json::to_vec_pretty(&doc) {
        Ok(j) => j,
        Err(e) => {
            eprintln!("error: {e}");
            return 1;
        }
    };
    if out == "-" {
        let _ = std::io::stdout().write_all(&json);
    } else if let Err(e) = std::fs::write(out, &json) {
        eprintln!("error: cannot write {out}: {e}");
        return 1;
    } else {
        println!("exported {} bookmarks to {out}", doc.bookmarks.len());
    }
    0
}

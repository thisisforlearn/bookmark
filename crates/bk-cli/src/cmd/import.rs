// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

//! `bookmark import` — load a JSON export. Idempotent by `url_norm`.

use bk_db::{bookmarks, tags};

use crate::cmd::common;
use crate::cmd::export::ExportDoc;
use crate::init_tracing;

pub async fn run(input: &str) -> i32 {
    init_tracing();
    let data = match std::fs::read(input) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("error: cannot read {input}: {e}");
            return 1;
        }
    };
    let doc: ExportDoc = match serde_json::from_slice(&data) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("error: invalid export file: {e}");
            return 1;
        }
    };
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
            eprintln!("error: no users yet — run `bookmark init`");
            return 1;
        }
    };

    let pb = indicatif::ProgressBar::new(doc.bookmarks.len() as u64);
    let mut added = 0usize;
    let mut skipped = 0usize;
    for b in doc.bookmarks {
        let exists = db
            .with_conn_sync(|c| bookmarks::get_by_url(c, user_id, &b.url_norm))
            .ok()
            .flatten()
            .is_some();
        if exists {
            skipped += 1;
            pb.inc(1);
            continue;
        }
        match db.with_conn_sync(|c| {
            bookmarks::insert(c, user_id, &b.url_norm, &b.url, &b.title, &b.text)
        }) {
            Ok(id) => {
                if !b.tags.is_empty() {
                    let _ = db.with_conn_sync(|c| tags::set(c, user_id, id, &b.tags));
                }
                added += 1;
            }
            Err(_) => skipped += 1,
        }
        pb.inc(1);
    }
    pb.finish_and_clear();
    println!("imported {added} bookmarks ({skipped} already present)");
    0
}

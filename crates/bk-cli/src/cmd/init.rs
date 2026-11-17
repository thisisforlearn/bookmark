// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

//! `bookmark init` — create the data directory, config, database and first admin.

use bk_core::state_dir;
use bk_db::users;
use rand::Rng;

use crate::cmd::common;
use crate::init_tracing;

pub async fn run() -> i32 {
    init_tracing();
    let cfg_path = common::config_path();
    let cfg = common::load_config();
    if let Err(e) = state_dir(&cfg.data_dir) {
        eprintln!(
            "error: cannot create data dir {}: {e}",
            cfg.data_dir.display()
        );
        return 1;
    }
    if let Err(e) = std::fs::create_dir_all(&cfg.models_dir) {
        eprintln!("error: cannot create models dir: {e}");
        return 1;
    }
    let mut cfg = cfg;
    cfg.models_dir = cfg.models_dir.clone();
    if let Err(e) = cfg.save(&cfg_path) {
        eprintln!("error: cannot write config {}: {e}", cfg_path.display());
        return 1;
    }
    let db = match bk_db::Db::open(&cfg.db_path()) {
        Ok(db) => db,
        Err(e) => {
            eprintln!("error: cannot open database: {e}");
            return 1;
        }
    };

    let existing = db.with_conn_sync(users::count).unwrap_or(0);
    if existing == 0 {
        let password = std::env::var("BOOKMARK_ADMIN_PASSWORD")
            .ok()
            .filter(|p| !p.is_empty())
            .unwrap_or_else(generate_password);
        match db.with_conn_sync(|c| users::create(c, &cfg.admin, &password, true)) {
            Ok(id) => {
                println!("created admin `{}` (id {})", cfg.admin, id);
                if std::env::var("BOOKMARK_ADMIN_PASSWORD").is_err() {
                    println!();
                    println!("  username: {}", cfg.admin);
                    println!("  password: {password}");
                    println!();
                    println!("  Save this password now — it is not shown again.");
                }
            }
            Err(e) => {
                eprintln!("error: cannot create admin: {e}");
                return 1;
            }
        }
    } else {
        println!("database already initialised ({} users)", existing);
    }

    println!();
    println!("  data dir: {}", cfg.data_dir.display());
    println!("  database: {}", cfg.db_path().display());
    println!("  models:   {}", cfg.models_dir.display());
    println!();
    println!("Next:  bookmark serve --host 127.0.0.1 --port 8080");
    println!("Then:  open http://127.0.0.1:8080 in your browser");
    0
}

fn generate_password() -> String {
    const ALPHABET: &[u8] = b"abcdefghjkmnpqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    let mut rng = rand::rng();
    (0..18)
        .map(|_| ALPHABET[rng.random_range(0..ALPHABET.len())] as char)
        .collect()
}

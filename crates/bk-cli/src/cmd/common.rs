// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

//! Shared helpers for CLI commands.

use std::path::PathBuf;
use std::sync::Arc;

use bk_core::{Config, Id, Result};
use bk_db::{Db, users};
use bk_serve::AppState;

pub fn config_path() -> PathBuf {
    if let Ok(p) = std::env::var("BOOKMARK_CONFIG") {
        return PathBuf::from(p);
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join(".config/bookmark/config.toml");
    }
    PathBuf::from("/etc/bookmark/config.toml")
}

pub fn load_config() -> Config {
    let path = config_path();
    match Config::load(&path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("warning: {e}; using defaults");
            Config::default()
        }
    }
}

pub fn open_state(cfg: Config) -> Result<Arc<AppState>> {
    bk_serve::build_state(cfg).map(Arc::new)
}

pub fn open_db() -> Result<Db> {
    let cfg = load_config();
    bk_core::state_dir(&cfg.data_dir)?;
    Db::open(&cfg.db_path())
}

/// The default identity for single-user CLI operations.
pub fn primary_user(db: &Db) -> Result<(Id, String)> {
    db.with_conn_sync(users::first_user)
        .and_then(|o| o.ok_or(bk_core::Error::NotFound))
}

pub fn human_bytes(n: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut v = n as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    format!("{v:.1} {}", UNITS[i])
}

// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

//! `bookmark backup` — write a compressed, checksummed snapshot.

use std::path::PathBuf;

use crate::cmd::common;
use crate::init_tracing;

pub async fn run(out: Option<&str>) -> i32 {
    init_tracing();
    let cfg = common::load_config();
    if let Err(e) = bk_core::state_dir(&cfg.data_dir) {
        eprintln!("error: {e}");
        return 1;
    }
    let out_path: Option<PathBuf> = out.map(PathBuf::from);
    match bk_ops::backup::create(&cfg.db_path(), out_path.as_deref()) {
        Ok(p) => {
            let size = std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
            println!(
                "backup written: {} ({})",
                p.display(),
                common::human_bytes(size)
            );
            0
        }
        Err(e) => {
            eprintln!("error: {e}");
            1
        }
    }
}

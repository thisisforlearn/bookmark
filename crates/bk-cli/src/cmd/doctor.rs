// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

//! `bookmark doctor` — health check with actionable detail.

use crate::cmd::common;
use crate::init_tracing;

pub async fn run() -> i32 {
    init_tracing();
    let cfg = common::load_config();
    let report = bk_ops::DoctorReport::run(&cfg);
    println!("bookmark doctor\n");
    for c in &report.checks {
        let mark = if c.ok { "ok  " } else { "FAIL" };
        println!("  [{mark}] {:<22} {}", c.name, c.detail);
    }
    println!();
    if report.all_ok() {
        println!("everything looks good.");
        0
    } else {
        println!("some checks failed — see FAIL lines above.");
        1
    }
}

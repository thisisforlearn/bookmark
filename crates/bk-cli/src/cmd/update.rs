// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

//! `bookmark update` — check for a newer release.

use crate::init_tracing;

pub async fn run() -> i32 {
    init_tracing();
    println!("bookmark {}", env!("CARGO_PKG_VERSION"));
    println!();
    println!("To update, re-run the installer:");
    println!(
        "  curl -fsSL https://github.com/thisisforlearn/bookmark/releases/latest/download/install.sh | bash"
    );
    println!();
    println!("Your database is preserved across upgrades (forward-only migrations).");
    0
}

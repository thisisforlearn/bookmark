// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

//! `bookmark` CLI library.

//! Shared clap types and subcommand dispatch.

pub mod cli;
pub mod cmd;

/// Initialise tracing from `$BOOKMARK_LOG_LEVEL` (default `warn`).
pub fn init_tracing() {
    let filter = std::env::var("BOOKMARK_LOG_LEVEL").unwrap_or_else(|_| "warn".to_string());
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .try_init();
}

/// Exit code helper: 0 success, 1 error.
pub type ExitCode = i32;

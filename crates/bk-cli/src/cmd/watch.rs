// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

//! `bookmark watch` — supervisor: restart the server on crash with backoff.

use std::time::Duration;

use crate::init_tracing;

pub async fn run(max_restarts: u32) -> i32 {
    init_tracing();
    let exe = match std::env::current_exe() {
        Ok(e) => e,
        Err(e) => {
            eprintln!("error: cannot locate executable: {e}");
            return 1;
        }
    };
    let cfg = std::env::var("BOOKMARK_CONFIG").ok();

    let mut restarts = 0u32;
    let mut backoff = 1u64;
    loop {
        let mut cmd = tokio::process::Command::new(&exe);
        cmd.arg("serve");
        if let Some(c) = &cfg {
            cmd.env("BOOKMARK_CONFIG", c);
        }
        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                eprintln!("error: cannot start server: {e}");
                return 1;
            }
        };
        let status = child.wait().await;
        let code = status.as_ref().ok().and_then(|s| s.code()).unwrap_or(-1);
        eprintln!("bookmark watch: server exited ({code}); restarting in {backoff}s");

        if max_restarts > 0 && restarts + 1 >= max_restarts {
            eprintln!("bookmark watch: reached --max-restarts {max_restarts}; stopping");
            notify(
                "bookmark stopped",
                &format!("server exited {max_restarts} times"),
            );
            return 1;
        }
        restarts += 1;
        if restarts >= 3 {
            notify(
                "bookmark keeps crashing",
                &format!("{restarts} restarts; last code {code}"),
            );
        }
        tokio::time::sleep(Duration::from_secs(backoff)).await;
        backoff = (backoff * 2).min(30);
    }
}

fn notify(title: &str, body: &str) {
    eprintln!("[{title}] {body}");
    // Best-effort Termux notification.
    let _ = std::process::Command::new("termux-notification")
        .arg("--title")
        .arg(title)
        .arg("--content")
        .arg(body)
        .status();
}

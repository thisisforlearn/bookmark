// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

//! `bookmark serve` — run the HTTP server.

use bk_core::Config;

use crate::cmd::common;
use crate::init_tracing;

pub async fn run(host: String, port: u16) -> i32 {
    init_tracing();
    let cfg = common::load_config();
    let cfg = Config { host, port, ..cfg };

    let state = match bk_serve::build_state(cfg) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: {e}");
            return 1;
        }
    };
    // A server with no accounts is a dead end: nobody can sign in and the
    // login page has nothing to authenticate against.
    if state.db.user_count() == 0 {
        eprintln!("error: no accounts exist yet — the server would have nothing to sign in to.");
        eprintln!();
        eprintln!("  data dir: {}", state.cfg.data_dir.display());
        eprintln!();
        eprintln!("Run this first:");
        eprintln!("    bookmark init");
        eprintln!();
        eprintln!("It creates the database and prints an admin password once.");
        return 1;
    }

    let addr = format!("{}:{}", state.cfg.host, state.cfg.port);

    let listener = match tokio::net::TcpListener::bind(&addr).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("error: cannot bind {addr}: {e}");
            return 1;
        }
    };

    println!("bookmark listening on http://{addr}");
    println!("  data: {}", state.cfg.data_dir.display());
    if state.models.degraded() {
        println!("  models: not all installed — search falls back to lexical");
    }

    let app = bk_serve::router(state);
    if let Err(e) = axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
    {
        eprintln!("error: server stopped: {e}");
        return 1;
    }
    0
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut sig) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            sig.recv().await;
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    println!("\nshutting down…");
}

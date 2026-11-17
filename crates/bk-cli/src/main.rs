// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

//! `bookmark` CLI entry point.

use bk_cli::cli::{Cli, Command};
use clap::Parser;

fn main() {
    let cli = Cli::parse();
    let rt = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("error: cannot start async runtime: {e}");
            std::process::exit(1);
        }
    };
    let rc = rt.block_on(dispatch(cli.command));
    std::process::exit(rc);
}

async fn dispatch(cmd: Command) -> i32 {
    match cmd {
        Command::Init => bk_cli::cmd::init::run().await,
        Command::Serve { host, port } => bk_cli::cmd::serve::run(host, port).await,
        Command::Watch { max_restarts } => bk_cli::cmd::watch::run(max_restarts).await,
        Command::User { command } => bk_cli::cmd::user::run(command).await,
        Command::Add { target } => bk_cli::cmd::add::run(&target).await,
        Command::Search { query } => bk_cli::cmd::search::run(&query).await,
        Command::Crawl { all, limit } => bk_cli::cmd::crawl::run(all, limit).await,
        Command::Index { rebuild } => bk_cli::cmd::index::run(rebuild).await,
        Command::Export { out } => bk_cli::cmd::export::run(&out).await,
        Command::Import { input } => bk_cli::cmd::import::run(&input).await,
        Command::Backup { out } => bk_cli::cmd::backup::run(out.as_deref()).await,
        Command::Model { command } => bk_cli::cmd::model::run(command).await,
        Command::Device { command } => bk_cli::cmd::device::run(command).await,
        Command::Doctor => bk_cli::cmd::doctor::run().await,
        Command::Service { command } => bk_cli::cmd::service::run(command).await,
        Command::Update => bk_cli::cmd::update::run().await,
    }
}

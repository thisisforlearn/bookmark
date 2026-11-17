// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

//! Shared clap argument types.

use clap::Parser;

#[derive(Parser)]
#[command(
    name = "bookmark",
    version,
    about = "Self-hostable multi-user bookmark server."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(clap::Subcommand)]
pub enum Command {
    /// Initialise the data directory and create the first admin user.
    Init,
    /// Run the HTTP server.
    Serve {
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        #[arg(long, default_value_t = 8080)]
        port: u16,
    },
    /// Supervisor loop: restart the server on crash.
    Watch {
        #[arg(long, default_value_t = 0)]
        max_restarts: u32,
    },
    /// User management.
    User {
        #[command(subcommand)]
        command: UserCommand,
    },
    /// Add a bookmark (URL or plain text).
    Add { target: String },
    /// Search the index.
    Search { query: String },
    /// Crawl bookmarks.
    Crawl {
        #[arg(long)]
        all: bool,
        #[arg(long)]
        limit: Option<usize>,
    },
    /// Rebuild the search index.
    Index {
        #[arg(long)]
        rebuild: bool,
    },
    /// Export bookmarks to a file.
    Export { out: String },
    /// Import bookmarks from a file.
    Import { input: String },
    /// Snapshot the database.
    Backup { out: Option<String> },
    /// Model management.
    Model {
        #[command(subcommand)]
        command: ModelCommand,
    },
    /// Device tokens for the Android app.
    Device {
        #[command(subcommand)]
        command: DeviceCommand,
    },
    /// Diagnose the install.
    Doctor,
    /// Service management (systemd / termux / windows).
    Service {
        #[command(subcommand)]
        command: ServiceCommand,
    },
    /// Check for and apply updates.
    Update,
}

#[derive(clap::Subcommand)]
pub enum UserCommand {
    Add {
        username: String,
        #[arg(long)]
        admin: bool,
    },
    List,
    Rm {
        username: String,
    },
}

#[derive(clap::Subcommand)]
pub enum ModelCommand {
    List,
    Verify,
    /// Download the prebuilt default models into the models directory.
    Install {
        /// Base URL or local directory to copy from.
        #[arg(long)]
        from: Option<String>,
    },
    /// Train a model. Only available in the `train` feature build.
    Train {
        model: String,
        #[arg(long, default_value = "wgpu")]
        backend: String,
        #[arg(long, default_value = "full")]
        mode: String,
    },
    /// Export a trained model to .bkmdl.
    Export {
        out: String,
    },
}

#[derive(clap::Subcommand)]
pub enum DeviceCommand {
    /// Create a token and print it (used by the Android app on first run).
    Add {
        name: String,
    },
    List,
    Revoke {
        id: i64,
    },
}

#[derive(clap::Subcommand)]
pub enum ServiceCommand {
    Install,
    Start,
    Stop,
    Status,
    Logs,
}

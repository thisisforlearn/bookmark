// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

//! `bookmark device` — manage Android app tokens.
//!
//! The app itself calls `bookmark device add app` against the embedded server on
//! first run and stores the printed token, so the user never types a password.

use crate::cli::DeviceCommand;
use bk_db::devices;

use crate::cmd::common;
use crate::init_tracing;

pub async fn run(command: DeviceCommand) -> i32 {
    init_tracing();
    let db = match common::open_db() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("error: {e}");
            return 1;
        }
    };
    match command {
        DeviceCommand::Add { name } => {
            let (user_id, _) = match common::primary_user(&db) {
                Ok(u) => u,
                Err(_) => {
                    eprintln!("error: no users yet — run `bookmark init`");
                    return 1;
                }
            };
            match db.with_conn_sync(|c| devices::create(c, user_id, &name)) {
                Ok(token) => {
                    // The token is the only thing on stdout, so the app can read it.
                    println!("{token}");
                    0
                }
                Err(e) => {
                    eprintln!("error: {e}");
                    1
                }
            }
        }
        DeviceCommand::List => match db
            .with_conn_sync(|c| common::primary_user(&db).and_then(|(u, _)| devices::list(c, u)))
        {
            Ok(rows) => {
                if rows.is_empty() {
                    println!("no devices paired");
                }
                for (id, name, created, last_seen) in rows {
                    let seen = last_seen
                        .map(bk_serve::views::date_str)
                        .unwrap_or_else(|| "never".into());
                    println!(
                        "{id:>4}  {name:<20} paired {}  last seen {seen}",
                        bk_serve::views::date_str(created)
                    );
                }
                0
            }
            Err(e) => {
                eprintln!("error: {e}");
                1
            }
        },
        DeviceCommand::Revoke { id } => {
            let (user_id, _) = match common::primary_user(&db) {
                Ok(u) => u,
                Err(_) => return 1,
            };
            match db.with_conn_sync(|c| devices::revoke(c, user_id, bk_core::Id::from_i64(id))) {
                Ok(true) => {
                    println!("revoked device {id}");
                    0
                }
                Ok(false) => {
                    eprintln!("error: no such device");
                    1
                }
                Err(e) => {
                    eprintln!("error: {e}");
                    1
                }
            }
        }
    }
}

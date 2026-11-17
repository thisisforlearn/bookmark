// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

//! `bookmark user` — manage accounts.

use crate::cli::UserCommand;
use bk_db::users;

use crate::cmd::common;
use crate::init_tracing;

pub async fn run(command: UserCommand) -> i32 {
    init_tracing();
    let db = match common::open_db() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("error: {e}");
            return 1;
        }
    };
    match command {
        UserCommand::Add { username, admin } => {
            let password = std::env::var("BOOKMARK_USER_PASSWORD").unwrap_or_default();
            let password = if password.is_empty() {
                eprintln!("error: set BOOKMARK_USER_PASSWORD to the new user's password");
                return 1;
            } else {
                password
            };
            match db.with_conn_sync(|c| users::create(c, &username, &password, admin)) {
                Ok(id) => {
                    println!(
                        "created user `{username}` (id {id}, role {})",
                        if admin { "admin" } else { "user" }
                    );
                    0
                }
                Err(e) => {
                    eprintln!("error: {e}");
                    1
                }
            }
        }
        UserCommand::List => match db.with_conn_sync(users::list) {
            Ok(rows) => {
                if rows.is_empty() {
                    println!("no users yet — run `bookmark init`");
                }
                for (id, name, role, created) in rows {
                    println!(
                        "{id:>4}  {name:<24} {role:<6} {}",
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
        UserCommand::Rm { username } => match db.with_conn_sync(|c| users::delete(c, &username)) {
            Ok(true) => {
                println!("removed `{username}`");
                0
            }
            Ok(false) => {
                eprintln!("error: no such user");
                1
            }
            Err(e) => {
                eprintln!("error: {e}");
                1
            }
        },
    }
}

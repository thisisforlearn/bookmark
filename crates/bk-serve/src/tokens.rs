// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

//! Bearer-token authentication for the Android app.

use axum::http::HeaderMap;

use bk_db::devices;

use crate::AppState;
use crate::auth::{self, AuthUser, Role};

/// Resolve `Authorization: Bearer <token>` to a user.
pub fn bearer_user(state: &AppState, headers: &HeaderMap) -> Option<AuthUser> {
    let raw = headers
        .get(axum::http::header::AUTHORIZATION)?
        .to_str()
        .ok()?;
    let token = raw
        .strip_prefix("Bearer ")
        .or_else(|| raw.strip_prefix("bearer "))?
        .trim();
    if token.is_empty() {
        return None;
    }
    let user_id = state
        .db
        .with_conn_sync(|c| devices::validate(c, token))
        .ok()
        .flatten()?;
    let user = state
        .db
        .with_conn_sync(|c| bk_db::users::find_by_id(c, user_id))
        .ok()
        .flatten()?;
    Some(AuthUser {
        id: user.id,
        username: user.username,
        role: if user.role == "admin" {
            Role::Admin
        } else {
            Role::User
        },
    })
}

/// Accept either a browser session cookie or a device bearer token.
pub fn any_user(state: &AppState, headers: &HeaderMap) -> Option<AuthUser> {
    auth::current_user(state, headers).or_else(|| bearer_user(state, headers))
}

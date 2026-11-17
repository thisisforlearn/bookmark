// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

//! Public share links. Tokens are 24 random bytes, base64url encoded, and can
//! be revoked or given an expiry.

use base64::Engine as _;
use rand::RngCore;

use bk_core::{Id, Result};

#[derive(Clone, Debug)]
pub struct Share {
    pub id: Id,
    pub user_id: Id,
    pub bookmark_id: Id,
    pub token: String,
    pub expires_at: Option<i64>,
}

fn new_token() -> String {
    let mut b = [0u8; 24];
    rand::rng().fill_bytes(&mut b);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(b)
}

pub fn create(
    conn: &rusqlite::Connection,
    user_id: Id,
    bookmark_id: Id,
    expires_at: Option<i64>,
) -> Result<Share> {
    let token = new_token();
    conn.execute(
        "INSERT INTO shares(user_id, bookmark_id, token, expires_at) VALUES (?, ?, ?, ?)",
        rusqlite::params![user_id.inner(), bookmark_id.inner(), token, expires_at],
    )?;
    Ok(Share {
        id: Id::from_i64(conn.last_insert_rowid()),
        user_id,
        bookmark_id,
        token,
        expires_at,
    })
}

/// Resolve a share token to a bookmark, honouring expiry.
pub fn resolve(conn: &rusqlite::Connection, token: &str) -> Result<Option<Share>> {
    let mut stmt = conn.prepare(
        "SELECT id, user_id, bookmark_id, token, expires_at FROM shares WHERE token = ?",
    )?;
    let row = stmt.query_row([token], |r| {
        Ok(Share {
            id: Id::from_i64(r.get(0)?),
            user_id: Id::from_i64(r.get(1)?),
            bookmark_id: Id::from_i64(r.get(2)?),
            token: r.get(3)?,
            expires_at: r.get(4)?,
        })
    });
    match row {
        Ok(s) => {
            if let Some(exp) = s.expires_at
                && exp < bk_core::now()
            {
                return Ok(None);
            }
            Ok(Some(s))
        }
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

pub fn revoke(conn: &rusqlite::Connection, user_id: Id, id: Id) -> Result<bool> {
    let n = conn.execute(
        "DELETE FROM shares WHERE id = ? AND user_id = ?",
        rusqlite::params![id.inner(), user_id.inner()],
    )?;
    Ok(n > 0)
}

pub fn list_for_bookmark(
    conn: &rusqlite::Connection,
    user_id: Id,
    bookmark_id: Id,
) -> Result<Vec<Share>> {
    let mut stmt = conn.prepare(
        "SELECT id, user_id, bookmark_id, token, expires_at FROM shares WHERE user_id = ? AND bookmark_id = ? ORDER BY id",
    )?;
    let rows = stmt.query_map(
        rusqlite::params![user_id.inner(), bookmark_id.inner()],
        |r| {
            Ok(Share {
                id: Id::from_i64(r.get(0)?),
                user_id: Id::from_i64(r.get(1)?),
                bookmark_id: Id::from_i64(r.get(2)?),
                token: r.get(3)?,
                expires_at: r.get(4)?,
            })
        },
    )?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|e| e.into())
}

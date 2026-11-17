// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

//! Device tokens for the Android app. A token is 32 random bytes; only its
//! SHA-256 is stored, exactly like session tokens.

use rand::RngCore;
use sha2::{Digest, Sha256};

use base64::Engine as _;
use bk_core::{Id, Result};

pub fn hash_token(token: &str) -> String {
    let mut h = Sha256::new();
    h.update(token.as_bytes());
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(h.finalize())
}

/// Create a device and return the plaintext token (shown once).
pub fn create(conn: &rusqlite::Connection, user_id: Id, name: &str) -> Result<String> {
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    let token = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);
    conn.execute(
        "INSERT INTO devices(user_id, name, token_hash, created_at) VALUES (?, ?, ?, ?)",
        rusqlite::params![user_id.inner(), name, hash_token(&token), bk_core::now()],
    )?;
    Ok(token)
}

/// Resolve a bearer token to a user id and bump `last_seen`.
pub fn validate(conn: &rusqlite::Connection, token: &str) -> Result<Option<Id>> {
    if token.is_empty() {
        return Ok(None);
    }
    let mut stmt = conn.prepare("SELECT id, user_id FROM devices WHERE token_hash = ?")?;
    let row = stmt.query_row([hash_token(token)], |r| {
        Ok((Id::from_i64(r.get(0)?), Id::from_i64(r.get(1)?)))
    });
    match row {
        Ok((device_id, user_id)) => {
            conn.execute(
                "UPDATE devices SET last_seen = ? WHERE id = ?",
                rusqlite::params![bk_core::now(), device_id.inner()],
            )?;
            Ok(Some(user_id))
        }
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// `(id, name, created_at, last_seen)`
pub type DeviceRow = (Id, String, i64, Option<i64>);

pub fn list(conn: &rusqlite::Connection, user_id: Id) -> Result<Vec<DeviceRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, created_at, last_seen FROM devices WHERE user_id = ? ORDER BY created_at DESC",
    )?;
    let rows = stmt.query_map([user_id.inner()], |r| {
        Ok((Id::from_i64(r.get(0)?), r.get(1)?, r.get(2)?, r.get(3)?))
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|e| e.into())
}

pub fn revoke(conn: &rusqlite::Connection, user_id: Id, device_id: Id) -> Result<bool> {
    let n = conn.execute(
        "DELETE FROM devices WHERE id = ? AND user_id = ?",
        rusqlite::params![device_id.inner(), user_id.inner()],
    )?;
    Ok(n > 0)
}

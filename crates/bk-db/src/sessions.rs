// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

//! Sessions. A session token is 32 random bytes; only its SHA-256 is stored,
//! so a database leak does not expose live tokens. Sessions rotate on login
//! and expire after 30 days.

use rand::RngCore;
use sha2::{Digest, Sha256};

use base64::Engine as _;
use bk_core::{Id, Result};

const SESSION_TTL_DAYS: i64 = 30;

/// Generate a fresh session token (base64url, 32 bytes).
pub fn generate_token() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn hash_token(token: &str) -> String {
    let mut h = Sha256::new();
    h.update(token.as_bytes());
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(h.finalize())
}

pub fn create(
    conn: &rusqlite::Connection,
    user_id: Id,
    ip: Option<&str>,
    ua: Option<&str>,
) -> Result<String> {
    let token = generate_token();
    let hash = hash_token(&token);
    let now = bk_core::now();
    let expires = now + SESSION_TTL_DAYS * 86_400;
    conn.execute(
        "INSERT INTO sessions(user_id, token_hash, created_at, expires_at, ip, ua) VALUES (?, ?, ?, ?, ?, ?)",
        rusqlite::params![user_id.inner(), hash, now, expires, ip, ua],
    )?;
    Ok(token)
}

/// Validate a session token. Returns the user id on success.
pub fn validate(conn: &rusqlite::Connection, token: &str) -> Result<Option<Id>> {
    let hash = hash_token(token);
    let now = bk_core::now();
    let mut stmt =
        conn.prepare("SELECT user_id FROM sessions WHERE token_hash = ? AND expires_at > ?")?;
    let row = stmt.query_row([hash.as_str(), &now.to_string()], |r| {
        Ok(Id::from_i64(r.get(0)?))
    });
    match row {
        Ok(id) => Ok(Some(id)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Drop a session (logout).
pub fn destroy(conn: &rusqlite::Connection, token: &str) -> Result<()> {
    let hash = hash_token(token);
    conn.execute("DELETE FROM sessions WHERE token_hash = ?", [hash])?;
    Ok(())
}

/// Purge expired sessions. Called periodically.
pub fn purge(conn: &rusqlite::Connection) -> Result<usize> {
    let n = conn.execute(
        "DELETE FROM sessions WHERE expires_at < ?",
        [bk_core::now()],
    )?;
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn db() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(
            "CREATE TABLE users(id INTEGER PRIMARY KEY, username TEXT NOT NULL UNIQUE, pw_hash TEXT NOT NULL, role TEXT NOT NULL DEFAULT 'user', created_at INTEGER NOT NULL, settings_json TEXT NOT NULL DEFAULT '{}');
             CREATE TABLE sessions(id INTEGER PRIMARY KEY, user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE, token_hash TEXT NOT NULL UNIQUE, created_at INTEGER NOT NULL, expires_at INTEGER NOT NULL, ip TEXT, ua TEXT);",
        ).unwrap();
        c.execute("INSERT INTO users(id, username, pw_hash, role, created_at, settings_json) VALUES (1, 'alice', 'x', 'user', 0, '{}')", []).unwrap();
        c
    }

    #[test]
    fn create_validate_destroy() {
        let c = db();
        let token = create(&c, Id::from_i64(1), None, None).unwrap();
        assert_eq!(validate(&c, &token).unwrap(), Some(Id::from_i64(1)));
        assert!(validate(&c, "bogus").unwrap().is_none());
        destroy(&c, &token).unwrap();
        assert!(validate(&c, &token).unwrap().is_none());
    }
}

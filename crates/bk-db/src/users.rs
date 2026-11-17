// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

//! User accounts. Argon2id with OWASP-recommended parameters (19 MiB memory,
//! t=2, p=1). Passwords are stored as PHC strings.

use argon2::password_hash::rand_core::OsRng;
use argon2::{Argon2, PasswordHasher, password_hash::SaltString};

use bk_core::{Id, Result};

#[derive(Clone, Debug)]
pub struct User {
    pub id: Id,
    pub username: String,
    pub role: String,
    pub created_at: i64,
    pub settings_json: String,
}

/// Create a user. Returns the new user id. Returns `Err(Conflict)` if the
/// username is taken.
pub fn create(
    conn: &rusqlite::Connection,
    username: &str,
    password: &str,
    is_admin: bool,
) -> Result<Id> {
    if username.is_empty() || password.is_empty() {
        return Err(bk_core::Error::Validation(
            "username and password are required".into(),
        ));
    }
    if username.len() > 64 {
        return Err(bk_core::Error::Validation("username too long".into()));
    }
    let hash = hash_password(password)?;
    let now = bk_core::now();
    let role = if is_admin { "admin" } else { "user" };
    conn.execute(
        "INSERT INTO users(username, pw_hash, role, created_at, settings_json) VALUES (?, ?, ?, ?, '{}')",
        rusqlite::params![username, hash, role, now],
    )?;
    Ok(Id::from_i64(conn.last_insert_rowid()))
}

/// Hash a password for storage. Public so the settings flow can rotate it.
pub fn hash_for_reset(password: &str) -> Result<String> {
    hash_password(password)
}

/// Replace a user's password.
pub fn set_password(conn: &rusqlite::Connection, user_id: Id, password: &str) -> Result<()> {
    let hash = hash_password(password)?;
    conn.execute(
        "UPDATE users SET pw_hash = ? WHERE id = ?",
        rusqlite::params![hash, user_id.inner()],
    )?;
    Ok(())
}

fn hash_password(password: &str) -> Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let hash = argon2
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| bk_core::Error::Internal(format!("argon2: {e}")))?;
    Ok(hash.to_string())
}

/// Verify a password against a stored PHC hash. Constant-time via argon2.
pub fn verify_password(stored: &str, password: &str) -> Result<bool> {
    use argon2::PasswordVerifier;
    let argon2 = Argon2::default();
    let parsed = match argon2::PasswordHash::new(stored) {
        Ok(p) => p,
        Err(_) => return Ok(false),
    };
    match argon2.verify_password(password.as_bytes(), &parsed) {
        Ok(()) => Ok(true),
        Err(_) => Ok(false),
    }
}

pub fn find_by_username(
    conn: &rusqlite::Connection,
    username: &str,
) -> Result<Option<(Id, String, String)>> {
    let mut stmt = conn.prepare("SELECT id, pw_hash, role FROM users WHERE username = ?")?;
    let row = stmt.query_row([username], |r| {
        Ok((Id::from_i64(r.get(0)?), r.get(1)?, r.get(2)?))
    });
    match row {
        Ok(r) => Ok(Some(r)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

pub fn find_by_id(conn: &rusqlite::Connection, id: Id) -> Result<Option<User>> {
    let mut stmt = conn
        .prepare("SELECT id, username, role, created_at, settings_json FROM users WHERE id = ?")?;
    let row = stmt.query_row([id.inner()], |r| {
        Ok(User {
            id: Id::from_i64(r.get(0)?),
            username: r.get(1)?,
            role: r.get(2)?,
            created_at: r.get(3)?,
            settings_json: r.get(4)?,
        })
    });
    match row {
        Ok(u) => Ok(Some(u)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

pub fn count(conn: &rusqlite::Connection) -> Result<i64> {
    let n: i64 = conn.query_row("SELECT COUNT(*) FROM users", [], |r| r.get(0))?;
    Ok(n)
}

pub fn list(conn: &rusqlite::Connection) -> Result<Vec<(Id, String, String, i64)>> {
    let mut stmt =
        conn.prepare("SELECT id, username, role, created_at FROM users ORDER BY created_at ASC")?;
    let rows = stmt.query_map([], |r| {
        Ok((Id::from_i64(r.get(0)?), r.get(1)?, r.get(2)?, r.get(3)?))
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|e| e.into())
}

pub fn set_settings(conn: &rusqlite::Connection, user_id: Id, json: &str) -> Result<()> {
    conn.execute(
        "UPDATE users SET settings_json = ? WHERE id = ?",
        rusqlite::params![json, user_id.inner()],
    )?;
    Ok(())
}
/// The first user created (used by CLI commands that need a default identity).
pub fn first_user(conn: &rusqlite::Connection) -> Result<Option<(Id, String)>> {
    let mut stmt = conn.prepare("SELECT id, username FROM users ORDER BY id ASC LIMIT 1")?;
    let row = stmt.query_row([], |r| {
        Ok((Id::from_i64(r.get(0)?), r.get::<_, String>(1)?))
    });
    match row {
        Ok(v) => Ok(Some(v)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Delete a user by name. Returns true if a row was removed.
pub fn delete(conn: &rusqlite::Connection, username: &str) -> Result<bool> {
    let n = conn.execute("DELETE FROM users WHERE username = ?", [username])?;
    Ok(n > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn db() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch("CREATE TABLE users(id INTEGER PRIMARY KEY, username TEXT NOT NULL UNIQUE, pw_hash TEXT NOT NULL, role TEXT NOT NULL DEFAULT 'user', created_at INTEGER NOT NULL, settings_json TEXT NOT NULL DEFAULT '{}');").unwrap();
        c
    }

    #[test]
    fn create_and_verify() {
        let c = db();
        let id = create(&c, "alice", "hunter2", false).unwrap();
        let (_, hash, _) = find_by_username(&c, "alice").unwrap().unwrap();
        assert!(verify_password(&hash, "hunter2").unwrap());
        assert!(!verify_password(&hash, "wrong").unwrap());
        assert_eq!(id.inner(), 1);
    }

    #[test]
    fn duplicate_username() {
        let c = db();
        create(&c, "alice", "hunter2", false).unwrap();
        assert!(matches!(
            create(&c, "alice", "x", false),
            Err(bk_core::Error::Conflict(_))
        ));
    }
}

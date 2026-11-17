// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

//! One-time pairing codes. Eight characters from an unambiguous alphabet, valid
//! for ten minutes and consumed on first use.

use rand::Rng;

use bk_core::{Id, Result};

/// No 0/O/1/I/L to avoid transcription mistakes.
const ALPHABET: &[u8] = b"23456789ABCDEFGHJKMNPQRSTUVWXYZ";
const LEN: usize = 8;
const TTL_SECS: i64 = 600;

pub fn generate() -> String {
    let mut rng = rand::rng();
    (0..LEN)
        .map(|_| ALPHABET[rng.random_range(0..ALPHABET.len())] as char)
        .collect()
}

/// Create a code for a user. Also clears that user's expired codes.
pub fn create(conn: &rusqlite::Connection, user_id: Id) -> Result<String> {
    let now = bk_core::now();
    conn.execute("DELETE FROM pair_codes WHERE expires_at < ?", [now])?;
    conn.execute(
        "DELETE FROM pair_codes WHERE user_id = ?",
        [user_id.inner()],
    )?;
    let mut code = generate();
    // Extremely unlikely, but keep the primary key unique.
    for _ in 0..4 {
        match conn.execute(
            "INSERT INTO pair_codes(code, user_id, expires_at) VALUES (?, ?, ?)",
            rusqlite::params![code, user_id.inner(), now + TTL_SECS],
        ) {
            Ok(_) => return Ok(code),
            Err(_) => code = generate(),
        }
    }
    Err(bk_core::Error::Internal(
        "could not allocate a pairing code".into(),
    ))
}

/// Consume a code and return the user it belongs to.
pub fn consume(conn: &rusqlite::Connection, code: &str) -> Result<Option<Id>> {
    let code = code.trim().to_uppercase();
    let mut stmt = conn.prepare("SELECT user_id, expires_at FROM pair_codes WHERE code = ?")?;
    let row = stmt.query_row([code.as_str()], |r| {
        Ok((Id::from_i64(r.get(0)?), r.get::<_, i64>(1)?))
    });
    match row {
        Ok((user_id, expires_at)) => {
            if expires_at < bk_core::now() {
                return Ok(None);
            }
            conn.execute("DELETE FROM pair_codes WHERE code = ?", [code])?;
            Ok(Some(user_id))
        }
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> rusqlite::Connection {
        let mut c = rusqlite::Connection::open_in_memory().unwrap();
        crate::migrate::apply(&mut c).unwrap();
        c.execute(
            "INSERT INTO users(id, username, pw_hash, role, created_at, settings_json)
             VALUES (1,'a','x','user',0,'{}')",
            [],
        )
        .unwrap();
        c
    }

    #[test]
    fn single_use_and_expiry() {
        let c = db();
        let u = Id::from_i64(1);
        let code = create(&c, u).unwrap();
        assert_eq!(code.len(), LEN);
        assert_eq!(consume(&c, &code).unwrap(), Some(u));
        assert_eq!(consume(&c, &code).unwrap(), None);

        let old = create(&c, u).unwrap();
        c.execute(
            "UPDATE pair_codes SET expires_at = 0 WHERE code = ?",
            [old.as_str()],
        )
        .unwrap();
        assert_eq!(consume(&c, &old).unwrap(), None);
    }

    #[test]
    fn device_token_round_trip() {
        let c = db();
        let u = Id::from_i64(1);
        let token = crate::devices::create(&c, u, "phone").unwrap();
        assert_eq!(crate::devices::validate(&c, &token).unwrap(), Some(u));
        assert_eq!(crate::devices::validate(&c, "nope").unwrap(), None);
    }
}

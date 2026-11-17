// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

//! Key/value metadata (`schema_version`, model ids, last index time).

use bk_core::Result;

pub fn get(conn: &rusqlite::Connection, key: &str) -> Result<Option<String>> {
    let mut stmt = conn.prepare("SELECT value FROM meta WHERE key = ?")?;
    let row = stmt.query_row([key], |r| r.get::<_, String>(0));
    match row {
        Ok(v) => Ok(Some(v)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

pub fn set(conn: &rusqlite::Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO meta(key, value) VALUES (?, ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        rusqlite::params![key, value],
    )?;
    Ok(())
}

pub fn bump(conn: &rusqlite::Connection, key: &str) -> Result<i64> {
    let cur: i64 = get(conn, key)?.and_then(|v| v.parse().ok()).unwrap_or(0);
    let next = cur + 1;
    set(conn, key, &next.to_string())?;
    Ok(next)
}

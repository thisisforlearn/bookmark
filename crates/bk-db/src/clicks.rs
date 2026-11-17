// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

//! Click log — the training labels for the LTR re-ranker.

use bk_core::{Id, Result};

pub fn record(
    conn: &rusqlite::Connection,
    user_id: Id,
    query: &str,
    bookmark_id: Id,
    rank: i64,
) -> Result<()> {
    conn.execute(
        "INSERT INTO clicks(user_id, query, bookmark_id, rank, ts) VALUES (?, ?, ?, ?, ?)",
        rusqlite::params![
            user_id.inner(),
            query,
            bookmark_id.inner(),
            rank,
            bk_core::now()
        ],
    )?;
    Ok(())
}

/// Per-(query, bookmark) click counts for a user. Used to build LTR features.
pub fn counts(
    conn: &rusqlite::Connection,
    user_id: Id,
    limit: usize,
) -> Result<Vec<(String, Id, i64)>> {
    let mut stmt = conn.prepare(
        "SELECT query, bookmark_id, COUNT(*) FROM clicks
         WHERE user_id = ? GROUP BY query, bookmark_id ORDER BY COUNT(*) DESC LIMIT ?",
    )?;
    let rows = stmt.query_map(rusqlite::params![user_id.inner(), limit as i64], |r| {
        Ok((
            r.get::<_, String>(0)?,
            Id::from_i64(r.get(1)?),
            r.get::<_, i64>(2)?,
        ))
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|e| e.into())
}

/// Total clicks for a user (used by the doctor / stats view).
pub fn total(conn: &rusqlite::Connection, user_id: Id) -> Result<i64> {
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM clicks WHERE user_id = ?",
        [user_id.inner()],
        |r| r.get(0),
    )?;
    Ok(n)
}

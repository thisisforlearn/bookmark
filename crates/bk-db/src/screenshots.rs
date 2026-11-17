// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

//! Screenshots: one per bookmark, stored as a deduplicated blob.

use bk_core::{Id, Result};

#[derive(Clone, Debug)]
pub struct Screenshot {
    pub bookmark_id: Id,
    pub mime: String,
    pub width: u32,
    pub height: u32,
    pub bytes: usize,
    pub sha256: String,
    pub blob: Vec<u8>,
}

/// Store (or replace) a bookmark's screenshot.
pub fn put(
    conn: &rusqlite::Connection,
    bookmark_id: Id,
    mime: &str,
    width: u32,
    height: u32,
    sha256: &str,
    blob: &[u8],
) -> Result<()> {
    conn.execute(
        "INSERT INTO screenshots(bookmark_id, mime, width, height, bytes, sha256, blob)
         VALUES (?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(bookmark_id) DO UPDATE SET
           mime = excluded.mime, width = excluded.width, height = excluded.height,
           bytes = excluded.bytes, sha256 = excluded.sha256, blob = excluded.blob",
        rusqlite::params![
            bookmark_id.inner(),
            mime,
            width,
            height,
            blob.len() as i64,
            sha256,
            blob
        ],
    )?;
    Ok(())
}

pub fn get(conn: &rusqlite::Connection, bookmark_id: Id) -> Result<Option<Screenshot>> {
    let mut stmt = conn.prepare(
        "SELECT bookmark_id, mime, width, height, bytes, sha256, blob FROM screenshots WHERE bookmark_id = ?",
    )?;
    let row = stmt.query_row([bookmark_id.inner()], |r| {
        Ok(Screenshot {
            bookmark_id: Id::from_i64(r.get(0)?),
            mime: r.get(1)?,
            width: r.get(2)?,
            height: r.get(3)?,
            bytes: r.get::<_, i64>(4)? as usize,
            sha256: r.get(5)?,
            blob: r.get(6)?,
        })
    });
    match row {
        Ok(s) => Ok(Some(s)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

pub fn delete(conn: &rusqlite::Connection, bookmark_id: Id) -> Result<()> {
    conn.execute(
        "DELETE FROM screenshots WHERE bookmark_id = ?",
        [bookmark_id.inner()],
    )?;
    Ok(())
}

pub fn exists(conn: &rusqlite::Connection, bookmark_id: Id) -> Result<bool> {
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM screenshots WHERE bookmark_id = ?",
        [bookmark_id.inner()],
        |r| r.get(0),
    )?;
    Ok(n > 0)
}
/// Which of `ids` have a screenshot. One query, used by the re-ranker.
pub fn existing_ids(
    conn: &rusqlite::Connection,
    ids: &[Id],
) -> Result<std::collections::HashSet<Id>> {
    let mut out = std::collections::HashSet::new();
    if ids.is_empty() {
        return Ok(out);
    }
    let mut placeholders = String::new();
    for (i, _) in ids.iter().enumerate() {
        if i > 0 {
            placeholders.push(',');
        }
        placeholders.push('?');
    }
    let sql = format!("SELECT bookmark_id FROM screenshots WHERE bookmark_id IN ({placeholders})");
    let mut stmt = conn.prepare(&sql)?;
    let params: Vec<rusqlite::types::Value> = ids
        .iter()
        .map(|i| rusqlite::types::Value::Integer(i.inner()))
        .collect();
    let rows = stmt.query_map(rusqlite::params_from_iter(params), |r| {
        Ok(Id::from_i64(r.get(0)?))
    })?;
    for r in rows {
        out.insert(r?);
    }
    Ok(out)
}

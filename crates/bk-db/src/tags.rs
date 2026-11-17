// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

//! Tags and the bookmark↔tag join table.

use bk_core::{Id, Result};

/// Find or create a tag by name for a user.
pub fn ensure(conn: &rusqlite::Connection, user_id: Id, name: &str) -> Result<Id> {
    let name = name.trim();
    if name.is_empty() {
        return Err(bk_core::Error::Validation("empty tag".into()));
    }
    if name.len() > 64 {
        return Err(bk_core::Error::Validation("tag too long".into()));
    }
    if let Some(id) = find(conn, user_id, name)? {
        return Ok(id);
    }
    conn.execute(
        "INSERT INTO tags(user_id, name) VALUES (?, ?)",
        rusqlite::params![user_id.inner(), name],
    )?;
    Ok(Id::from_i64(conn.last_insert_rowid()))
}

pub fn find(conn: &rusqlite::Connection, user_id: Id, name: &str) -> Result<Option<Id>> {
    let mut stmt = conn.prepare("SELECT id FROM tags WHERE user_id = ? AND name = ?")?;
    let row = stmt.query_row(rusqlite::params![user_id.inner(), name], |r| {
        Ok(Id::from_i64(r.get(0)?))
    });
    match row {
        Ok(id) => Ok(Some(id)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

pub fn attach(conn: &rusqlite::Connection, bookmark_id: Id, tag_id: Id) -> Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO bookmark_tags(bookmark_id, tag_id) VALUES (?, ?)",
        rusqlite::params![bookmark_id.inner(), tag_id.inner()],
    )?;
    Ok(())
}

pub fn detach(conn: &rusqlite::Connection, bookmark_id: Id, tag_id: Id) -> Result<()> {
    conn.execute(
        "DELETE FROM bookmark_tags WHERE bookmark_id = ? AND tag_id = ?",
        rusqlite::params![bookmark_id.inner(), tag_id.inner()],
    )?;
    Ok(())
}

/// Replace the full tag set on a bookmark.
pub fn set(
    conn: &rusqlite::Connection,
    user_id: Id,
    bookmark_id: Id,
    names: &[String],
) -> Result<()> {
    conn.execute(
        "DELETE FROM bookmark_tags WHERE bookmark_id = ?",
        rusqlite::params![bookmark_id.inner()],
    )?;
    for n in names {
        let tid = ensure(conn, user_id, n)?;
        attach(conn, bookmark_id, tid)?;
    }
    Ok(())
}

/// All tags for a bookmark.
pub fn for_bookmark(conn: &rusqlite::Connection, bookmark_id: Id) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT t.name FROM tags t JOIN bookmark_tags bt ON bt.tag_id = t.id WHERE bt.bookmark_id = ? ORDER BY t.name",
    )?;
    let rows = stmt.query_map([bookmark_id.inner()], |r| r.get::<_, String>(0))?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|e| e.into())
}

/// All tags for a user, with usage counts.
pub fn list(conn: &rusqlite::Connection, user_id: Id) -> Result<Vec<(String, i64)>> {
    let mut stmt = conn.prepare(
        "SELECT t.name, COUNT(bt.bookmark_id)
         FROM tags t LEFT JOIN bookmark_tags bt ON bt.tag_id = t.id
         WHERE t.user_id = ? GROUP BY t.id ORDER BY COUNT(bt.bookmark_id) DESC, t.name",
    )?;
    let rows = stmt.query_map([user_id.inner()], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|e| e.into())
}

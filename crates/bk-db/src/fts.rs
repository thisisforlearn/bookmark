// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

//! FTS5 query helpers. The lexical arm runs a `MATCH` against `bookmark_fts`
//! (unicode61 + porter) or `fts_tri` (trigram), falling back to `LIKE` for 1-2
//! character tokens that the trigram tokenizer cannot index.

use bk_core::{Id, Result};

/// Quote a raw term for safe embedding in an FTS5 MATCH expression.
pub fn quote(term: &str) -> String {
    let mut out = String::with_capacity(term.len() + 4);
    out.push('"');
    for ch in term.chars() {
        if ch == '"' {
            out.push('"');
        }
        out.push(ch);
    }
    out.push('"');
    out
}

/// Run a lexical MATCH against `bookmark_fts`, restricted to a user.
/// Returns `(bookmark_id, bm25)` where bm25 is SQLite's raw score (more negative
/// is better).
pub fn match_fts(
    conn: &rusqlite::Connection,
    user_id: Id,
    expr: &str,
    limit: usize,
) -> Result<Vec<(Id, f64)>> {
    if expr.trim().is_empty() {
        return Ok(Vec::new());
    }
    let sql = "SELECT b.id, bm25(bookmark_fts) AS score
               FROM bookmark_fts JOIN bookmarks b ON b.id = bookmark_fts.rowid
               WHERE bookmark_fts MATCH ? AND b.user_id = ?
               ORDER BY score LIMIT ?";
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(
        rusqlite::params![expr, user_id.inner(), limit as i64],
        |r| Ok((Id::from_i64(r.get(0)?), r.get::<_, f64>(1)?)),
    )?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|e| e.into())
}

/// Substring search via the trigram index (only useful for terms >= 3 chars).
pub fn match_trigram(
    conn: &rusqlite::Connection,
    user_id: Id,
    expr: &str,
    limit: usize,
) -> Result<Vec<(Id, f64)>> {
    if expr.trim().is_empty() {
        return Ok(Vec::new());
    }
    let sql = "SELECT b.id, bm25(fts_tri) AS score
               FROM fts_tri JOIN bookmarks b ON b.id = fts_tri.rowid
               WHERE fts_tri MATCH ? AND b.user_id = ?
               ORDER BY score LIMIT ?";
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(
        rusqlite::params![expr, user_id.inner(), limit as i64],
        |r| Ok((Id::from_i64(r.get(0)?), r.get::<_, f64>(1)?)),
    )?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|e| e.into())
}

/// `LIKE` fallback for 1-2 character tokens. Scans a user's rows only.
pub fn like_fallback(
    conn: &rusqlite::Connection,
    user_id: Id,
    term: &str,
    limit: usize,
) -> Result<Vec<(Id, f64)>> {
    let pattern = format!(
        "%{}%",
        term.replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_")
    );
    let sql = "SELECT id, 0.0 FROM bookmarks
               WHERE user_id = ? AND (title LIKE ? ESCAPE '\\' OR text LIKE ? ESCAPE '\\')
               ORDER BY pinned DESC, created_at DESC LIMIT ?";
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(
        rusqlite::params![user_id.inner(), pattern, pattern, limit as i64],
        |r| Ok((Id::from_i64(r.get(0)?), r.get::<_, f64>(1)?)),
    )?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|e| e.into())
}

/// Rebuild the FTS indexes from the base table (used by `bookmark index --rebuild`).
pub fn rebuild(conn: &rusqlite::Connection) -> Result<()> {
    conn.execute_batch(
        "INSERT INTO bookmark_fts(bookmark_fts) VALUES('rebuild');
         INSERT INTO fts_tri(fts_tri) VALUES('rebuild');",
    )?;
    Ok(())
}

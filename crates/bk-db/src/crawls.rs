// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

//! Crawl bookkeeping and retry scheduling with exponential backoff.

use bk_core::{Id, Result};

#[derive(Clone, Debug)]
pub struct CrawlRow {
    pub id: Id,
    pub bookmark_id: Id,
    pub http_status: Option<i64>,
    pub error_kind: Option<String>,
    pub attempts: i64,
    pub next_try_at: i64,
    pub bytes: i64,
    pub ms: i64,
}

/// Record a crawl attempt. On success `error_kind` is `None`; on failure the
/// next retry is scheduled with exponential backoff (capped at 24 h).
pub fn record(
    conn: &rusqlite::Connection,
    bookmark_id: Id,
    http_status: Option<u16>,
    error_kind: Option<&str>,
    bytes: i64,
    ms: i64,
) -> Result<()> {
    let attempts: i64 = conn
        .query_row(
            "SELECT attempts FROM crawls WHERE bookmark_id = ?",
            [bookmark_id.inner()],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let next_attempt = attempts + 1;
    let next_try_at = if error_kind.is_some() {
        let base = 30i64 * 60;
        let backoff = base.saturating_mul(1i64 << next_attempt.min(10));
        bk_core::now() + backoff.min(86_400)
    } else {
        0
    };
    conn.execute(
        "INSERT INTO crawls(bookmark_id, http_status, error_kind, attempts, next_try_at, bytes, ms)
         VALUES (?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(bookmark_id) DO UPDATE SET
           http_status = excluded.http_status,
           error_kind  = excluded.error_kind,
           attempts    = excluded.attempts,
           next_try_at = excluded.next_try_at,
           bytes       = excluded.bytes,
           ms          = excluded.ms",
        rusqlite::params![
            bookmark_id.inner(),
            http_status.map(|s| s as i64),
            error_kind,
            next_attempt,
            next_try_at,
            bytes,
            ms
        ],
    )?;
    Ok(())
}

/// Bookmarks that are due for a crawl (never crawled, or retry time reached).
pub fn due(conn: &rusqlite::Connection, user_id: Option<Id>, limit: usize) -> Result<Vec<Id>> {
    let now = bk_core::now();
    let sql = "SELECT b.id FROM bookmarks b
               LEFT JOIN crawls c ON c.bookmark_id = b.id
               WHERE (?1 IS NULL OR b.user_id = ?1)
                 AND (c.id IS NULL OR (c.next_try_at != 0 AND c.next_try_at <= ?2))
               ORDER BY b.created_at ASC LIMIT ?3";
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(
        rusqlite::params![user_id.map(|u| u.inner()), now, limit as i64],
        |r| Ok(Id::from_i64(r.get(0)?)),
    )?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|e| e.into())
}

pub fn set_next_try(conn: &rusqlite::Connection, bookmark_id: Id, next: i64) -> Result<()> {
    conn.execute(
        "INSERT INTO crawls(bookmark_id, attempts, next_try_at) VALUES (?, 0, ?)
         ON CONFLICT(bookmark_id) DO UPDATE SET next_try_at = excluded.next_try_at",
        rusqlite::params![bookmark_id.inner(), next],
    )?;
    Ok(())
}

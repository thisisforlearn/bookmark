// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

//! Bookmarks: the central record. One row per (user, normalized URL).

use bk_core::{Id, Result};

#[derive(Clone, Debug, Default)]
pub struct Bookmark {
    pub id: Id,
    pub user_id: Id,
    pub url_norm: String,
    pub url: String,
    pub title: String,
    pub description: String,
    pub text: String,
    pub text_hash: i64,
    pub lang: Option<String>,
    pub site: String,
    pub status: String,
    pub note: String,
    pub pinned: bool,
    pub created_at: i64,
    pub updated_at: i64,
    pub crawled_at: Option<i64>,
    /// 0..1 signal of how much real content the page had.
    pub quality: f32,
}

const COLS: &str = "id, user_id, url_norm, url, title, description, text, text_hash, lang, site, status, note, pinned, created_at, updated_at, crawled_at, quality";

fn row_to_bookmark(r: &rusqlite::Row<'_>) -> rusqlite::Result<Bookmark> {
    Ok(Bookmark {
        id: Id::from_i64(r.get(0)?),
        user_id: Id::from_i64(r.get(1)?),
        url_norm: r.get(2)?,
        url: r.get(3)?,
        title: r.get(4)?,
        description: r.get(5)?,
        text: r.get(6)?,
        text_hash: r.get(7)?,
        lang: r.get(8)?,
        site: r.get(9)?,
        status: r.get(10)?,
        note: r.get(11)?,
        pinned: r.get::<_, i64>(12)? != 0,
        created_at: r.get(13)?,
        updated_at: r.get(14)?,
        crawled_at: r.get(15)?,
        quality: r.get::<_, f64>(16).unwrap_or(1.0) as f32,
    })
}

/// Insert a new bookmark. Returns `Err(Conflict)` if `(user_id, url_norm)` exists.
pub fn insert(
    conn: &rusqlite::Connection,
    user_id: Id,
    url_norm: &str,
    url: &str,
    title: &str,
    text: &str,
) -> Result<Id> {
    let now = bk_core::now();
    let hash = bk_core::xxh3_str(text) as i64;
    let res = conn.execute(
        "INSERT INTO bookmarks(user_id, url_norm, url, title, text, text_hash, status, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, 'new', ?, ?)",
        rusqlite::params![user_id.inner(), url_norm, url, title, text, hash, now, now],
    );
    match res {
        Ok(_) => Ok(Id::from_i64(conn.last_insert_rowid())),
        Err(rusqlite::Error::SqliteFailure(e, _))
            if e.code == rusqlite::ErrorCode::ConstraintViolation =>
        {
            Err(bk_core::Error::Conflict("bookmark already exists".into()))
        }
        Err(e) => Err(e.into()),
    }
}

pub fn get(conn: &rusqlite::Connection, user_id: Id, id: Id) -> Result<Option<Bookmark>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLS} FROM bookmarks WHERE id = ? AND user_id = ?"
    ))?;
    let row = stmt.query_row(
        rusqlite::params![id.inner(), user_id.inner()],
        row_to_bookmark,
    );
    match row {
        Ok(b) => Ok(Some(b)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

pub fn get_by_url(
    conn: &rusqlite::Connection,
    user_id: Id,
    url_norm: &str,
) -> Result<Option<Bookmark>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLS} FROM bookmarks WHERE user_id = ? AND url_norm = ?"
    ))?;
    let row = stmt.query_row(
        rusqlite::params![user_id.inner(), url_norm],
        row_to_bookmark,
    );
    match row {
        Ok(b) => Ok(Some(b)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// List a user's bookmarks, newest first, pinned first.
pub fn list(
    conn: &rusqlite::Connection,
    user_id: Id,
    limit: usize,
    offset: usize,
) -> Result<Vec<Bookmark>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLS} FROM bookmarks WHERE user_id = ? ORDER BY pinned DESC, created_at DESC LIMIT ? OFFSET ?"
    ))?;
    let rows = stmt.query_map(
        rusqlite::params![user_id.inner(), limit as i64, offset as i64],
        row_to_bookmark,
    )?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|e| e.into())
}

pub fn count(conn: &rusqlite::Connection, user_id: Id) -> Result<i64> {
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM bookmarks WHERE user_id = ?",
        [user_id.inner()],
        |r| r.get(0),
    )?;
    Ok(n)
}

pub fn delete(conn: &rusqlite::Connection, user_id: Id, id: Id) -> Result<bool> {
    let n = conn.execute(
        "DELETE FROM bookmarks WHERE id = ? AND user_id = ?",
        rusqlite::params![id.inner(), user_id.inner()],
    )?;
    Ok(n > 0)
}

pub fn set_pinned(conn: &rusqlite::Connection, user_id: Id, id: Id, pinned: bool) -> Result<()> {
    conn.execute(
        "UPDATE bookmarks SET pinned = ?, updated_at = ? WHERE id = ? AND user_id = ?",
        rusqlite::params![pinned as i64, bk_core::now(), id.inner(), user_id.inner()],
    )?;
    Ok(())
}

pub fn set_note(conn: &rusqlite::Connection, user_id: Id, id: Id, note: &str) -> Result<()> {
    conn.execute(
        "UPDATE bookmarks SET note = ?, updated_at = ? WHERE id = ? AND user_id = ?",
        rusqlite::params![note, bk_core::now(), id.inner(), user_id.inner()],
    )?;
    Ok(())
}

/// Update the crawl result for a bookmark (title/description/text/site/lang).
#[allow(clippy::too_many_arguments)]
pub fn apply_crawl(
    conn: &rusqlite::Connection,
    id: Id,
    title: &str,
    description: &str,
    text: &str,
    lang: Option<&str>,
    site: &str,
    quality: f32,
) -> Result<()> {
    let hash = bk_core::xxh3_str(text) as i64;
    conn.execute(
        "UPDATE bookmarks SET title = ?, description = ?, text = ?, text_hash = ?, lang = ?, site = ?, quality = ?, status = 'ok', crawled_at = ?, updated_at = ? WHERE id = ?",
        rusqlite::params![title, description, text, hash, lang, site, quality, bk_core::now(), bk_core::now(), id.inner()],
    )?;
    Ok(())
}

pub fn set_status(conn: &rusqlite::Connection, id: Id, status: &str) -> Result<()> {
    conn.execute(
        "UPDATE bookmarks SET status = ?, updated_at = ? WHERE id = ?",
        rusqlite::params![status, bk_core::now(), id.inner()],
    )?;
    Ok(())
}
/// Batch-fetch bookmarks by id for one user. Returns only rows that exist and
/// belong to the user.
pub fn get_many(conn: &rusqlite::Connection, user_id: Id, ids: &[Id]) -> Result<Vec<Bookmark>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let mut placeholders = String::with_capacity(ids.len() * 2);
    for (i, _) in ids.iter().enumerate() {
        if i > 0 {
            placeholders.push(',');
        }
        placeholders.push('?');
    }
    let sql = format!("SELECT {COLS} FROM bookmarks WHERE user_id = ? AND id IN ({placeholders})");
    let mut stmt = conn.prepare(&sql)?;
    let mut params: Vec<rusqlite::types::Value> = Vec::with_capacity(ids.len() + 1);
    params.push(rusqlite::types::Value::Integer(user_id.inner()));
    for id in ids {
        params.push(rusqlite::types::Value::Integer(id.inner()));
    }
    let rows = stmt.query_map(rusqlite::params_from_iter(params), row_to_bookmark)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|e| e.into())
}

/// Fetch a bookmark by id without a user filter. Internal jobs only.
pub fn get_raw(conn: &rusqlite::Connection, id: Id) -> Result<Option<Bookmark>> {
    let mut stmt = conn.prepare(&format!("SELECT {COLS} FROM bookmarks WHERE id = ?"))?;
    let row = stmt.query_row([id.inner()], row_to_bookmark);
    match row {
        Ok(b) => Ok(Some(b)),
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
        c.execute("INSERT INTO users(id, username, pw_hash, role, created_at, settings_json) VALUES (1,'a','x','user',0,'{}')", []).unwrap();
        c
    }

    #[test]
    fn insert_get_list_delete() {
        let c = db();
        let u = Id::from_i64(1);
        let id = insert(
            &c,
            u,
            "example.com/a",
            "https://example.com/a",
            "Hello",
            "body text",
        )
        .unwrap();
        let b = get(&c, u, id).unwrap().unwrap();
        assert_eq!(b.title, "Hello");
        assert_eq!(list(&c, u, 10, 0).unwrap().len(), 1);
        assert!(matches!(
            insert(&c, u, "example.com/a", "x", "y", "z"),
            Err(bk_core::Error::Conflict(_))
        ));
        assert!(delete(&c, u, id).unwrap());
        assert!(get(&c, u, id).unwrap().is_none());
    }

    #[test]
    fn isolation_between_users() {
        let c = db();
        c.execute("INSERT INTO users(id, username, pw_hash, role, created_at, settings_json) VALUES (2,'b','x','user',0,'{}')", []).unwrap();
        let a = Id::from_i64(1);
        let b = Id::from_i64(2);
        let id = insert(&c, a, "x.com", "https://x.com", "t", "").unwrap();
        assert!(get(&c, b, id).unwrap().is_none());
        assert!(list(&c, b, 10, 0).unwrap().is_empty());
        assert!(!delete(&c, b, id).unwrap());
    }
}

// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

//! Int8 embedding storage. One row per (bookmark, model_id). The vector is
//! stored as a raw int8 blob plus a per-row dequantization scale.

use bk_core::{Id, Result};

#[derive(Clone, Debug)]
pub struct VectorRow {
    pub bookmark_id: Id,
    pub dim: usize,
    pub scale: f32,
    pub data: Vec<i8>,
}

pub fn put(
    conn: &rusqlite::Connection,
    bookmark_id: Id,
    model_id: &str,
    dim: usize,
    scale: f32,
    data: &[i8],
) -> Result<()> {
    // SAFETY: i8 and u8 have identical layout; this is just a byte view.
    let blob: &[u8] = unsafe { std::slice::from_raw_parts(data.as_ptr() as *const u8, data.len()) };
    conn.execute(
        "INSERT INTO vectors(bookmark_id, model_id, dim, quant, scale, blob)
         VALUES (?, ?, ?, 'int8', ?, ?)
         ON CONFLICT(bookmark_id, model_id) DO UPDATE SET
           dim = excluded.dim, quant = excluded.quant,
           scale = excluded.scale, blob = excluded.blob",
        rusqlite::params![bookmark_id.inner(), model_id, dim as i64, scale, blob],
    )?;
    Ok(())
}

pub fn get(
    conn: &rusqlite::Connection,
    bookmark_id: Id,
    model_id: &str,
) -> Result<Option<VectorRow>> {
    let mut stmt = conn
        .prepare("SELECT dim, scale, blob FROM vectors WHERE bookmark_id = ? AND model_id = ?")?;
    let row = stmt.query_row(rusqlite::params![bookmark_id.inner(), model_id], |r| {
        let dim: i64 = r.get(0)?;
        let scale: f32 = r.get(1)?;
        let blob: Vec<u8> = r.get(2)?;
        Ok(VectorRow {
            bookmark_id,
            dim: dim as usize,
            scale,
            data: blob.iter().map(|&b| b as i8).collect(),
        })
    });
    match row {
        Ok(v) => Ok(Some(v)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Load every vector for a user + model. Used by the brute-force semantic arm.
pub fn all_for_user(
    conn: &rusqlite::Connection,
    user_id: Id,
    model_id: &str,
) -> Result<Vec<VectorRow>> {
    let mut stmt = conn.prepare(
        "SELECT v.bookmark_id, v.dim, v.scale, v.blob
         FROM vectors v JOIN bookmarks b ON b.id = v.bookmark_id
         WHERE b.user_id = ? AND v.model_id = ?",
    )?;
    let rows = stmt.query_map(rusqlite::params![user_id.inner(), model_id], |r| {
        let bookmark_id = Id::from_i64(r.get(0)?);
        let dim: i64 = r.get(1)?;
        let scale: f32 = r.get(2)?;
        let blob: Vec<u8> = r.get(3)?;
        Ok(VectorRow {
            bookmark_id,
            dim: dim as usize,
            scale,
            data: blob.iter().map(|&b| b as i8).collect(),
        })
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|e| e.into())
}

pub fn delete(conn: &rusqlite::Connection, bookmark_id: Id) -> Result<()> {
    conn.execute(
        "DELETE FROM vectors WHERE bookmark_id = ?",
        [bookmark_id.inner()],
    )?;
    Ok(())
}

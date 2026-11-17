// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

//! `bookmark backup`: a compressed, checksummed snapshot of the database.

use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use bk_core::{Error, Result};

/// Create a `.tar.zst` snapshot of `db_path`. Returns the archive path.
pub fn create(db_path: &Path, out: Option<&Path>) -> Result<PathBuf> {
    // Flush the WAL into the main database file first.
    if let Ok(conn) = rusqlite::Connection::open(db_path) {
        let _ = conn.pragma_update(None, "wal_checkpoint", "TRUNCATE");
    }

    let out_path = out.map(|p| p.to_path_buf()).unwrap_or_else(|| {
        let stamp = bk_core::now();
        db_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(format!("bookmark-backup-{stamp}.tar.zst"))
    });

    let sha = file_sha256(db_path)?;
    let manifest = serde_json::json!({
        "created_at": bk_core::now(),
        "database_sha256": sha,
        "database": "bookmark.db",
        "version": env!("CARGO_PKG_VERSION"),
    });

    let file = File::create(&out_path)?;
    let enc = zstd::Encoder::new(file, 9).map_err(|e| Error::Io(std::io::Error::other(e)))?;
    let mut tar = tar::Builder::new(enc);

    let mut db = File::open(db_path)?;
    let mut header = tar::Header::new_gnu();
    header.set_size(db.metadata()?.len());
    header.set_mode(0o600);
    header.set_cksum();
    tar.append_data(&mut header, "bookmark.db", &mut db)?;

    let mbytes =
        serde_json::to_vec_pretty(&manifest).map_err(|e| Error::Internal(e.to_string()))?;
    let mut mheader = tar::Header::new_gnu();
    mheader.set_size(mbytes.len() as u64);
    mheader.set_mode(0o600);
    mheader.set_cksum();
    tar.append_data(&mut mheader, "manifest.json", mbytes.as_slice())?;

    let enc = tar.into_inner()?;
    let mut file = enc
        .finish()
        .map_err(|e| Error::Io(std::io::Error::other(e)))?;
    file.flush()?;
    Ok(out_path)
}

/// Restore a backup archive into `db_path`. Refuses if the target exists.
pub fn restore(archive: &Path, db_path: &Path) -> Result<()> {
    if db_path.exists() {
        return Err(Error::Conflict(
            "a database already exists; move it aside first".into(),
        ));
    }
    let file = File::open(archive)?;
    let dec = zstd::Decoder::new(file).map_err(|e| Error::Io(std::io::Error::other(e)))?;
    let mut tar = tar::Archive::new(dec);
    let parent = db_path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)?;
    tar.unpack(parent)?;
    Ok(())
}

fn file_sha256(path: &Path) -> Result<String> {
    let mut f = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    use base64::Engine as _;
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("bookmark.db");
        {
            let c = rusqlite::Connection::open(&db).unwrap();
            c.execute_batch("CREATE TABLE t(x); INSERT INTO t VALUES (42);")
                .unwrap();
        }
        let archive = create(&db, None).unwrap();
        assert!(archive.exists());
        std::fs::remove_file(&db).unwrap();
        restore(&archive, &db).unwrap();
        let c = rusqlite::Connection::open(&db).unwrap();
        let v: i64 = c.query_row("SELECT x FROM t", [], |r| r.get(0)).unwrap();
        assert_eq!(v, 42);
    }
}

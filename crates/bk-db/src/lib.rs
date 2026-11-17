// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

//! SQLite access layer.
//!
//! Architecture:
//! - One **writer task** fed by an mpsc channel. All mutations go through it and
//!   are batched into transactions (flush on 25 ms or 512 jobs). This avoids
//!   SQLITE_BUSY storms from many concurrent htmx requests.
//! - Readers use a plain `rusqlite::Connection` each, wrapped in a small pool.
//! - WAL + `busy_timeout` + `synchronous=NORMAL` make reads never block on the
//!   writer for long.

use std::path::Path;
use std::sync::Arc;
use std::sync::Mutex;

use rusqlite::Connection;

use bk_core::Result;

pub mod bookmarks;
pub mod clicks;
pub mod crawls;
pub mod fts;
pub mod meta;
pub mod migrate;
pub mod pool;
pub mod screenshots;
pub mod sessions;
pub mod shares;
pub mod tags;
pub mod users;
pub mod vectors;
pub mod writer;

/// The database handle. Clone-cheap: the writer channel is shared.
#[derive(Clone)]
pub struct Db {
    inner: Arc<DbInner>,
}

struct DbInner {
    path: Arc<std::path::PathBuf>,
    writer: writer::Writer,
    pool: pool::ReadPool,
}

impl Db {
    /// Open the database, applying PRAGMAs and migrations. Idempotent.
    pub fn open(path: &Path) -> Result<Self> {
        let path = path.to_path_buf();
        let mut conn = Self::open_conn(&path)?;
        migrate::apply(&mut conn)?;
        let writer = writer::Writer::new(conn)?;
        Ok(Self {
            inner: Arc::new(DbInner {
                pool: pool::ReadPool::new(path.clone()),
                path: Arc::new(path),
                writer,
            }),
        })
    }

    fn open_conn(path: &Path) -> Result<Connection> {
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA synchronous=NORMAL;
             PRAGMA busy_timeout=5000;
             PRAGMA foreign_keys=ON;
             PRAGMA mmap_size=268435456;
             PRAGMA cache_size=-32000;",
        )?;
        Ok(conn)
    }

    /// Run a read-only query on a pooled connection.
    pub fn with_conn<R: Send + 'static>(
        &self,
        f: impl FnOnce(&Connection) -> Result<R> + Send + 'static,
    ) -> Result<R> {
        self.inner.pool.with(f)
    }

    /// Run a blocking read synchronously (for CLI use and the query path).
    pub fn with_conn_sync<R>(&self, f: impl FnOnce(&Connection) -> Result<R>) -> Result<R> {
        self.inner.pool.with(f)
    }

    /// Submit a write job. The job runs inside a single transaction, batched
    /// with other jobs. The returned future resolves when the job has been
    /// applied (or its error has propagated).
    pub async fn exec<F>(&self, f: F) -> Result<()>
    where
        F: FnOnce(&rusqlite::Transaction<'_>) -> Result<()> + Send + 'static,
    {
        self.inner.writer.exec(f).await
    }

    /// Run a batch of jobs in one transaction.
    pub async fn exec_batch<F>(&self, jobs: Vec<F>) -> Result<()>
    where
        F: FnOnce(&rusqlite::Transaction<'_>) -> Result<()> + Send + 'static,
    {
        self.inner.writer.exec_batch(jobs).await
    }

    /// Number of registered users.
    pub fn user_count(&self) -> i64 {
        self.with_conn_sync(|c| {
            let n: i64 = c.query_row("SELECT COUNT(*) FROM users", [], |r| r.get(0))?;
            Ok(n)
        })
        .unwrap_or(0)
    }

    /// Access the writer handle directly (for bulk index rebuilds).
    pub fn writer(&self) -> &writer::Writer {
        &self.inner.writer
    }

    /// Path to the database file.
    pub fn path(&self) -> &Path {
        &self.inner.path
    }
}

/// A read pool is just a Mutex of connections for the common case where the
/// server opens one connection per request handler. `Db` itself is cheap to
/// clone.
pub type Pool = Arc<Mutex<Connection>>;

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn open_and_migrate() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let db = Db::open(&path).unwrap();
        assert_eq!(db.user_count(), 0);
        // Idempotent.
        let db2 = Db::open(&path).unwrap();
        assert_eq!(db2.user_count(), 0);
    }
}

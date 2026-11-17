// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

//! Marker module: the database connection pool for reader paths.

use std::sync::Arc;
use std::sync::Mutex;

use rusqlite::Connection;

use bk_core::Result;

/// A tiny pool of read connections. Each call checks out one connection, uses
/// it, and returns it. Safe to share across threads (`Connection: Send`).
#[derive(Clone)]
pub struct ReadPool {
    path: Arc<std::path::PathBuf>,
    idle: Arc<Mutex<Vec<Connection>>>,
}

impl ReadPool {
    pub fn new(path: impl Into<std::path::PathBuf>) -> Self {
        Self {
            path: Arc::new(path.into()),
            idle: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn with<R>(&self, f: impl FnOnce(&Connection) -> Result<R>) -> Result<R> {
        let conn = {
            let mut idle = self
                .idle
                .lock()
                .map_err(|_| bk_core::Error::Internal("pool poisoned".into()))?;
            idle.pop()
        };
        let conn = match conn {
            Some(c) => c,
            None => {
                let c = Connection::open(self.path.as_path())?;
                c.execute_batch(
                    "PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA busy_timeout=5000;
                     PRAGMA foreign_keys=ON; PRAGMA mmap_size=268435456; PRAGMA cache_size=-32000;",
                )?;
                c
            }
        };
        let result = f(&conn);
        if let Ok(mut idle) = self.idle.lock()
            && idle.len() < 8
        {
            idle.push(conn);
        }
        result
    }
}

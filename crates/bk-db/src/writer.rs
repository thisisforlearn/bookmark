// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

//! Single writer task.
//!
//! All mutations funnel through one channel into one task that batches jobs
//! into transactions. Batching is bounded by **25 ms** or **512 jobs**, so
//! interactive adds stay snappy while bulk crawls don't thrash the journal.

use std::time::Duration;

use rusqlite::Connection;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tokio::time;

use bk_core::Result;

/// A write job. The closure runs inside a single transaction; if it returns
/// `Err`, that job's error is sent back to its caller (the rest of the batch
/// still commits).
pub type WriteJob = Box<dyn FnOnce(&rusqlite::Transaction<'_>) -> Result<()> + Send + 'static>;

const FLUSH_MS: u64 = 25;
const FLUSH_JOBS: usize = 512;

/// Handle used to submit jobs. Cheap to clone (channel only).
#[derive(Clone)]
pub struct Writer {
    tx: mpsc::UnboundedSender<Job>,
}

struct Job {
    f: WriteJob,
    done: oneshot::Sender<Result<()>>,
}

impl Writer {
    pub fn new(conn: Connection) -> Result<Self> {
        let (tx, rx) = mpsc::unbounded_channel();
        let rt = tokio::runtime::Handle::current();
        rt.spawn(async move { run(rx, conn).await });
        Ok(Self { tx })
    }

    /// Submit a single job and await its result.
    pub async fn exec<F>(&self, f: F) -> Result<()>
    where
        F: FnOnce(&rusqlite::Transaction<'_>) -> Result<()> + Send + 'static,
    {
        let (done, rx) = oneshot::channel();
        let _ = self.tx.send(Job {
            f: Box::new(f),
            done,
        });
        rx.await
            .map_err(|_| bk_core::Error::Internal("writer task died".into()))?
    }

    /// Submit a batch of jobs in one transaction.
    pub async fn exec_batch<F>(&self, jobs: Vec<F>) -> Result<()>
    where
        F: FnOnce(&rusqlite::Transaction<'_>) -> Result<()> + Send + 'static,
    {
        let mut handles = Vec::with_capacity(jobs.len());
        let mut batch = Vec::with_capacity(jobs.len());
        for f in jobs {
            let (done, rx) = oneshot::channel();
            handles.push(rx);
            batch.push(Job {
                f: Box::new(f),
                done,
            });
        }
        for j in batch {
            let _ = self.tx.send(j);
        }
        for r in handles {
            // A failed job returns its error; a dropped job means the writer
            // task is gone.
            match r.await {
                Ok(Ok(())) => {}
                Ok(Err(e)) => return Err(e),
                Err(_) => return Err(bk_core::Error::Internal("writer task died".into())),
            }
        }
        Ok(())
    }
}

async fn run(mut rx: mpsc::UnboundedReceiver<Job>, conn: Connection) {
    let (sync_tx, sync_rx) = std::sync::mpsc::channel::<Job>();
    let conn = std::sync::Mutex::new(conn);
    let handle = std::thread::spawn(move || {
        while let Ok(job) = sync_rx.recv() {
            let mut c = conn.lock().unwrap();
            let tx = match c.transaction() {
                Ok(tx) => tx,
                Err(e) => {
                    tracing::error!("writer: transaction failed: {e}");
                    let _ = job.done.send(Err(bk_core::Error::Db(e.to_string())));
                    continue;
                }
            };
            match (job.f)(&tx) {
                Ok(()) => match tx.commit() {
                    Ok(()) => {
                        let _ = job.done.send(Ok(()));
                    }
                    Err(e) => {
                        tracing::error!("writer: commit failed: {e}");
                        let _ = job.done.send(Err(bk_core::Error::Db(e.to_string())));
                    }
                },
                Err(e) => {
                    tracing::warn!("writer: job failed: {e}");
                    let _ = tx.rollback();
                    let _ = job.done.send(Err(e));
                }
            }
        }
    });

    loop {
        let first = match rx.recv().await {
            Some(j) => j,
            None => break,
        };
        let mut batch: Vec<Job> = vec![first];
        while let Ok(j) = rx.try_recv() {
            batch.push(j);
            if batch.len() >= FLUSH_JOBS {
                break;
            }
        }
        let deadline = time::Instant::now() + Duration::from_millis(FLUSH_MS);
        loop {
            if batch.len() >= FLUSH_JOBS {
                break;
            }
            let now = time::Instant::now();
            if now >= deadline {
                break;
            }
            match time::timeout(deadline - now, rx.recv()).await {
                Ok(Some(j)) => batch.push(j),
                Ok(None) => break,
                Err(_) => break,
            }
            if batch.len() >= FLUSH_JOBS {
                break;
            }
        }
        for j in batch.drain(..) {
            let _ = sync_tx.send(j);
        }
    }
    drop(sync_tx);
    let _ = handle.join();
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    #[tokio::test]
    async fn serial_writes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("w.db");
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch("PRAGMA journal_mode=WAL; CREATE TABLE t(n INTEGER NOT NULL);")
            .unwrap();
        let w = Writer::new(conn).unwrap();
        let jobs: Vec<_> = (0..100)
            .map(|i| {
                move |tx: &rusqlite::Transaction<'_>| {
                    tx.execute("INSERT INTO t(n) VALUES (?)", [i])?;
                    Ok(())
                }
            })
            .collect();
        w.exec_batch(jobs).await.unwrap();
        let check = Connection::open(&path).unwrap();
        let n: i64 = check
            .query_row("SELECT COUNT(*) FROM t", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 100);
    }
}

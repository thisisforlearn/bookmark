// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

//! Forward-only migrations. The schema lives in `migrations/000N_*.sql` and is
//! embedded at compile time. `apply()` is idempotent: it bumps the SQLite
//! `user_version` to the latest migration number and only runs new files.

use rusqlite::Connection;

use bk_core::Result;

const CURRENT_VERSION: i64 = 1;

/// Apply any pending migrations. Idempotent.
pub fn apply(conn: &mut Connection) -> Result<()> {
    let current: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if current >= CURRENT_VERSION {
        return Ok(());
    }
    let tx = conn.transaction()?;
    tx.execute_batch(include_str!("../../../migrations/0001_init.sql"))?;
    tx.pragma_update(None, "user_version", CURRENT_VERSION)?;
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    #[test]
    fn idempotent() {
        let mut conn = Connection::open_in_memory().unwrap();
        apply(&mut conn).unwrap();
        apply(&mut conn).unwrap();
        let v: i64 = conn
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap();
        assert_eq!(v, CURRENT_VERSION);
    }
}

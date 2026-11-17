// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

//! Forward-only migrations. Each entry is `(version, sql)`; `apply` runs any
//! migration newer than the database's `user_version`, in order.

use rusqlite::Connection;

use bk_core::Result;

const MIGRATIONS: &[(i64, &str)] = &[
    (1, include_str!("../../../migrations/0001_init.sql")),
    (2, include_str!("../../../migrations/0002_quality.sql")),
    (3, include_str!("../../../migrations/0003_devices.sql")),
];

pub fn current_version() -> i64 {
    MIGRATIONS.last().map(|(v, _)| *v).unwrap_or(0)
}

/// Apply any pending migrations. Idempotent.
pub fn apply(conn: &mut Connection) -> Result<()> {
    let mut version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    for (v, sql) in MIGRATIONS {
        if *v <= version {
            continue;
        }
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", *v)?;
        tx.commit()?;
        version = *v;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idempotent_and_ordered() {
        let mut conn = Connection::open_in_memory().unwrap();
        apply(&mut conn).unwrap();
        apply(&mut conn).unwrap();
        let v: i64 = conn
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap();
        assert_eq!(v, current_version());
        let _: f64 = conn
            .query_row("SELECT quality FROM bookmarks LIMIT 1", [], |r| r.get(0))
            .unwrap_or(1.0);
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM devices", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0);
    }
}

// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

//! Wall-clock helpers. All timestamps are unix seconds (i64) so they fit in
//! SQLite INTEGER columns and sort naturally.

use std::time::{SystemTime, UNIX_EPOCH};

/// Current unix timestamp in seconds.
#[inline]
pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Current unix timestamp in milliseconds.
#[inline]
pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn monotonic_and_reasonable() {
        let a = now();
        let b = now();
        assert!(a <= b);
        assert!(a > 1_700_000_000);
    }
}

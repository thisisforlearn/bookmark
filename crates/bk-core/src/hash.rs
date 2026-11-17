// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

//! xxHash-3 hashing. Used for fast, non-cryptographic content dedupe and for
//! the `text_hash` column. xxh3 has a 64-bit output and is collision-resistant
//! enough for our dedupe use case.

use xxhash_rust::xxh3::xxh3_64;

/// xxh3-64 of raw bytes.
#[inline]
pub fn xxh3(bytes: &[u8]) -> u64 {
    xxh3_64(bytes)
}

/// xxh3-64 of a UTF-8 string.
#[inline]
pub fn xxh3_str(s: &str) -> u64 {
    xxh3_64(s.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable() {
        assert_eq!(xxh3(b"hello"), xxh3(b"hello"));
        assert_ne!(xxh3(b"hello"), xxh3(b"world"));
        assert_eq!(xxh3_str("foo"), xxh3(b"foo"));
    }
}

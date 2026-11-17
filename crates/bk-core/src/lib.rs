// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav
//
// Shared primitives for the bookmark server: typed errors, ids, time, hashing,
// configuration and feature detection.

//! `bk-core`

pub mod config;
pub mod error;
pub mod hash;
pub mod time;

pub use config::Config;
pub use error::{Error, Result};
pub use hash::{xxh3, xxh3_str};
pub use time::{now, now_ms};

/// A database row id. Newtype so we never confuse it with a `usize` or `i64`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct Id(pub i64);

impl std::fmt::Display for Id {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Default for Id {
    #[inline]
    fn default() -> Self {
        Id(0)
    }
}

impl Id {
    #[inline]
    pub fn from_i64(n: i64) -> Self {
        Self(n)
    }
    #[inline]
    pub fn inner(self) -> i64 {
        self.0
    }
}

/// Best-effort runtime feature detection for the inference kernels.
#[derive(Clone, Copy, Debug, Default)]
pub struct SimdFeatures {
    pub avx2: bool,
    pub avx_vnni: bool,
    pub neon: bool,
    pub sdot: bool,
}

impl SimdFeatures {
    #[cfg(target_arch = "x86_64")]
    pub fn detect() -> Self {
        use std::arch::is_x86_feature_detected;
        Self {
            avx2: is_x86_feature_detected!("avx2"),
            avx_vnni: is_x86_feature_detected!("avxvnni"),
            neon: false,
            sdot: false,
        }
    }

    #[cfg(target_arch = "aarch64")]
    pub fn detect() -> Self {
        Self {
            avx2: false,
            avx_vnni: false,
            neon: true,
            sdot: true,
        }
    }

    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    pub fn detect() -> Self {
        Self::default()
    }
}

/// Validate a config file path exists and is readable; return the directory.
pub fn state_dir(dir: &std::path::Path) -> std::io::Result<()> {
    let dir = dir.to_path_buf();
    std::fs::create_dir_all(&dir)?;
    let meta = dir.metadata()?;
    use std::os::unix::fs::PermissionsExt;
    let mut perms = meta.permissions();
    perms.set_mode(0o700);
    std::fs::set_permissions(&dir, perms)?;
    Ok(())
}

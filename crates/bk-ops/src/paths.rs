// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

//! Path helpers. Delegates to `bk_core` so there is one source of truth for
//! per-platform defaults.

use std::path::PathBuf;

pub fn config_path() -> PathBuf {
    if let Ok(p) = std::env::var("BOOKMARK_CONFIG") {
        return PathBuf::from(p);
    }
    bk_core::config::default_config_path()
}

pub fn data_dir() -> PathBuf {
    bk_core::config::default_data_dir()
}

pub fn models_dir() -> PathBuf {
    data_dir().join("models")
}

/// True when running under Termux/Android.
pub fn is_termux() -> bool {
    bk_core::config::is_termux()
}

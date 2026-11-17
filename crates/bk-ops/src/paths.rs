// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

//! Path helpers that mirror `bk_core::Config` defaults.

use std::path::PathBuf;

pub fn config_path() -> PathBuf {
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join(".config/bookmark/config.toml");
    }
    PathBuf::from("/etc/bookmark/config.toml")
}

pub fn data_dir() -> PathBuf {
    if let Ok(home) = std::env::var("HOME") {
        let p = PathBuf::from(&home);
        if p.starts_with("/data/data") {
            return p.join(".bookmark");
        }
        return p.join(".local/share/bookmark");
    }
    PathBuf::from("/var/lib/bookmark")
}

pub fn models_dir() -> PathBuf {
    data_dir().join("models")
}

/// True when running under Termux/Android.
pub fn is_termux() -> bool {
    std::env::var("PREFIX")
        .map(|p| p.contains("com.termux"))
        .unwrap_or(false)
}

// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

//! Runtime configuration. A TOML file (default `~/.config/bookmark/config.toml`),
//! overridable per-invocation via CLI flags. Everything has a sane default so
//! the server works out of the box after `bookmark init`.

use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Clone, Debug, Deserialize, serde::Serialize)]
#[serde(default)]
pub struct Config {
    /// Directory holding the SQLite DB and runtime state.
    pub data_dir: PathBuf,
    /// Directory holding the `.bkmdl` model files.
    pub models_dir: PathBuf,
    /// Host the HTTP server binds to.
    pub host: String,
    /// Port the HTTP server binds to.
    pub port: u16,
    /// Optional Tailscale-only mode: reject connections not on the tailnet.
    pub require_tailscale: bool,
    /// Allow public self-registration (invite-only by default).
    pub open_registration: bool,
    /// Initial admin username created by `bookmark init` if no users exist.
    pub admin: String,
    /// Per-user request rate limit (requests per second).
    pub rate_limit_rps: u32,
    /// Global concurrency cap on the crawl semaphore.
    pub crawl_concurrency: usize,
    /// Polite per-host delay in milliseconds between crawl requests.
    pub crawl_polite_ms: u64,
    /// Maximum text bytes stored per bookmark after crawl extraction.
    pub max_text_bytes: usize,
    /// Maximum screenshot bytes accepted on upload.
    pub max_screenshot_bytes: usize,
    /// Whether the semantic/vector arms of search are enabled.
    pub search_semantic: bool,
    /// Whether the LTR re-ranker is enabled.
    pub search_rerank: bool,
    /// Whether the spell corrector is enabled.
    pub search_spell: bool,
}

impl Default for Config {
    fn default() -> Self {
        let data_dir = default_data_dir();
        Self {
            models_dir: data_dir.join("models"),
            host: "127.0.0.1".to_string(),
            port: 8080,
            require_tailscale: false,
            open_registration: false,
            admin: "admin".to_string(),
            rate_limit_rps: 30,
            crawl_concurrency: 4,
            crawl_polite_ms: 1500,
            max_text_bytes: 400 * 1024,
            max_screenshot_bytes: 5 * 1024 * 1024,
            search_semantic: true,
            search_rerank: true,
            search_spell: true,
            data_dir,
        }
    }
}

impl Config {
    /// Load from a TOML file, falling back to defaults for any missing keys.
    pub fn load(path: &Path) -> crate::Result<Self> {
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(e.into()),
        };
        let mut cfg: Config =
            toml::from_str(&text).map_err(|e| crate::Error::Internal(e.to_string()))?;
        if cfg.data_dir.as_os_str().is_empty() {
            cfg.data_dir = default_data_dir();
        }
        if cfg.models_dir.as_os_str().is_empty() {
            cfg.models_dir = cfg.data_dir.join("models");
        }
        Ok(cfg)
    }

    /// Path to the SQLite database.
    pub fn db_path(&self) -> PathBuf {
        self.data_dir.join("bookmark.db")
    }

    /// Path to the runtime log ring buffer.
    pub fn log_path(&self) -> PathBuf {
        self.data_dir.join("bookmark.log")
    }

    /// Persist this config back to `path` (used by `bookmark init`).
    pub fn save(&self, path: &Path) -> crate::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = toml::to_string(self).map_err(|e| crate::Error::Internal(e.to_string()))?;
        std::fs::write(path, text)?;
        Ok(())
    }
}

/// Default data directory, per platform.
///
/// - Windows: `%LOCALAPPDATA%\\bookmark`
/// - Termux: `$HOME/.bookmark`
/// - Linux/macOS: `$HOME/.local/share/bookmark`
pub fn default_data_dir() -> PathBuf {
    #[cfg(windows)]
    {
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            return PathBuf::from(local).join("bookmark");
        }
        if let Ok(profile) = std::env::var("USERPROFILE") {
            return PathBuf::from(profile).join(".bookmark");
        }
        return PathBuf::from("bookmark-data");
    }
    #[cfg(not(windows))]
    {
        if let Ok(home) = std::env::var("HOME") {
            let p = PathBuf::from(&home);
            // Termux/Android
            if p.starts_with("/data/data") {
                return p.join(".bookmark");
            }
            return p.join(".local/share/bookmark");
        }
        PathBuf::from("/var/lib/bookmark")
    }
}

/// Default config file location, per platform.
pub fn default_config_path() -> PathBuf {
    #[cfg(windows)]
    {
        if let Ok(appdata) = std::env::var("APPDATA") {
            return PathBuf::from(appdata).join("bookmark").join("config.toml");
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join(".config/bookmark/config.toml");
    }
    PathBuf::from("/etc/bookmark/config.toml")
}

/// True when running under Termux/Android.
pub fn is_termux() -> bool {
    std::env::var("PREFIX")
        .map(|p| p.contains("com.termux"))
        .unwrap_or(false)
}

/// True when running on Android (Termux or a native build).
pub fn is_android() -> bool {
    cfg!(target_os = "android") || is_termux()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_round_trip() {
        let cfg = Config::default();
        assert_eq!(cfg.host, "127.0.0.1");
        assert_eq!(cfg.port, 8080);
        assert!(cfg.max_text_bytes > 0);
        let text = toml::to_string(&cfg).unwrap();
        let cfg2: Config = toml::from_str(&text).unwrap();
        assert_eq!(cfg.host, cfg2.host);
        assert_eq!(cfg.port, cfg2.port);
    }
}

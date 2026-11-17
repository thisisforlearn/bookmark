// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

//! `bookmark doctor`: a plain-language health check.

use std::path::Path;

use bk_core::{Config, SimdFeatures};

#[derive(Clone, Debug)]
pub struct Check {
    pub name: &'static str,
    pub ok: bool,
    pub detail: String,
}

#[derive(Clone, Debug, Default)]
pub struct DoctorReport {
    pub checks: Vec<Check>,
}

impl DoctorReport {
    pub fn all_ok(&self) -> bool {
        self.checks.iter().all(|c| c.ok)
    }

    pub fn run(cfg: &Config) -> Self {
        let mut checks = Vec::new();

        // Data directory.
        let data_ok = cfg.data_dir.exists() && cfg.data_dir.is_dir();
        checks.push(Check {
            name: "data directory",
            ok: data_ok,
            detail: format!("{}", cfg.data_dir.display()),
        });

        // Database.
        let db_path = cfg.db_path();
        let (db_ok, db_detail) = match rusqlite::Connection::open(&db_path) {
            Ok(conn) => {
                let n: i64 = conn
                    .query_row(
                        "SELECT COUNT(*) FROM sqlite_master WHERE type='table'",
                        [],
                        |r| r.get(0),
                    )
                    .unwrap_or(0);
                (true, format!("{} tables at {}", n, db_path.display()))
            }
            Err(e) => (false, format!("cannot open {}: {e}", db_path.display())),
        };
        checks.push(Check {
            name: "database",
            ok: db_ok,
            detail: db_detail,
        });

        // FTS5.
        let fts_ok = rusqlite::Connection::open_in_memory()
            .and_then(|c| c.execute_batch("CREATE VIRTUAL TABLE t USING fts5(x);"))
            .is_ok();
        checks.push(Check {
            name: "FTS5 full-text search",
            ok: fts_ok,
            detail: if fts_ok {
                "available".into()
            } else {
                "missing — rebuild rusqlite with FTS5".into()
            },
        });

        // Models.
        let models = [
            ("spell", cfg.models_dir.join("spell.bkmdl")),
            ("embed", cfg.models_dir.join("embed.bkmdl")),
            ("rank", cfg.models_dir.join("rank.bkmdl")),
        ];
        for (name, path) in models {
            let present = path.exists();
            let ok = if present {
                bk_model::ModelFile::open(&path).is_ok()
            } else {
                true // optional; lexical search still works
            };
            checks.push(Check {
                name: match name {
                    "spell" => "model: spell",
                    "embed" => "model: embed",
                    _ => "model: rank",
                },
                ok,
                detail: if present {
                    format!("{}", path.display())
                } else {
                    "not installed (search is lexical-only)".to_string()
                },
            });
        }

        // Port availability.
        let addr = format!("{}:{}", cfg.host, cfg.port);
        let port_free = std::net::TcpListener::bind(&addr).is_ok();
        checks.push(Check {
            name: "listen address",
            ok: port_free,
            detail: if port_free {
                format!("{addr} is available")
            } else {
                format!("{addr} is already in use")
            },
        });

        // Disk space on the data dir.
        let disk = disk_free(&cfg.data_dir);
        checks.push(Check {
            name: "disk space",
            ok: disk.map(|d| d > 64 * 1024 * 1024).unwrap_or(true),
            detail: disk
                .map(|d| format!("{} MiB free", d / (1024 * 1024)))
                .unwrap_or_else(|| "unknown".into()),
        });

        // SIMD.
        let f = SimdFeatures::detect();
        let simd = if f.avx2 || f.neon {
            "enabled"
        } else {
            "scalar fallback"
        };
        checks.push(Check {
            name: "SIMD kernels",
            ok: true,
            detail: format!(
                "{simd} (avx2={} avxvnni={} neon={})",
                f.avx2, f.avx_vnni, f.neon
            ),
        });

        checks.push(Check {
            name: "version",
            ok: true,
            detail: format!("bookmark {}", env!("CARGO_PKG_VERSION")),
        });

        Self { checks }
    }
}

#[cfg(unix)]
fn disk_free(path: &Path) -> Option<u64> {
    use std::ffi::CString;
    let c = CString::new(path.to_string_lossy().as_bytes()).ok()?;
    // SAFETY: `statvfs` writes into a zeroed, correctly-sized struct; the CString
    // outlives the call.
    unsafe {
        let mut st: libc::statvfs = std::mem::zeroed();
        if libc::statvfs(c.as_ptr(), &mut st) != 0 {
            return None;
        }
        Some(st.f_bavail as u64 * st.f_frsize as u64)
    }
}

#[cfg(not(unix))]
fn disk_free(_path: &Path) -> Option<u64> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runs_without_panicking() {
        let cfg = Config::default();
        let r = DoctorReport::run(&cfg);
        assert!(r.checks.iter().any(|c| c.name == "FTS5 full-text search"));
    }
}

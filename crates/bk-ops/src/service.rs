// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

//! Service integration: systemd user unit, Termux boot script, Windows note.

use std::path::{Path, PathBuf};

use bk_core::Result;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ServiceKind {
    Systemd,
    Termux,
    Windows,
    Unknown,
}

pub fn detect() -> ServiceKind {
    if cfg!(target_os = "windows") {
        return ServiceKind::Windows;
    }
    if std::env::var("PREFIX")
        .map(|p| p.contains("com.termux"))
        .unwrap_or(false)
    {
        return ServiceKind::Termux;
    }
    if Path::new("/run/systemd/system").exists() {
        return ServiceKind::Systemd;
    }
    ServiceKind::Unknown
}

/// The systemd user unit contents.
pub fn systemd_unit(exe: &Path, config: &Path) -> String {
    format!(
        "[Unit]\n\
         Description=bookmark server\n\
         After=network-online.target\n\
         \n\
         [Service]\n\
         Type=simple\n\
         ExecStart={exe} watch\n\
         Environment=BOOKMARK_CONFIG={cfg}\n\
         Restart=always\n\
         RestartSec=2\n\
         LimitNOFILE=65536\n\
         \n\
         [Install]\n\
         WantedBy=default.target\n",
        exe = exe.display(),
        cfg = config.display(),
    )
}

/// The Termux boot hook contents.
pub fn termux_boot_script(exe: &Path) -> String {
    format!(
        "#!/data/data/com.termux/files/usr/bin/sh\n\
         # SPDX-License-Identifier: GPL-3.0-only\n\
         termux-wake-lock 2>/dev/null || true\n\
         exec {exe} watch >> $HOME/.bookmark/bookmark.log 2>&1\n",
        exe = exe.display(),
    )
}

/// Write the unit/boot file for the current platform. Returns the path written.
pub fn install(exe: &Path, config: &Path) -> Result<PathBuf> {
    match detect() {
        ServiceKind::Systemd => {
            let dir = home().join(".config/systemd/user");
            std::fs::create_dir_all(&dir)?;
            let path = dir.join("bookmark.service");
            std::fs::write(&path, systemd_unit(exe, config))?;
            Ok(path)
        }
        ServiceKind::Termux => {
            let dir = home().join(".termux/boot");
            std::fs::create_dir_all(&dir)?;
            let path = dir.join("start-bookmark.sh");
            std::fs::write(&path, termux_boot_script(exe))?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mut p = std::fs::metadata(&path)?.permissions();
                p.set_mode(0o700);
                std::fs::set_permissions(&path, p)?;
            }
            Ok(path)
        }
        ServiceKind::Windows => Err(bk_core::Error::Internal(
            "On Windows, use Task Scheduler to run `bookmark watch` at logon.".into(),
        )),
        ServiceKind::Unknown => Err(bk_core::Error::Internal(
            "no supported service manager found".into(),
        )),
    }
}

fn home() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_contains_exec() {
        let u = systemd_unit(Path::new("/usr/bin/bookmark"), Path::new("/etc/x.toml"));
        assert!(u.contains("ExecStart=/usr/bin/bookmark watch"));
        assert!(u.contains("Restart=always"));
    }
}

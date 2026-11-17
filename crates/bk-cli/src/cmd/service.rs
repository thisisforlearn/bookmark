// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

//! `bookmark service` — install/start/stop a background service.

use crate::cli::ServiceCommand;

use crate::cmd::common;
use crate::init_tracing;

pub async fn run(command: ServiceCommand) -> i32 {
    init_tracing();
    let cfg_path = common::config_path();
    match command {
        ServiceCommand::Install => {
            let exe = match std::env::current_exe() {
                Ok(e) => e,
                Err(e) => {
                    eprintln!("error: cannot locate executable: {e}");
                    return 1;
                }
            };
            match bk_ops::service::install(&exe, &cfg_path) {
                Ok(path) => {
                    println!("installed service file: {}", path.display());
                    match bk_ops::service::detect() {
                        bk_ops::ServiceKind::Systemd => {
                            println!();
                            println!("Enable it with:");
                            println!("  systemctl --user daemon-reload");
                            println!("  systemctl --user enable --now bookmark");
                            println!("  loginctl enable-linger $USER   # start on boot");
                        }
                        bk_ops::ServiceKind::Termux => {
                            println!();
                            println!("The boot script runs on Termux:Boot. Install that app once,");
                            println!("open it a single time, then reboot.");
                        }
                        _ => {}
                    }
                    0
                }
                Err(e) => {
                    eprintln!("error: {e}");
                    1
                }
            }
        }
        ServiceCommand::Start
        | ServiceCommand::Stop
        | ServiceCommand::Status
        | ServiceCommand::Logs => {
            let kind = bk_ops::service::detect();
            match kind {
                bk_ops::ServiceKind::Systemd => {
                    let verb = match command {
                        ServiceCommand::Start => "start",
                        ServiceCommand::Stop => "stop",
                        ServiceCommand::Status => "status",
                        _ => "status",
                    };
                    println!("run: systemctl --user {verb} bookmark");
                    println!("logs: journalctl --user -u bookmark -f");
                }
                bk_ops::ServiceKind::Termux => {
                    println!("Termux has no service manager. Use:");
                    println!("  bookmark watch &        # foreground supervisor");
                    println!("  or install Termux:Boot and run `bookmark service install`");
                }
                _ => println!("no supported service manager detected"),
            }
            0
        }
    }
}

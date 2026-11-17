// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

//! `bookmark model` — inspect, verify and (optionally) train models.

use crate::cli::ModelCommand;

use crate::cmd::common;
use crate::init_tracing;

pub async fn run(command: ModelCommand) -> i32 {
    init_tracing();
    let cfg = common::load_config();
    match command {
        ModelCommand::List => {
            println!("models directory: {}", cfg.models_dir.display());
            let entries = std::fs::read_dir(&cfg.models_dir).ok();
            let mut found = false;
            if let Some(entries) = entries {
                for e in entries.flatten() {
                    let p = e.path();
                    if p.extension().and_then(|s| s.to_str()) == Some("bkmdl") {
                        let size = e.metadata().map(|m| m.len()).unwrap_or(0);
                        println!(
                            "  {}  {}",
                            p.file_name().unwrap_or_default().to_string_lossy(),
                            common::human_bytes(size)
                        );
                        found = true;
                    }
                }
            }
            if !found {
                println!("  (no .bkmdl files installed)");
                println!();
                println!("  Search still works without models (lexical + SymSpell).");
                println!(
                    "  To add semantic search: build/train models with `bookmark model train`,"
                );
                println!(
                    "  then copy `spell.bkmdl`, `embed.bkmdl`, `rank.bkmdl` into the directory above."
                );
            }
            0
        }
        ModelCommand::Verify => {
            let mut ok = true;
            for name in ["spell", "embed", "rank"] {
                let path = cfg.models_dir.join(format!("{name}.bkmdl"));
                if !path.exists() {
                    println!("{name:<6} not installed");
                    continue;
                }
                match bk_model::ModelFile::open(&path) {
                    Ok(mf) => {
                        let secs = mf.header().tensor_count;
                        let vocab = mf.vocab().len();
                        println!(
                            "{name:<6} ok  {secs} tensors, {vocab} vocab, kind {}",
                            mf.kind().as_str()
                        );
                    }
                    Err(e) => {
                        println!("{name:<6} INVALID: {e}");
                        ok = false;
                    }
                }
            }
            if ok { 0 } else { 1 }
        }
        ModelCommand::Train {
            model,
            backend,
            mode,
        } => train(&model, &backend, &mode),
        ModelCommand::Export { out } => {
            println!("Models are exported as `.bkmdl` automatically by `bookmark model train`.");
            println!("Target path requested: {out}");
            println!(
                "Copy `models/*.bkmdl` to {} after training.",
                cfg.models_dir.display()
            );
            0
        }
    }
}

#[cfg(feature = "train")]
fn train(model: &str, backend: &str, mode: &str) -> i32 {
    bk_train::cli::train(model, backend, mode)
}

#[cfg(not(feature = "train"))]
fn train(model: &str, backend: &str, mode: &str) -> i32 {
    let _ = (model, backend, mode);
    eprintln!("This build has no training support.");
    eprintln!();
    eprintln!("Training requires the Burn backend, which is only compiled on the Windows");
    eprintln!("training binary. To enable it, build with:");
    eprintln!();
    eprintln!("  cargo build -p bk-cli --features train");
    eprintln!();
    eprintln!("Then run:");
    eprintln!("  bookmark model train spell --backend wgpu --mode full");
    2
}

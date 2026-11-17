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
        ModelCommand::Install { from } => install_models(&cfg, from.as_deref()).await,
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

fn train(model: &str, backend: &str, mode: &str) -> i32 {
    bk_train::cli::train(model, backend, mode)
}

const MODELS: [&str; 3] = ["spell.bkmdl", "embed.bkmdl", "rank.bkmdl"];

/// Download (or copy) the prebuilt default models. Never fails hard: each file
/// that can't be fetched is reported and skipped.
async fn install_models(cfg: &bk_core::Config, from: Option<&str>) -> i32 {
    if let Err(e) = std::fs::create_dir_all(&cfg.models_dir) {
        eprintln!("error: cannot create {}: {e}", cfg.models_dir.display());
        return 1;
    }

    // Local directory?
    if let Some(src) = from {
        let p = std::path::Path::new(src);
        if p.is_dir() {
            let mut ok = 0;
            for name in MODELS {
                let file = p.join(name);
                if file.exists() {
                    match std::fs::copy(&file, cfg.models_dir.join(name)) {
                        Ok(_) => {
                            println!("installed {name}");
                            ok += 1;
                        }
                        Err(e) => eprintln!("could not copy {name}: {e}"),
                    }
                } else {
                    eprintln!("missing {name} in {}", p.display());
                }
            }
            return finish(ok);
        }
    }

    let base = from
        .map(|s| s.trim_end_matches('/').to_string())
        .unwrap_or_else(|| {
            "https://github.com/thisisforlearn/bookmark/releases/latest/download".to_string()
        });

    println!("downloading models from {base}");
    let mut ok = 0;
    for name in MODELS {
        let url = format!("{base}/{name}");
        match bk_crawl::download(&url, 64 * 1024 * 1024).await {
            Ok(bytes) => {
                let arc: std::sync::Arc<[u8]> = bytes.clone().into();
                if let Err(e) = bk_model::ModelFile::from_arc(arc) {
                    eprintln!("{name} is invalid, skipping: {e}");
                    continue;
                }
                match std::fs::write(cfg.models_dir.join(name), &bytes) {
                    Ok(_) => {
                        println!(
                            "installed {name} ({})",
                            common::human_bytes(bytes.len() as u64)
                        );
                        ok += 1;
                    }
                    Err(e) => eprintln!("could not write {name}: {e}"),
                }
            }
            Err(e) => eprintln!("could not fetch {name}: {e}"),
        }
    }
    finish(ok)
}

fn finish(ok: usize) -> i32 {
    if ok == 0 {
        eprintln!();
        eprintln!("Nothing installed. You can also build the models locally:");
        eprintln!("    bookmark model train all --mode quick");
        return 1;
    }
    println!();
    println!("{ok} model(s) installed. Restart the server to load them.");
    0
}

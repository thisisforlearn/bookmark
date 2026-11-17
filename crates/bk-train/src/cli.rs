// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

//! `bookmark model train` — build the shipped model bundle.

use std::path::{Path, PathBuf};

use indicatif::{ProgressBar, ProgressStyle};

use bk_core::{Config, Result};
use bk_db::Db;

use crate::{data, embed, export, rank, spell};

fn models_dir(cfg: &Config) -> PathBuf {
    cfg.models_dir.clone()
}

fn config_path() -> PathBuf {
    if let Ok(p) = std::env::var("BOOKMARK_CONFIG") {
        return PathBuf::from(p);
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join(".config/bookmark/config.toml");
    }
    PathBuf::from("/etc/bookmark/config.toml")
}

fn bar(n: u64, label: &str) -> ProgressBar {
    let pb = ProgressBar::new(n);
    pb.set_style(
        ProgressStyle::with_template(&format!(
            "{{spinner}} {label} {{bar:32}} {{pos}}/{{len}} {{msg}}"
        ))
        .unwrap_or_else(|_| ProgressStyle::default_bar()),
    );
    pb
}

/// Train one model. `model` is `spell`, `embed`, `rank` or `all`.
pub fn train(model: &str, _backend: &str, mode: &str) -> i32 {
    let cfg = match Config::load(&config_path()) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {e}");
            return 1;
        }
    };
    if let Err(e) = std::fs::create_dir_all(models_dir(&cfg)) {
        eprintln!("error: cannot create models dir: {e}");
        return 1;
    }
    let db = match Db::open(&cfg.db_path()) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("error: cannot open database: {e}");
            return 1;
        }
    };

    let (epochs, pairs_per_epoch) = match mode {
        "smoke" => (2usize, 1_000usize),
        "quick" => (8, 20_000),
        _ => (25, 80_000),
    };

    let models: Vec<&str> = if model == "all" {
        vec!["spell", "embed", "rank"]
    } else {
        vec![model]
    };
    for m in models {
        let r = match m {
            "spell" => train_spell(&db, &cfg, epochs, pairs_per_epoch),
            "embed" => train_embed(&db, &cfg),
            "rank" => train_rank(&cfg),
            other => {
                eprintln!("error: unknown model `{other}` (use spell|embed|rank|all)");
                return 1;
            }
        };
        if let Err(e) = r {
            eprintln!("error: {e}");
            return 1;
        }
    }
    println!();
    println!("models written to {}", models_dir(&cfg).display());
    println!("Restart the server to load them.");
    0
}

fn train_spell(db: &Db, cfg: &Config, epochs: usize, pairs: usize) -> Result<()> {
    println!("spell: building vocabulary…");
    let vocab = data::build_vocabulary(db, None, 20_000);
    println!("spell: {} words, generating {} pairs", vocab.len(), pairs);
    let pairs = spell::build_pairs(&vocab, pairs);
    let dim = 256usize;
    let hidden = 32usize;
    let pb = bar(epochs as u64, "spell");
    let weights = spell::train(&pairs, dim, hidden, epochs, 0.15, &|e, total, loss| {
        pb.set_position(e as u64 + 1);
        pb.set_message(format!("loss {loss:.4} ({}/{total})", e + 1));
    });
    pb.finish_and_clear();
    let acc = spell::accuracy(&weights, &pairs);
    println!("spell: train accuracy {:.1}%", acc * 100.0);
    let bytes = export::spell_bytes(&weights);
    write_model_file(&models_dir(cfg).join("spell.bkmdl"), &bytes)?;
    Ok(())
}

fn train_embed(db: &Db, cfg: &Config) -> Result<()> {
    println!("embed: building vocabulary…");
    let vocab = data::build_vocabulary(db, None, 20_000);
    println!("embed: {} words → {} dims", vocab.len(), 192);
    let pb = bar(vocab.len() as u64, "embed");
    let weights = embed::build(&vocab, 192);
    pb.set_position(vocab.len() as u64);
    pb.finish_and_clear();
    let bytes = export::embed_bytes(&weights);
    write_model_file(&models_dir(cfg).join("embed.bkmdl"), &bytes)?;
    Ok(())
}

fn train_rank(cfg: &Config) -> Result<()> {
    println!("rank: writing heuristic re-ranker");
    let bytes = export::rank_bytes(&rank::heuristic());
    write_model_file(&models_dir(cfg).join("rank.bkmdl"), &bytes)?;
    Ok(())
}

fn write_model_file(path: &Path, bytes: &[u8]) -> Result<()> {
    std::fs::write(path, bytes)?;
    println!("wrote {} ({})", path.display(), human(bytes.len() as u64));
    Ok(())
}

fn human(n: u64) -> String {
    if n >= 1024 * 1024 {
        format!("{:.1} MiB", n as f64 / (1024.0 * 1024.0))
    } else if n >= 1024 {
        format!("{:.1} KiB", n as f64 / 1024.0)
    } else {
        format!("{n} B")
    }
}

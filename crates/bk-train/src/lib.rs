// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

//! Self-contained model trainer. No deep-learning runtime: the spell model is a
//! small MLP trained with SGD, the embedding model is a deterministic hashing
//! vectorizer, and the re-ranker starts from the tuned heuristic weights. All
//! three export into the `.bkmdl` container the server loads at runtime.
#![allow(clippy::needless_range_loop)]

pub mod cli;
pub mod data;
pub mod embed;
pub mod export;
pub mod rank;
pub mod spell;

pub use cli::train;

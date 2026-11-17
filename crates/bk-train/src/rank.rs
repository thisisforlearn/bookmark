// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

//! LTR re-ranker. The shipped default is an identity first layer plus the tuned
//! heuristic weights, so it reproduces the hand-tuned ranking and can later be
//! replaced by a model trained on real click logs.

pub struct RankWeights {
    pub in_dim: usize,
    pub hidden: usize,
    pub w1: Vec<f32>, // [hidden, in_dim]
    pub b1: Vec<f32>, // [hidden]
    pub w2: Vec<f32>, // [hidden]
    pub b2: f32,
}

/// Weights that reproduce `bk_search`'s heuristic scoring.
pub fn heuristic() -> RankWeights {
    let in_dim = 8usize;
    let hidden = 8usize;
    // Identity projection so each hidden unit sees one feature.
    let mut w1 = vec![0.0f32; hidden * in_dim];
    for j in 0..hidden {
        w1[j * in_dim + j] = 1.0;
    }
    let b1 = vec![0.0f32; hidden];
    // Same weights as `bk_search::heuristic`.
    let w2 = vec![1.0, 0.35, 0.20, 0.30, 0.25, 0.05, 0.03, 0.25];
    let b2 = -0.40;
    RankWeights {
        in_dim,
        hidden,
        w1,
        b1,
        w2,
        b2,
    }
}

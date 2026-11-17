// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

//! Spell model: a 1-hidden-layer MLP over char-n-gram features, trained with
//! plain SGD on a pairwise (typo, candidate) objective. Self-contained so it
//! runs on any target without a deep-learning runtime.

use rand::Rng;

use crate::data;

pub struct SpellWeights {
    pub dim: usize,
    pub hidden: usize,
    pub w1: Vec<f32>, // [hidden, dim]
    pub b1: Vec<f32>, // [hidden]
    pub w2: Vec<f32>, // [hidden]
    pub b2: f32,
}

/// Build synthetic (typo, candidate, label) pairs from a vocabulary.
pub fn build_pairs(vocab: &[(String, u32)], n: usize) -> Vec<(String, String, f32)> {
    let mut rng = rand::rng();
    let mut out = Vec::with_capacity(n);
    if vocab.is_empty() {
        return out;
    }
    while out.len() < n {
        let word = &vocab[rng.random_range(0..vocab.len())].0;
        if word.chars().count() < 3 {
            continue;
        }
        let typo = data::corrupt(word, &mut rng);
        out.push((typo.clone(), word.clone(), 1.0));
        // A negative: the same typo against an unrelated word.
        let other = &vocab[rng.random_range(0..vocab.len())].0;
        if other != word {
            out.push((typo, other.clone(), 0.0));
        }
    }
    out
}

pub fn train(
    pairs: &[(String, String, f32)],
    dim: usize,
    hidden: usize,
    epochs: usize,
    lr: f32,
    progress: &dyn Fn(usize, usize, f32),
) -> SpellWeights {
    let mut rng = rand::rng();
    let mut w1 = vec![0.0f32; hidden * dim];
    for v in w1.iter_mut() {
        *v = rng.random_range(-0.05..0.05);
    }
    let b1 = vec![0.0f32; hidden];
    let mut w2 = vec![0.0f32; hidden];
    for v in w2.iter_mut() {
        *v = rng.random_range(-0.05..0.05);
    }
    let mut b2 = 0.0f32;

    let mut x = vec![0.0f32; dim];
    let mut h = vec![0.0f32; hidden];
    let mut dh = vec![0.0f32; hidden];

    for epoch in 0..epochs {
        let mut loss_sum = 0.0f32;
        for (typo, cand, label) in pairs {
            bk_infer::spell_features(typo, cand, dim, &mut x);
            // forward
            for j in 0..hidden {
                let row = &w1[j * dim..(j + 1) * dim];
                let mut a = b1[j];
                for i in 0..dim {
                    a += row[i] * x[i];
                }
                h[j] = a.max(0.0);
            }
            let mut logit = b2;
            for j in 0..hidden {
                logit += w2[j] * h[j];
            }
            let p = 1.0 / (1.0 + (-logit).exp());
            let y = *label;
            let loss = -(y * (p.max(1e-6)).ln() + (1.0 - y) * ((1.0 - p).max(1e-6)).ln());
            loss_sum += loss;

            // backward
            let dlogit = p - y;
            for j in 0..hidden {
                dh[j] = dlogit * w2[j] * if h[j] > 0.0 { 1.0 } else { 0.0 };
            }
            for j in 0..hidden {
                w2[j] -= lr * dlogit * h[j];
                let row = &mut w1[j * dim..(j + 1) * dim];
                for i in 0..dim {
                    row[i] -= lr * dh[j] * x[i];
                }
            }
            b2 -= lr * dlogit;
        }
        let avg = loss_sum / pairs.len().max(1) as f32;
        progress(epoch, epochs, avg);
    }

    SpellWeights {
        dim,
        hidden,
        w1,
        b1,
        w2,
        b2,
    }
}

/// Mean top-1 accuracy of the trained scorer over `pairs` of positives.
pub fn accuracy(w: &SpellWeights, pairs: &[(String, String, f32)]) -> f32 {
    let mut correct = 0;
    let mut total = 0;
    let mut x = vec![0.0f32; w.dim];
    for (typo, cand, label) in pairs {
        if *label < 0.5 {
            continue;
        }
        bk_infer::spell_features(typo, cand, w.dim, &mut x);
        let mut h = vec![0.0f32; w.hidden];
        for j in 0..w.hidden {
            let row = &w.w1[j * w.dim..(j + 1) * w.dim];
            let mut a = w.b1[j];
            for i in 0..w.dim {
                a += row[i] * x[i];
            }
            h[j] = a.max(0.0);
        }
        let mut logit = w.b2;
        for j in 0..w.hidden {
            logit += w.w2[j] * h[j];
        }
        if logit > 0.0 {
            correct += 1;
        }
        total += 1;
    }
    if total == 0 {
        0.0
    } else {
        correct as f32 / total as f32
    }
}

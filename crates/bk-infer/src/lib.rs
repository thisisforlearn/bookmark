// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

//! Int8 inference kernels and the tiny model runtimes.
//!
//! The numeric kernels have three paths: a scalar reference (always compiled,
//! used as the test oracle), an AVX2 path, and an AVX-VNNI path (`dpwssd`) on
//! x86_64. aarch64 uses NEON/SDOT via `core::arch`. Dispatch is runtime feature
//! detection, so the shipped binary stays portable.
#![allow(clippy::needless_range_loop)]

pub mod kernels;
pub mod models;

pub use kernels::{
    conv1d_int8, dot_i8_scalar, gelu, int8_gemm, layer_norm, maxpool1d, rms_norm, softmax,
};
pub use models::{EmbedModel, ModelSet, RankModel, SpellModel};

/// Thread-local scratch buffers. The hot query path reuses these so steady-state
/// search performs no allocation for model I/O.
#[derive(Default)]
pub struct Scratch {
    pub f32_a: Vec<f32>,
    pub f32_b: Vec<f32>,
    pub f32_c: Vec<f32>,
    pub i32_a: Vec<i32>,
    pub i8_a: Vec<i8>,
    pub bytes: Vec<u8>,
}

impl Scratch {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        self.f32_a.clear();
        self.f32_b.clear();
        self.f32_c.clear();
        self.i32_a.clear();
        self.i8_a.clear();
        self.bytes.clear();
    }
}

/// Shared character-n-gram featurizer for the spell model. Used by both the
/// runtime and the trainer so their feature spaces match exactly.
///
/// The vector is `dim` wide: unigrams of the typo (weight 1.0) and candidate
/// (0.7), plus bigrams/trigrams at 0.5, plus a length-difference feature, then
/// L2-normalised.
pub fn spell_features(typo: &str, cand: &str, dim: usize, out: &mut Vec<f32>) {
    if dim == 0 {
        out.clear();
        return;
    }
    out.clear();
    out.resize(dim, 0.0);
    let bucket = |s: &str| -> usize { (bk_core::xxh3_str(s) % dim as u64) as usize };
    for (src, w) in [(typo, 1.0f32), (cand, 0.7)] {
        let t = src.to_lowercase();
        let chars: Vec<char> = t.chars().collect();
        for c in &chars {
            let mut b = [0u8; 4];
            let s = c.encode_utf8(&mut b);
            out[bucket(s)] += w;
        }
        for n in [2usize, 3] {
            if chars.len() >= n {
                for win in chars.windows(n) {
                    let s: String = win.iter().collect();
                    out[bucket(&s)] += w * 0.5;
                }
            }
        }
    }
    let ld = (typo.chars().count() as f32 - cand.chars().count() as f32).abs();
    out[bucket("len")] += ld * 0.1;
    let norm = out.iter().map(|v| v * v).sum::<f32>().sqrt().max(1e-6);
    for v in out.iter_mut() {
        *v /= norm;
    }
}

/// Hashing vectorizer for a single word: whole-word + character trigrams and
/// 4-grams, hash-projected with a sign, then L2-normalised. Both the trainer and
/// the runtime use this, so vocabulary words and out-of-vocabulary words live in
/// the same space.
pub fn hash_word_unit(word: &str, dim: usize, out: &mut Vec<f32>) {
    out.clear();
    if dim == 0 {
        return;
    }
    out.resize(dim, 0.0);
    let project = |v: &mut Vec<f32>, s: &str, weight: f32| {
        let h = bk_core::xxh3_str(s);
        let idx = (h % dim as u64) as usize;
        let sign = if (h >> 63) & 1 == 0 { 1.0 } else { -1.0 };
        v[idx] += weight * sign;
    };
    project(out, word, 1.0);
    let chars: Vec<char> = word.chars().collect();
    for n in [3usize, 4] {
        if chars.len() >= n {
            for win in chars.windows(n) {
                let s: String = win.iter().collect();
                project(out, &s, 0.5);
            }
        }
    }
    let norm = out.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-6);
    for x in out.iter_mut() {
        *x /= norm;
    }
}

/// Quantize an f32 vector to int8 with a symmetric per-vector scale.
/// Returns the scale; `out` receives the quantized values.
pub fn quantize_symmetric(x: &[f32], out: &mut Vec<i8>) -> f32 {
    let max = x.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    let scale = if max == 0.0 { 1.0 } else { max / 127.0 };
    out.clear();
    out.extend(
        x.iter()
            .map(|&v| (v / scale).round().clamp(-127.0, 127.0) as i8),
    );
    scale
}

/// Dequantize int8 back to f32 using a symmetric scale.
pub fn dequantize_symmetric(x: &[i8], scale: f32, out: &mut Vec<f32>) {
    out.clear();
    out.extend(x.iter().map(|&v| v as f32 * scale));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quant_round_trip() {
        let x = vec![0.0, 1.0, -1.0, 0.5, -0.25];
        let mut q = Vec::new();
        let scale = quantize_symmetric(&x, &mut q);
        let mut back = Vec::new();
        dequantize_symmetric(&q, scale, &mut back);
        for (a, b) in x.iter().zip(back.iter()) {
            assert!((a - b).abs() < 0.02, "{a} vs {b}");
        }
    }
}

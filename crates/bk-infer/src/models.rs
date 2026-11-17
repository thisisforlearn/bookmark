// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

//! Model runtimes. Each model is a thin forward pass over tensors loaded from a
//! `.bkmdl` container. When a model file is missing or corrupt the corresponding
//! `Option` is `None` and the server degrades to lexical search.

use std::path::Path;
use std::sync::Arc;

use bk_core::{Error, Result};
use bk_model::{DType, ModelFile};

use crate::{Scratch, int8_gemm, layer_norm, softmax};

/// All three models, loaded best-effort. `load` never fails.
pub struct ModelSet {
    pub spell: Option<Arc<SpellModel>>,
    pub embed: Option<Arc<EmbedModel>>,
    pub rank: Option<Arc<RankModel>>,
    pub embed_id: String,
}

impl ModelSet {
    /// Load whichever `.bkmdl` files exist in `dir`. Missing/corrupt files are
    /// silently skipped (logged at debug level), so the server always starts.
    pub fn load(dir: &Path) -> Self {
        let spell = load_one(dir, "spell", SpellModel::open);
        let embed = load_one(dir, "embed", EmbedModel::open);
        let rank = load_one(dir, "rank", RankModel::open);
        let embed_id = embed
            .as_ref()
            .map(|e| format!("embed-{}", e.dim()))
            .unwrap_or_else(|| "none".to_string());
        Self {
            spell,
            embed,
            rank,
            embed_id,
        }
    }

    pub fn empty() -> Self {
        Self {
            spell: None,
            embed: None,
            rank: None,
            embed_id: "none".into(),
        }
    }

    pub fn degraded(&self) -> bool {
        self.spell.is_none() || self.embed.is_none() || self.rank.is_none()
    }
}

fn load_one<T>(dir: &Path, name: &str, open: impl Fn(&Path) -> Result<T>) -> Option<Arc<T>> {
    let path = dir.join(format!("{name}.bkmdl"));
    if !path.exists() {
        return None;
    }
    match open(&path) {
        Ok(m) => Some(Arc::new(m)),
        Err(e) => {
            tracing::warn!("model {name}: {e}");
            None
        }
    }
}

fn dequant_f32(mf: &ModelFile, name: &str) -> Result<Vec<f32>> {
    let s = mf.section(name)?;
    let bytes = mf.bytes(name)?;
    let n = s.numel();
    let mut out = Vec::with_capacity(n);
    match s.dtype {
        DType::F32 => {
            for chunk in bytes.as_chunks::<4>().0 {
                out.push(f32::from_le_bytes(*chunk));
            }
        }
        DType::Int8 => {
            for &b in bytes {
                out.push(b as i8 as f32 * s.scale);
            }
        }
        DType::F16 => {
            for chunk in bytes.as_chunks::<2>().0 {
                out.push(f16_to_f32(u16::from_le_bytes(*chunk)) * s.scale);
            }
        }
    }
    Ok(out)
}

fn f16_to_f32(h: u16) -> f32 {
    let sign = ((h >> 15) & 1) as u32;
    let exp = ((h >> 10) & 0x1F) as u32;
    let frac = (h & 0x03FF) as u32;
    let bits = if exp == 0 {
        if frac == 0 {
            sign << 31
        } else {
            // subnormal
            let mut e = 127 - 15 + 1;
            let mut f = frac;
            while f & 0x0400 == 0 {
                f <<= 1;
                e -= 1;
            }
            (sign << 31) | ((e as u32) << 23) | ((f & 0x03FF) << 13)
        }
    } else if exp == 0x1F {
        (sign << 31) | (0xFF << 23) | (frac << 13)
    } else {
        (sign << 31) | ((exp + 127 - 15) << 23) | (frac << 13)
    };
    f32::from_bits(bits)
}

/// Character-n-gram MLP noisy-channel scorer.
pub struct SpellModel {
    dim: usize,
    w1: Vec<f32>,
    b1: Vec<f32>,
    w2: Vec<f32>,
    b2: Vec<f32>,
}

impl SpellModel {
    pub fn open(path: &Path) -> Result<Self> {
        let mf = ModelFile::open(path)?;
        let w1 = dequant_f32(&mf, "w1")?;
        let b1 = dequant_f32(&mf, "b1")?;
        let w2 = dequant_f32(&mf, "w2")?;
        let b2 = dequant_f32(&mf, "b2")?;
        let hidden = b1.len().max(1);
        let dim = w1.len() / hidden;
        Ok(Self {
            dim,
            w1,
            b1,
            w2,
            b2,
        })
    }

    /// Featurize a (typo, candidate) pair into a fixed-length vector.
    fn featurize(&self, typo: &str, cand: &str, out: &mut Vec<f32>) {
        crate::spell_features(typo, cand, self.dim, out);
    }

    /// Score how likely `cand` is the intended spelling of `typo`.
    pub fn score(&self, typo: &str, cand: &str, s: &mut Scratch) -> f32 {
        let mut x = std::mem::take(&mut s.f32_a);
        self.featurize(typo, cand, &mut x);
        let hidden = self.b1.len();
        let mut h = std::mem::take(&mut s.f32_b);
        h.clear();
        h.resize(hidden, 0.0);
        for j in 0..hidden {
            let row = &self.w1[j * self.dim..(j + 1) * self.dim];
            let mut acc = self.b1[j];
            for (a, b) in x.iter().zip(row.iter()) {
                acc += a * b;
            }
            h[j] = acc.max(0.0); // ReLU
        }
        let mut logit = self.b2.first().copied().unwrap_or(0.0);
        for (j, v) in h.iter().enumerate() {
            logit += v * self.w2.get(j).copied().unwrap_or(0.0);
        }
        s.f32_a = x;
        s.f32_b = h;
        1.0 / (1.0 + (-logit).exp())
    }

    /// Pick the best candidate for a typo.
    pub fn correct(&self, typo: &str, cands: &[String]) -> Option<(String, f32)> {
        let mut scratch = Scratch::new();
        let mut best: Option<(String, f32)> = None;
        for c in cands {
            let sc = self.score(typo, c, &mut scratch);
            if best.as_ref().is_none_or(|(_, b)| sc > *b) {
                best = Some((c.clone(), sc));
            }
        }
        best
    }
}

/// Bag-of-token-embeddings encoder: mean of int8 embedding rows, LayerNorm,
/// optional projection, L2 normalise, then symmetric int8 quantisation.
pub struct EmbedModel {
    dim: usize,
    emb: Vec<i8>,
    emb_scale: f32,
    vocab_size: usize,
    vocab: Vec<String>,
    gamma: Vec<f32>,
    beta: Vec<f32>,
    proj: Option<(Vec<f32>, usize)>, // [dim, dim] row-major
}

impl EmbedModel {
    pub fn open(path: &Path) -> Result<Self> {
        let mf = ModelFile::open(path)?;
        let s = mf.section("emb")?;
        let dim = *s
            .dims
            .last()
            .ok_or_else(|| Error::Model("emb has no dims".into()))? as usize;
        if dim == 0 {
            return Err(Error::Model("emb dim is zero".into()));
        }
        let bytes = mf.bytes("emb")?;
        let emb: Vec<i8> = bytes.iter().map(|&b| b as i8).collect();
        let vocab_size = emb.len() / dim;
        let gamma = dequant_f32(&mf, "gamma").unwrap_or_else(|_| vec![1.0; dim]);
        let beta = dequant_f32(&mf, "beta").unwrap_or_else(|_| vec![0.0; dim]);
        let proj = dequant_f32(&mf, "proj").ok().map(|p| (p, dim));
        Ok(Self {
            dim,
            emb,
            emb_scale: if s.scale == 0.0 { 1.0 } else { s.scale },
            vocab_size,
            vocab: mf.vocab().to_vec(),
            gamma,
            beta,
            proj,
        })
    }

    pub fn dim(&self) -> usize {
        self.dim
    }

    fn token_index(&self, tok: &str) -> Option<usize> {
        // Linear scan is acceptable for the small shipped vocab; a real build
        // swaps in a hash map at load time.
        self.vocab
            .iter()
            .position(|v| v == tok)
            .filter(|i| *i < self.vocab_size)
    }

    /// Encode into an int8 vector. `out` receives the bytes; the return value is
    /// the scale needed to dequantise them.
    pub fn encode(&self, text: &str, out: &mut Vec<u8>, s: &mut Scratch) -> Result<f32> {
        let mut acc = std::mem::take(&mut s.f32_a);
        acc.clear();
        acc.resize(self.dim, 0.0);
        let mut count = 0usize;
        for tok in tokenize(text) {
            if let Some(idx) = self.token_index(&tok) {
                let row = &self.emb[idx * self.dim..(idx + 1) * self.dim];
                for (d, &q) in row.iter().enumerate() {
                    acc[d] += q as f32 * self.emb_scale;
                }
                count += 1;
            }
        }
        if count > 0 {
            let inv = 1.0 / count as f32;
            for v in acc.iter_mut() {
                *v *= inv;
            }
        }
        let mut normed = std::mem::take(&mut s.f32_b);
        normed.clear();
        normed.resize(self.dim, 0.0);
        layer_norm(&acc, &self.gamma, &self.beta, 1e-5, &mut normed);

        let mut proj = Vec::new();
        if let Some((w, d)) = &self.proj {
            proj.resize(self.dim, 0.0);
            for i in 0..self.dim {
                let row = &w[i * d..(i + 1) * d];
                let mut a = 0.0;
                for (x, y) in normed.iter().zip(row.iter()) {
                    a += x * y;
                }
                proj[i] = a;
            }
        }
        let final_vec: &[f32] = if self.proj.is_some() { &proj } else { &normed };

        // L2 normalise then quantise.
        let l2 = final_vec
            .iter()
            .map(|v| v * v)
            .sum::<f32>()
            .sqrt()
            .max(1e-6);
        let mut normed_unit = Vec::with_capacity(self.dim);
        for v in final_vec {
            normed_unit.push(v / l2);
        }
        let mut q = std::mem::take(&mut s.i8_a);
        let scale = crate::quantize_symmetric(&normed_unit, &mut q);
        out.clear();
        out.reserve(q.len());
        out.extend(q.iter().map(|&v| v as u8));
        s.i8_a = q;
        s.f32_a = acc;
        s.f32_b = normed;
        Ok(scale)
    }
}

fn tokenize(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for ch in text.chars() {
        if ch.is_alphanumeric() {
            cur.extend(ch.to_lowercase());
        } else if !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// Small MLP re-ranker over dense features.
pub struct RankModel {
    w1: Vec<f32>,
    b1: Vec<f32>,
    w2: Vec<f32>,
    b2: Vec<f32>,
    in_dim: usize,
    hidden: usize,
}

impl RankModel {
    pub fn open(path: &Path) -> Result<Self> {
        let mf = ModelFile::open(path)?;
        let w1 = dequant_f32(&mf, "w1")?;
        let b1 = dequant_f32(&mf, "b1")?;
        let w2 = dequant_f32(&mf, "w2")?;
        let b2 = dequant_f32(&mf, "b2")?;
        let hidden = b1.len().max(1);
        let in_dim = (w1.len() / hidden).max(1);
        Ok(Self {
            w1,
            b1,
            w2,
            b2,
            in_dim,
            hidden,
        })
    }

    pub fn in_dim(&self) -> usize {
        self.in_dim
    }

    pub fn score(&self, feats: &[f32], s: &mut Scratch) -> f32 {
        let mut h = std::mem::take(&mut s.f32_b);
        h.clear();
        h.resize(self.hidden, 0.0);
        for j in 0..self.hidden {
            let row = &self.w1[j * self.in_dim..(j + 1) * self.in_dim];
            let mut acc = self.b1[j];
            for i in 0..self.in_dim {
                acc += feats.get(i).copied().unwrap_or(0.0) * row[i];
            }
            h[j] = gelu_one(acc);
        }
        let mut logit = self.b2.first().copied().unwrap_or(0.0);
        for (j, v) in h.iter().enumerate() {
            logit += v * self.w2.get(j).copied().unwrap_or(0.0);
        }
        s.f32_b = h;
        1.0 / (1.0 + (-logit).exp())
    }
}

#[inline]
fn gelu_one(v: f32) -> f32 {
    const C: f32 = 0.797_884_6;
    0.5 * v * (1.0 + (C * (v + 0.044_715 * v * v * v)).tanh())
}

/// Unused-but-public helpers kept for the trainer to share code.
pub fn _softmax_ref(x: &[f32], out: &mut [f32]) {
    softmax(x, out);
}
pub fn _gemm_ref(a: &[i8], w: &[i8], m: usize, n: usize, k: usize, out: &mut [i32]) {
    int8_gemm(a, w, m, n, k, out);
}

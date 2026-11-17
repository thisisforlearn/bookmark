// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

//! Embedding model: a hashing vectorizer over character n-grams. Deterministic
//! and training-free, which makes it a dependable default. Words that share
//! morphology land near each other, so the semantic arm still adds useful
//! fuzzy recall on top of lexical search.

pub struct EmbedWeights {
    pub vocab: Vec<String>,
    pub dim: usize,
    pub emb: Vec<i8>,
    pub scale: f32,
    pub gamma: Vec<f32>,
    pub beta: Vec<f32>,
}

/// Build an int8 embedding table for `vocab`.
pub fn build(vocab: &[(String, u32)], dim: usize) -> EmbedWeights {
    let dim = dim.max(32);
    let mut rows: Vec<Vec<f32>> = Vec::with_capacity(vocab.len());
    let mut global_max = 1e-6f32;

    for (word, _) in vocab {
        // Same vectorizer the runtime uses for out-of-vocabulary words.
        let mut v = Vec::with_capacity(dim);
        bk_infer::hash_word_unit(word, dim, &mut v);
        for x in &v {
            global_max = global_max.max(x.abs());
        }
        rows.push(v);
    }

    let scale = global_max / 127.0;
    let mut emb = Vec::with_capacity(rows.len() * dim);
    for row in &rows {
        for &x in row {
            let q = (x / scale).round().clamp(-127.0, 127.0) as i8;
            emb.push(q);
        }
    }

    EmbedWeights {
        vocab: vocab.iter().map(|(w, _)| w.clone()).collect(),
        dim,
        emb,
        scale,
        gamma: vec![1.0; dim],
        beta: vec![0.0; dim],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_and_normalised() {
        let vocab = vec![("rust".to_string(), 1u32), ("rusty".to_string(), 1u32)];
        let a = build(&vocab, 64);
        let b = build(&vocab, 64);
        assert_eq!(a.emb, b.emb);
        assert_eq!(a.emb.len(), 2 * 64);
        assert!(a.scale > 0.0);
    }

    #[test]
    fn similar_words_are_closer() {
        let vocab = vec![
            ("bookmark".to_string(), 1),
            ("bookmarks".to_string(), 1),
            ("elephant".to_string(), 1),
        ];
        let e = build(&vocab, 128);
        let d = |i: usize, j: usize| -> f32 {
            (0..e.dim)
                .map(|k| {
                    let a = e.emb[i * e.dim + k] as f32;
                    let b = e.emb[j * e.dim + k] as f32;
                    (a - b) * (a - b)
                })
                .sum::<f32>()
                .sqrt()
        };
        assert!(
            d(0, 1) < d(0, 2),
            "bookmark~bookmarks should be closer than bookmark~elephant"
        );
    }
}

// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

//! Export trained weights into the `.bkmdl` container.

use bk_model::{ArchFlags, DType, ModelKind, RawSection, write_model};

fn f32_section(name: &str, dims: &[u32], data: &[f32]) -> RawSection {
    let mut payload = Vec::with_capacity(data.len() * 4);
    for v in data {
        payload.extend_from_slice(&v.to_le_bytes());
    }
    RawSection {
        name: name.to_string(),
        dims: dims.to_vec(),
        dtype: DType::F32,
        scale: 1.0,
        zero: 0,
        payload,
    }
}

pub fn spell_bytes(w: &crate::spell::SpellWeights) -> Vec<u8> {
    let sections = vec![
        f32_section("w1", &[w.hidden as u32, w.dim as u32], &w.w1),
        f32_section("b1", &[w.hidden as u32], &w.b1),
        f32_section("w2", &[w.hidden as u32], &w.w2),
        f32_section("b2", &[1], &[w.b2]),
    ];
    write_model(
        ModelKind::Spell,
        ArchFlags::new(true, false, true, false).0,
        &sections,
        &[],
    )
}

pub fn embed_bytes(e: &crate::embed::EmbedWeights) -> Vec<u8> {
    let vocab_rows = e.vocab.len().max(1) as u32;
    let mut emb = RawSection {
        name: "emb".to_string(),
        dims: vec![vocab_rows, e.dim as u32],
        dtype: DType::Int8,
        scale: e.scale,
        zero: 0,
        payload: e.emb.iter().map(|&v| v as u8).collect(),
    };
    emb.payload.shrink_to_fit();
    let sections = vec![
        emb,
        f32_section("gamma", &[e.dim as u32], &e.gamma),
        f32_section("beta", &[e.dim as u32], &e.beta),
    ];
    write_model(
        ModelKind::Embed,
        ArchFlags::new(true, false, true, false).0,
        &sections,
        &e.vocab,
    )
}

pub fn rank_bytes(r: &crate::rank::RankWeights) -> Vec<u8> {
    let sections = vec![
        f32_section("w1", &[r.hidden as u32, r.in_dim as u32], &r.w1),
        f32_section("b1", &[r.hidden as u32], &r.b1),
        f32_section("w2", &[r.hidden as u32], &r.w2),
        f32_section("b2", &[1], &[r.b2]),
    ];
    write_model(
        ModelKind::Rank,
        ArchFlags::new(true, false, true, false).0,
        &sections,
        &[],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use bk_model::ModelFile;

    #[test]
    fn spell_round_trips() {
        let w = crate::spell::train(&[], 64, 8, 0, 0.0, &|_, _, _| {});
        let blob = spell_bytes(&w);
        let mf = ModelFile::from_arc(std::sync::Arc::from(blob)).unwrap();
        assert_eq!(mf.kind(), ModelKind::Spell);
        assert_eq!(mf.section("w1").unwrap().dims, vec![8, 64]);
    }

    #[test]
    fn rank_round_trips() {
        let blob = rank_bytes(&crate::rank::heuristic());
        let mf = ModelFile::from_arc(std::sync::Arc::from(blob)).unwrap();
        assert_eq!(mf.kind(), ModelKind::Rank);
    }
}

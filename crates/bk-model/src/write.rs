// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

//! Writer for the `.bkmdl` container. Used by `bk-train` to export trained models
//! and by the test suite to synthesize valid fixtures.

use crate::crc::Crc32C;
use crate::header::{DType, MAGIC, ModelHeader, ModelKind, VERSION};
use xxhash_rust::xxh3::xxh3_64;

/// A section supplied to the writer: name, shape, dtype, quant params, payload.
#[derive(Clone, Debug)]
pub struct RawSection {
    pub name: String,
    pub dims: Vec<u32>,
    pub dtype: DType,
    pub scale: f32,
    pub zero: i8,
    pub payload: Vec<u8>,
}

/// Build a `.bkmdl` blob from raw sections and a vocab list.
///
/// Layout:
///   [header 25B][section descriptors...][vocab...][payloads...][trailer 8B xxh3]
///
/// The descriptors are written first (so offsets are known before payloads), then
/// the payloads, then an xxh3 trailer of the entire preceding blob.
pub fn write_model(
    kind: ModelKind,
    arch: u32,
    sections: &[RawSection],
    vocab: &[String],
) -> Vec<u8> {
    let crc = Crc32C::new();
    let mut out = Vec::with_capacity(1 << 20);

    // --- header placeholder ---
    let header = ModelHeader {
        magic: MAGIC,
        version: VERSION,
        kind,
        arch,
        tensor_count: sections.len() as u32,
        vocab_len: vocab.len() as u32,
        flags: 0,
    };
    out.extend_from_slice(&header.to_bytes());

    // --- section descriptors ---
    let mut desc_bytes = 0usize;
    for s in sections {
        desc_bytes += 4 + s.name.len() + 1 + 4 + s.dims.len() * 4 + 1 + 4 + 1 + 8 + 8 + 4;
    }
    let mut vocab_bytes = 0usize;
    for tok in vocab {
        vocab_bytes += 4 + tok.len();
    }
    let payload_start = ModelHeader::SIZE + desc_bytes + vocab_bytes;
    let mut off = payload_start;
    for s in sections {
        write_lstr(&mut out, &s.name);
        out.push(s.dims.len() as u8);
        out.extend_from_slice(&(s.dims.len() as u32).to_le_bytes());
        for d in &s.dims {
            out.extend_from_slice(&d.to_le_bytes());
        }
        out.push(s.dtype as u8);
        out.extend_from_slice(&s.scale.to_le_bytes());
        out.push(s.zero as u8);
        out.extend_from_slice(&(off as u64).to_le_bytes());
        out.extend_from_slice(&(s.payload.len() as u64).to_le_bytes());
        let crc_val = crc.checksum(&s.payload);
        out.extend_from_slice(&crc_val.to_le_bytes());
        off += s.payload.len();
    }

    // --- vocab ---
    for tok in vocab {
        write_lstr(&mut out, tok);
    }

    // --- payloads ---
    for s in sections {
        out.extend_from_slice(&s.payload);
    }

    // --- trailer: xxh3 of everything written so far ---
    let trailer = xxh3_64(&out).to_le_bytes();
    out.extend_from_slice(&trailer);
    out
}

fn write_lstr(out: &mut Vec<u8>, s: &str) {
    let b = s.as_bytes();
    out.extend_from_slice(&(b.len() as u32).to_le_bytes());
    out.extend_from_slice(b);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mmap::ModelFile;

    #[test]
    fn round_trip() {
        let sections = vec![RawSection {
            name: "w".into(),
            dims: vec![4, 8],
            dtype: DType::Int8,
            scale: 1.0,
            zero: 0,
            payload: (0..32).map(|i| (i & 0xFF) as u8).collect(),
        }];
        let vocab = vec!["hello".into(), "world".into()];
        let blob = write_model(ModelKind::Spell, 0, &sections, &vocab);
        let mf = ModelFile::from_arc(std::sync::Arc::from(blob)).unwrap();
        assert_eq!(mf.kind(), ModelKind::Spell);
        assert_eq!(mf.vocab(), &["hello", "world"]);
        let s = mf.section("w").unwrap();
        assert_eq!(s.dims, vec![4, 8]);
        assert_eq!(mf.bytes("w").unwrap().len(), 32);
    }

    #[test]
    fn reject_corrupt() {
        let sections = vec![RawSection {
            name: "w".into(),
            dims: vec![4],
            dtype: DType::Int8,
            scale: 1.0,
            zero: 0,
            payload: vec![1, 2, 3],
        }];
        let mut blob = write_model(ModelKind::Spell, 0, &sections, &[]);
        blob[0] ^= 0xFF; // corrupt the magic
        let err = ModelFile::from_arc(std::sync::Arc::from(blob)).unwrap_err();
        assert_eq!(err.kind(), "model");
    }
}

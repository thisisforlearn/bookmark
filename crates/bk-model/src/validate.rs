// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

//! Validation of a `.bkmdl` blob. This is the hot path on every model load and
//! every fuzz input, so it is deliberately branch-light and never panics.

use crate::crc::Crc32C;
use crate::header::{DType, MAGIC, ModelHeader, VERSION};
use crate::section::{Section, read_lstr};

use bk_core::Error;

/// Validate a complete `.bkmdl` blob and return the parsed sections.
///
/// Returns `None` (as `Err`) for any malformed input — never panics, never
/// reads out of bounds.
pub fn validate(bytes: &[u8]) -> Result<Vec<Section>, Error> {
    let crc = Crc32C::new();

    if bytes.len() < ModelHeader::SIZE + 8 {
        return Err(Error::Model("file too short for header".into()));
    }
    let header = ModelHeader::from_bytes(bytes).ok_or_else(|| Error::Model("bad header".into()))?;
    if header.magic != MAGIC {
        return Err(Error::Model("bad magic".into()));
    }
    if header.version != VERSION {
        return Err(Error::Model("unsupported version".into()));
    }
    if header.tensor_count > 256 {
        return Err(Error::Model("implausible tensor count".into()));
    }
    if header.vocab_len > 1 << 24 {
        return Err(Error::Model("implausible vocab size".into()));
    }

    let mut sections = Vec::with_capacity(header.tensor_count as usize);
    let mut off = ModelHeader::SIZE;
    let payload_start = off;

    for i in 0..header.tensor_count as usize {
        let (name, next) = read_lstr(bytes, off)
            .ok_or_else(|| Error::Model(format!("section {i}: missing name")))?;
        off = next;
        if off + 9 > bytes.len() {
            return Err(Error::Model(format!("section {i}: truncated descriptor")));
        }
        let rank = bytes[off] as usize;
        off += 1;
        if rank > 8 {
            return Err(Error::Model(format!("section {i}: bad rank {rank}")));
        }
        if off + 4 + rank * 4 > bytes.len() {
            return Err(Error::Model(format!("section {i}: truncated dims")));
        }
        let ndims = u32::from_le_bytes([bytes[off], bytes[off + 1], bytes[off + 2], bytes[off + 3]])
            as usize;
        off += 4;
        let mut dims = Vec::with_capacity(ndims);
        for _ in 0..ndims {
            let d =
                u32::from_le_bytes([bytes[off], bytes[off + 1], bytes[off + 2], bytes[off + 3]]);
            off += 4;
            dims.push(d);
        }
        let dtype = DType::from_u8(bytes[off])
            .ok_or_else(|| Error::Model(format!("section {i}: bad dtype")))?;
        off += 1;
        let scale =
            f32::from_le_bytes([bytes[off], bytes[off + 1], bytes[off + 2], bytes[off + 3]]);
        off += 4;
        let zero = bytes[off] as i8;
        off += 1;
        let offset = u64::from_le_bytes([
            bytes[off],
            bytes[off + 1],
            bytes[off + 2],
            bytes[off + 3],
            bytes[off + 4],
            bytes[off + 5],
            bytes[off + 6],
            bytes[off + 7],
        ]) as usize;
        off += 8;
        let len = u64::from_le_bytes([
            bytes[off],
            bytes[off + 1],
            bytes[off + 2],
            bytes[off + 3],
            bytes[off + 4],
            bytes[off + 5],
            bytes[off + 6],
            bytes[off + 7],
        ]) as usize;
        off += 8;
        let crc_stored =
            u32::from_le_bytes([bytes[off], bytes[off + 1], bytes[off + 2], bytes[off + 3]]);
        off += 4;

        // Payload must sit after the header and inside the file.
        if offset < payload_start || offset.checked_add(len).is_none_or(|e| e > bytes.len()) {
            return Err(Error::Model(format!("section {i}: payload out of bounds")));
        }
        let payload = &bytes[offset..offset + len];
        let crc_calc = crc.checksum(payload);
        if crc_calc != crc_stored {
            return Err(Error::Model(format!("section {i}: crc mismatch")));
        }
        sections.push(Section {
            name,
            rank: rank as u8,
            dims,
            dtype,
            scale,
            zero,
            offset: offset as u64,
            len: len as u64,
            crc: crc_stored,
            payload: payload.to_vec(),
        });
    }

    // Vocab block: `vocab_len` length-prefixed strings, then an 8-byte xxh3 trailer.
    let mut vocab_off = off;
    for _ in 0..header.vocab_len {
        let (tok, next) = read_lstr(bytes, vocab_off)
            .ok_or_else(|| Error::Model("truncated vocab entry".into()))?;
        vocab_off = next;
        if tok.len() > 512 {
            return Err(Error::Model("implausible vocab token".into()));
        }
    }
    if vocab_off + 8 > bytes.len() {
        return Err(Error::Model("truncated trailer".into()));
    }
    Ok(sections)
}

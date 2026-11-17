// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

//! Zero-copy, mmap-backed model loader.

use std::path::Path;
use std::sync::Arc;

use memmap2::Mmap;

use crate::header::ModelHeader;
use crate::section::{Section, read_lstr};
use crate::validate;

use bk_core::Error;

impl std::fmt::Debug for ModelFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ModelFile")
            .field("kind", &self.header.kind)
            .field("len", &self.len())
            .finish()
    }
}

/// Backing storage for a loaded model. Either an mmap (the normal case) or an
/// in-memory `Arc<[u8]>` (fuzz harness and tests).
enum Data {
    Mmap(Mmap),
    Bytes(Arc<[u8]>),
}

impl Data {
    fn as_slice(&self) -> &[u8] {
        match self {
            Data::Mmap(m) => m.as_ref(),
            Data::Bytes(b) => b.as_ref(),
        }
    }
}

/// A loaded `.bkmdl` file. Tensor payloads are borrowed from the backing bytes;
/// nothing is copied out.
pub struct ModelFile {
    data: Data,
    header: ModelHeader,
    sections: Vec<Section>,
    vocab: Vec<String>,
}

impl ModelFile {
    /// Open and validate a `.bkmdl` file from disk.
    pub fn open(path: &Path) -> Result<Self, Error> {
        let file = std::fs::File::open(path)
            .map_err(|e| Error::Model(format!("open {}: {e}", path.display())))?;
        // SAFETY: we hold the file open for the lifetime of the mmap and never
        // mutate it.
        let mmap = unsafe { Mmap::map(&file) }
            .map_err(|e| Error::Model(format!("mmap {}: {e}", path.display())))?;
        Self::from_data(Data::Mmap(mmap))
    }

    /// Validate and parse an owned byte buffer.
    pub fn from_arc(bytes: Arc<[u8]>) -> Result<Self, Error> {
        Self::from_data(Data::Bytes(bytes))
    }

    fn from_data(data: Data) -> Result<Self, Error> {
        let bytes = data.as_slice();
        let sections = validate::validate(bytes)?;
        let header =
            ModelHeader::from_bytes(bytes).ok_or_else(|| Error::Model("bad header".into()))?;
        let vocab = parse_vocab(bytes, header.vocab_len)?;
        Ok(Self {
            data,
            header,
            sections,
            vocab,
        })
    }

    pub fn header(&self) -> &ModelHeader {
        &self.header
    }

    pub fn kind(&self) -> crate::header::ModelKind {
        self.header.kind
    }

    pub fn vocab(&self) -> &[String] {
        &self.vocab
    }

    pub fn section(&self, name: &str) -> Result<&Section, Error> {
        self.sections
            .iter()
            .find(|s| s.name == name)
            .ok_or_else(|| Error::Model(format!("tensor {name} not found")))
    }

    /// Raw bytes of a tensor payload (already CRC-validated at load time).
    pub fn bytes(&self, name: &str) -> Result<&[u8], Error> {
        let s = self.section(name)?;
        let start = s.offset as usize;
        let end = start
            .checked_add(s.len as usize)
            .ok_or_else(|| Error::Model("tensor overflow".into()))?;
        let data = self.data.as_slice();
        if end > data.len() {
            return Err(Error::Model(format!("tensor {name} out of bounds")));
        }
        Ok(&data[start..end])
    }

    /// Total file size in bytes.
    pub fn len(&self) -> usize {
        self.data.as_slice().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

fn parse_vocab(bytes: &[u8], vocab_len: u32) -> Result<Vec<String>, Error> {
    let mut off = ModelHeader::SIZE;
    let tensor_count = ModelHeader::from_bytes(bytes)
        .map(|h| h.tensor_count)
        .unwrap_or(0);
    for _ in 0..tensor_count {
        let (_name, next) =
            read_lstr(bytes, off).ok_or_else(|| Error::Model("vocab: bad descriptor".into()))?;
        off = next;
        off += 1; // rank byte
        if off + 4 > bytes.len() {
            return Err(Error::Model("vocab: truncated".into()));
        }
        let ndims = u32::from_le_bytes([bytes[off], bytes[off + 1], bytes[off + 2], bytes[off + 3]])
            as usize;
        off += 4 + ndims * 4;
        off += 1 + 4 + 1 + 8 + 8 + 4; // dtype + scale + zero + offset + len + crc
        if off > bytes.len() {
            return Err(Error::Model("vocab: truncated".into()));
        }
    }
    let mut vocab = Vec::with_capacity(vocab_len as usize);
    for _ in 0..vocab_len {
        let (tok, next) =
            read_lstr(bytes, off).ok_or_else(|| Error::Model("truncated vocab".into()))?;
        off = next;
        vocab.push(tok);
    }
    Ok(vocab)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::header::{DType, ModelKind};
    use crate::write::{RawSection, write_model};
    use std::io::Write;

    fn sample_blob() -> Vec<u8> {
        let sections = vec![RawSection {
            name: "w".into(),
            dims: vec![3, 4],
            dtype: DType::Int8,
            scale: 0.5,
            zero: 0,
            payload: (0..12).map(|i| i as u8).collect(),
        }];
        write_model(
            ModelKind::Spell,
            0,
            &sections,
            &["alpha".into(), "beta".into()],
        )
    }

    #[test]
    fn open_from_disk_reads_tensors() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("m.bkmdl");
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(&sample_blob()).unwrap();
        drop(f);
        let mf = ModelFile::open(&path).unwrap();
        assert_eq!(mf.vocab(), &["alpha", "beta"]);
        assert_eq!(mf.bytes("w").unwrap(), &(0..12u8).collect::<Vec<_>>()[..]);
        assert_eq!(mf.len(), sample_blob().len());
    }

    #[test]
    fn reject_truncated() {
        let err = ModelFile::from_arc(std::sync::Arc::from(b"short".to_vec())).unwrap_err();
        assert_eq!(err.kind(), "model");
    }
}

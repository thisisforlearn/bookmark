// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

//! Binary header layout for the `.bkmdl` container.

/// File magic: ASCII `BKMDL1`.
pub const MAGIC: [u8; 6] = *b"BKMDL1";

/// On-disk format version.
pub const VERSION: u16 = 1;

/// Model kinds.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum ModelKind {
    Spell = 0,
    Embed = 1,
    Rank = 2,
}

impl ModelKind {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Spell),
            1 => Some(Self::Embed),
            2 => Some(Self::Rank),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Spell => "spell",
            Self::Embed => "embed",
            Self::Rank => "rank",
        }
    }
}

/// Tensor data type.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum DType {
    Int8 = 0,
    F16 = 1,
    F32 = 2,
}

impl DType {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Int8),
            1 => Some(Self::F16),
            2 => Some(Self::F32),
            _ => None,
        }
    }

    /// Size in bytes of one element.
    #[inline]
    pub fn size_bytes(&self) -> usize {
        match self {
            Self::Int8 => 1,
            Self::F16 => 2,
            Self::F32 => 4,
        }
    }
}

/// Bitfield of runtime SIMD features the model was trained/quantized for.
#[derive(Clone, Copy, Default, Debug)]
pub struct ArchFlags(pub u32);

impl ArchFlags {
    pub const AVX2: u32 = 1 << 0;
    pub const VNNI: u32 = 1 << 1;
    pub const NEON: u32 = 1 << 2;
    pub const FP16: u32 = 1 << 3;

    pub fn new(avx2: bool, vnni: bool, neon: bool, fp16: bool) -> Self {
        Self(
            ((avx2 as u32) * Self::AVX2)
                | ((vnni as u32) * Self::VNNI)
                | ((neon as u32) * Self::NEON)
                | ((fp16 as u32) * Self::FP16),
        )
    }

    pub fn has(&self, flag: u32) -> bool {
        self.0 & flag != 0
    }
}

/// The fixed 25-byte header that prefixes every `.bkmdl` file.
#[derive(Clone, Copy, Debug)]
pub struct ModelHeader {
    pub magic: [u8; 6],
    pub version: u16,
    pub kind: ModelKind,
    pub arch: u32,
    pub tensor_count: u32,
    pub vocab_len: u32,
    pub flags: u32,
}

impl ModelHeader {
    pub const SIZE: usize = 25;

    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < Self::SIZE {
            return None;
        }
        let mut b = [0u8; 25];
        b.copy_from_slice(&bytes[..Self::SIZE]);
        let mut magic = [0u8; 6];
        magic.copy_from_slice(&b[0..6]);
        let version = u16::from_le_bytes([b[6], b[7]]);
        let kind = ModelKind::from_u8(b[8])?;
        let arch = u32::from_le_bytes([b[9], b[10], b[11], b[12]]);
        let tensor_count = u32::from_le_bytes([b[13], b[14], b[15], b[16]]);
        let vocab_len = u32::from_le_bytes([b[17], b[18], b[19], b[20]]);
        let flags = u32::from_le_bytes([b[21], b[22], b[23], b[24]]);
        Some(Self {
            magic,
            version,
            kind,
            arch,
            tensor_count,
            vocab_len,
            flags,
        })
    }

    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        let mut b = [0u8; Self::SIZE];
        b[0..6].copy_from_slice(&self.magic);
        b[6..8].copy_from_slice(&self.version.to_le_bytes());
        b[8] = self.kind as u8;
        b[9..13].copy_from_slice(&self.arch.to_le_bytes());
        b[13..17].copy_from_slice(&self.tensor_count.to_le_bytes());
        b[17..21].copy_from_slice(&self.vocab_len.to_le_bytes());
        b[21..25].copy_from_slice(&self.flags.to_le_bytes());
        b
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_round_trip() {
        let h = ModelHeader {
            magic: MAGIC,
            version: VERSION,
            kind: ModelKind::Embed,
            arch: ArchFlags::new(true, true, false, true).0,
            tensor_count: 7,
            vocab_len: 32000,
            flags: 0,
        };
        let bytes = h.to_bytes();
        let h2 = ModelHeader::from_bytes(&bytes).unwrap();
        assert_eq!(h.magic, h2.magic);
        assert_eq!(h.version, h2.version);
        assert_eq!(h.kind, h2.kind);
        assert_eq!(h.arch, h2.arch);
        assert_eq!(h.tensor_count, h2.tensor_count);
        assert_eq!(h.vocab_len, h2.vocab_len);
    }

    #[test]
    fn bad_kind_rejected() {
        let mut bytes = ModelHeader::from_bytes(
            &ModelHeader {
                magic: MAGIC,
                version: VERSION,
                kind: ModelKind::Spell,
                arch: 0,
                tensor_count: 0,
                vocab_len: 0,
                flags: 0,
            }
            .to_bytes(),
        )
        .unwrap()
        .to_bytes();
        bytes[8] = 99;
        assert!(ModelHeader::from_bytes(&bytes).is_none());
    }
}

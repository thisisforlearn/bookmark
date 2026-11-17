// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

//! Per-section descriptors and zero-copy tensor views.

use crate::header::DType;

/// A single tensor section inside a `.bkmdl` file.
#[derive(Clone, Debug)]
pub struct Section {
    pub name: String,
    pub rank: u8,
    pub dims: Vec<u32>,
    pub dtype: DType,
    /// Per-tensor symmetric quantization scale (fp32).
    pub scale: f32,
    /// Zero point (int8), used for asymmetric quant; symmetric quant sets 0.
    pub zero: i8,
    /// Byte offset of the tensor payload relative to the file start.
    pub offset: u64,
    /// Byte length of the tensor payload.
    pub len: u64,
    /// CRC32C of the payload bytes.
    pub crc: u32,
    /// Raw payload bytes (kept alongside the descriptor so the writer can
    /// serialize without re-reading the file).
    pub payload: Vec<u8>,
}

impl Section {
    /// Number of elements in this tensor.
    #[inline]
    pub fn numel(&self) -> usize {
        self.dims.iter().map(|&d| d as usize).product()
    }

    /// Byte stride between consecutive elements of the innermost dimension.
    #[inline]
    pub fn elem_size(&self) -> usize {
        self.dtype.size_bytes()
    }
}

/// A zero-copy view into a tensor payload.
#[derive(Clone, Copy)]
pub struct TensorView<'a> {
    pub dims: &'a [u32],
    pub dtype: DType,
    pub bytes: &'a [u8],
    pub scale: f32,
    pub zero: i8,
}

impl<'a> TensorView<'a> {
    /// Number of elements.
    #[inline]
    pub fn numel(&self) -> usize {
        self.dims.iter().map(|&d| d as usize).product()
    }

    /// Interpret the payload as a slice of `f32` (only valid when dtype == F32).
    #[inline]
    pub fn as_f32(&self) -> &'a [f32] {
        assert!(self.dtype == DType::F32, "tensor is not f32");
        let n = self.numel();
        let p = self.bytes.as_ptr() as *const f32;
        unsafe { std::slice::from_raw_parts(p, n) }
    }

    /// Interpret the payload as a slice of `i8` (only valid when dtype == Int8).
    #[inline]
    pub fn as_i8(&self) -> &'a [i8] {
        assert!(self.dtype == DType::Int8, "tensor is not int8");
        let n = self.numel();
        let p = self.bytes.as_ptr() as *const i8;
        unsafe { std::slice::from_raw_parts(p, n) }
    }

    /// Interpret the payload as a slice of `u16` (only valid when dtype == F16).
    #[inline]
    pub fn as_f16(&self) -> &'a [u16] {
        assert!(self.dtype == DType::F16, "tensor is not f16");
        let n = self.numel();
        let p = self.bytes.as_ptr() as *const u16;
        unsafe { std::slice::from_raw_parts(p, n) }
    }
}

/// Read a null-terminated UTF-8 string from a byte slice at `offset`.
#[inline]
pub fn read_cstr(bytes: &[u8], offset: usize) -> Option<String> {
    let end = bytes[offset..].iter().position(|&b| b == 0)?;
    Some(String::from_utf8_lossy(&bytes[offset..offset + end]).into_owned())
}

/// Parse a 4-byte length-prefixed UTF-8 string.
#[inline]
pub fn read_lstr(bytes: &[u8], offset: usize) -> Option<(String, usize)> {
    if offset + 4 > bytes.len() {
        return None;
    }
    let len = u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ]) as usize;
    if offset + 4 + len > bytes.len() {
        return None;
    }
    let s = String::from_utf8_lossy(&bytes[offset + 4..offset + 4 + len]).into_owned();
    Some((s, offset + 4 + len))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numel() {
        let s = Section {
            name: "w".into(),
            rank: 2,
            dims: vec![4, 8],
            dtype: DType::Int8,
            scale: 1.0,
            zero: 0,
            offset: 0,
            len: 32,
            crc: 0,
            payload: vec![],
        };
        assert_eq!(s.numel(), 32);
        assert_eq!(s.elem_size(), 1);
    }

    #[test]
    fn lstr_round_trip() {
        let mut buf = Vec::new();
        let s = b"hello";
        buf.extend_from_slice(&(s.len() as u32).to_le_bytes());
        buf.extend_from_slice(s);
        let (out, next) = read_lstr(&buf, 0).unwrap();
        assert_eq!(out, "hello");
        assert_eq!(next, buf.len());
    }
}

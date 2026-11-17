// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

//! The `.bkmdl` flat int8 container.

//!

//! A `.bkmdl` file is a single mmap-able blob:

//!

//!   [magic:6][version:2][kind:1][arch:4][tensor_count:4][vocab_len:4][flags:4]
//!   [sections...][vocab...][trailer:8]

//!

//! Every section is self-describing: name, rank, dims, dtype, per-tensor scale/zero,
//! byte offset and length, plus a CRC32C of the section bytes. Validation never
//! panics: any malformed input is returned as `Error::Model`.

pub mod crc;
pub mod header;
pub mod mmap;
pub mod section;
pub mod validate;
pub mod write;

pub use header::{ArchFlags, DType, ModelHeader, ModelKind};
pub use mmap::ModelFile;
pub use section::{Section, TensorView};
pub use validate::validate;
pub use write::{RawSection, write_model};

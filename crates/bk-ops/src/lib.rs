// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

//! Operational helpers: backup/restore, diagnostics, and service files.

pub mod backup;
pub mod doctor;
pub mod paths;
pub mod service;

pub use doctor::{Check, DoctorReport};
pub use paths::{config_path, data_dir, models_dir};
pub use service::ServiceKind;

// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Data between the program and the database: descriptions of columns
//! and markers, and values in the program's buffers.

pub mod convert;
pub mod describe;

pub use describe::{ColDesc, ParamDesc};

/// Text in CCSID 37; a character it lacks becomes the substitute
/// (`X'3F'`).
#[must_use]
pub fn ebcdic(s: &str) -> Vec<u8> {
    s.chars().map(|c| ccsid::byte_of(c).unwrap_or(0x3F)).collect()
}

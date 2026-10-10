// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The DDS form's columns, 1-based and inclusive.

/// Sequence number, ignored by the compiler.
pub const SEQ: (usize, usize) = (1, 5);
/// Form type: blank or `A`.
pub const FORM: usize = 6;
/// `*` marks a comment line.
pub const COMMENT: usize = 7;
/// Conditioning indicators (display and printer files).
pub const CONDITIONING: (usize, usize) = (8, 16);
/// Name type: `R`, `K`, `J`, `S`, `O` or blank.
pub const NAME_TYPE: usize = 17;
/// The name.
pub const NAME: (usize, usize) = (19, 28);
/// `R` marks a reference field.
pub const REFERENCE: usize = 29;
/// Length, right-justified.
pub const LENGTH: (usize, usize) = (30, 34);
/// Data type letter.
pub const DATA_TYPE: usize = 35;
/// Decimal positions.
pub const DECIMALS: (usize, usize) = (36, 37);
/// Usage.
pub const USAGE: usize = 38;
/// Location (display and printer files).
pub const LOCATION: (usize, usize) = (39, 44);
/// Keywords.
pub const FUNCTIONS: (usize, usize) = (45, 80);

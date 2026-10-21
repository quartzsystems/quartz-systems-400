// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The SCS (SNA character string) subset spooled files hold: single-byte
//! controls, the presentation position command, transparent data and the
//! `X'2B'` format commands. `docs/WORKMGMT.md` lists the subset.

pub mod decode;
pub mod encode;

pub use decode::{Page, decode};
pub use encode::Encoder;

/// Form feed: line 1 of the next page.
pub const FF: u8 = 0x0C;
/// Carriage return: column 1 of the same line.
pub const CR: u8 = 0x0D;
/// New line: column 1 of the next line.
pub const NL: u8 = 0x15;
/// Backspace.
pub const BS: u8 = 0x16;
/// Interchange record separator: a new line.
pub const IRS: u8 = 0x1E;
/// Line feed: the next line, same column.
pub const LF: u8 = 0x25;
/// Bell: stop and alert the operator.
pub const BEL: u8 = 0x2F;
/// Presentation position: `34 sub n`.
pub const PP: u8 = 0x34;
/// Transparent: `35 n` then `n` bytes taken as they are.
pub const TRN: u8 = 0x35;
/// The format command prefix: `2B class len params`; `len` counts
/// itself and the parameters.
pub const CSP: u8 = 0x2B;

/// `PP` absolute horizontal: to column `n`.
pub const AHPP: u8 = 0xC0;
/// `PP` relative horizontal: right `n` columns.
pub const RHPP: u8 = 0xC8;
/// `PP` absolute vertical: to line `n`, on the next page when `n` is
/// above the current line.
pub const AVPP: u8 = 0x4C;
/// `PP` relative vertical: down `n` lines.
pub const RVPP: u8 = 0xC4;

/// `2B C1`: set horizontal format, the maximum presentation position.
pub const SHF: u8 = 0xC1;
/// `2B C2`: set vertical format, the maximum presentation line.
pub const SVF: u8 = 0xC2;
/// `2B C6`: set line density, in 1/72 inch.
pub const SLD: u8 = 0xC6;
/// `2B D1`: set the graphic character set (`SCG`).
pub const SCG: u8 = 0xD1;
/// `2B D2`: page presentation; subcommand `29` sets characters per inch.
pub const PAGE: u8 = 0xD2;
/// The `PAGE` subcommand for characters per inch.
pub const SCD: u8 = 0x29;

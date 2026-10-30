// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Archives: `pax`, `tar`, `compress`, `uncompress`, `zcat` and `ajar`.
//!
//! [`member`] is the model every format reads and writes; [`ustar`] and
//! [`cpio`] are the formats, [`codec`] the one interface over them;
//! [`gather`] and [`build`] make members from files, [`restore`] makes
//! files from members; [`select`], [`subst`] and [`listing`] are the
//! patterns, renames and verbose lines. [`lzw`] is `compress`'s coder;
//! [`zip`] the container, deflate and inflate behind `ajar`.

pub mod ajar;
pub mod arcio;
pub mod build;
pub mod codec;
pub mod compress;
pub mod cpio;
pub mod gather;
pub mod listing;
pub mod lzw;
pub mod member;
pub mod pax;
pub mod restore;
pub mod select;
pub mod subst;
pub mod tar;
pub mod ustar;
pub mod zip;

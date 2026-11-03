// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The shift state of mixed single- and double-byte CCSIDs (`ccsid`'s
//! 930, 939, 5026, 5035): where a conversion starts in a file, where it
//! stopped, and the output put back in single-byte state.

use ccsid::conv::{Codec, Converter};

pub use ccsid::conv::{SI, SO, Shift};

/// Whether `ccsid` mixes single- and double-byte characters.
#[must_use]
pub fn is_mixed(ccsid: u16) -> bool {
    matches!(Codec::of(ccsid), Ok(Some(c)) if c.is_mixed())
}

/// `c`, decoding from state `s`.
#[must_use]
pub fn start(c: Converter, s: Shift) -> Converter {
    c.with_shift(s)
}

/// The input's state after the bytes `c` has consumed.
#[must_use]
pub fn input(c: &Converter) -> Shift {
    c.input_shift()
}

/// Put `c`'s output back in single-byte state, keeping its pending
/// input.
pub fn flush(c: &mut Converter, out: &mut Vec<u8>) {
    c.flush_shift(out);
}

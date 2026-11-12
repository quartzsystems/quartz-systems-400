// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What the store's format fixes for its parity sets; the layout they
//! go into is `protect::parity`'s. See `docs/STORE_FORMAT.md` §11.

use protect::parity::Format;

use crate::page::trailer;

/// Magic of a partial-parity slot header.
pub const PPL_MAGIC: &[u8; 8] = b"QS400PPL";
/// Magic of the state page.
pub const STATE_MAGIC: &[u8; 8] = b"QS400PST";

/// The two magics, and a page's trailer checksum as the sign of damage.
pub const FORMAT: Format = Format {
    ppl_magic: *PPL_MAGIC,
    state_magic: *STATE_MAGIC,
    suspicious,
};

/// Whether a page is not zero and fails its trailer checksum.
fn suspicious(page: &[u8]) -> bool {
    page.iter().any(|&b| b != 0) && trailer::stored_crc(page) != trailer::checksum(page)
}

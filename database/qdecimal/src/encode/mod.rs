// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The field encodings: how a [`Decimal`](crate::Decimal) is laid down
//! in a record and read back. Byte layouts are in `docs/DECIMAL.md`.

mod binary;
mod float;
mod packed;
mod zoned;

pub use binary::{Binary, Integer};
pub use float::Float;
pub use packed::Packed;
pub use zoned::Zoned;

use crate::error::{DecimalError, Result};

/// The sign a nibble or zone means, when it is one.
fn sign_of(nibble: u8) -> Option<bool> {
    match nibble {
        0xA | 0xC | 0xE | 0xF => Some(false),
        0xB | 0xD => Some(true),
        _ => None,
    }
}

fn bad(what: String) -> DecimalError {
    DecimalError::DecimalData { what }
}

/// The buffer must be exactly the field's size.
fn check_len(bytes: &[u8], want: usize) -> Result<()> {
    if bytes.len() == want {
        Ok(())
    } else {
        Err(bad(format!(
            "{} bytes given for a {want}-byte field",
            bytes.len()
        )))
    }
}

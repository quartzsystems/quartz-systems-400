// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What a product says of its record.

use uuid::Uuid;

/// Bytes a copy has when the product has no reason for another size.
pub const SLOT_BYTES: usize = 64 * 1024;

/// One product's record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Spec {
    /// The eight bytes every copy starts with.
    pub magic: [u8; 8],
    /// The payload's format version; a copy with another is not read.
    pub version: u32,
    /// GPT type of the partition the record lives on.
    pub type_guid: Uuid,
    /// Bytes reserved for each copy. Copy A is at 0, copy B at
    /// `slot_bytes`.
    pub slot_bytes: usize,
    /// The payload's fixed part: a copy with less is malformed.
    pub min_payload: usize,
}

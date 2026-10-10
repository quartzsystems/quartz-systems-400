// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Tag-length-value entries: tag u16, len u32, data.

use crate::error::LicError;
use crate::format::{Reader, put_u16, put_u32};

/// Reserved: one IPL history record.
pub(crate) const TAG_HISTORY: u16 = 0x0002;
/// First tag available to other crates. Lower tags are reserved here.
pub const TAG_USER_MIN: u16 = 0x0100;

/// One entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tlv {
    /// Tag. See `TAG_USER_MIN`.
    pub tag: u16,
    /// Payload.
    pub data: Vec<u8>,
}

impl Tlv {
    pub(crate) fn encode(&self, out: &mut Vec<u8>) {
        put_u16(out, self.tag);
        put_u32(
            out,
            u32::try_from(self.data.len()).expect("TLV under 4 GiB"),
        );
        out.extend_from_slice(&self.data);
    }

    pub(crate) fn decode(r: &mut Reader<'_>) -> Result<Self, LicError> {
        let tag = r.u16()?;
        let len = usize::try_from(r.u32()?).map_err(|_| LicError::Malformed("tlv length"))?;
        Ok(Self {
            tag,
            data: r.bytes(len)?.to_vec(),
        })
    }
}

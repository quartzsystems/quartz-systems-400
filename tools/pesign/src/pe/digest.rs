// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The Authenticode image digest, as the firmware's verifier computes it
//! (EDK2's `DxeImageVerificationLib`): the headers without the checksum
//! and the certificate table entry, each section's raw data by file
//! offset, then whatever follows them up to the certificate table.

use sha2::{Digest, Sha256};

use super::Layout;
use crate::Error;

/// SHA-256 over `image` the Authenticode way.
pub fn sha256(image: &[u8]) -> Result<[u8; 32], Error> {
    let l = Layout::of(image)?;
    let mut h = Sha256::new();
    h.update(&image[..l.checksum]);
    h.update(&image[l.checksum + 4..l.cert_entry]);
    h.update(&image[l.cert_entry + 8..l.headers]);
    let mut hashed = l.headers;
    for s in &l.sections {
        h.update(&image[s.offset..s.offset + s.size]);
        hashed += s.size;
    }
    let end = image.len() - l.cert_table.1;
    if end > hashed {
        h.update(&image[hashed..end]);
    }
    Ok(h.finalize().into())
}

// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! An object's digest (`SAVE_FORMAT.md` §5): its content only, so the
//! signature made when it was built holds after renames, ownership and
//! authority changes, use, saves and restores.

use std::collections::BTreeMap;

use sha2::{Digest, Sha256};

use crate::object::{ObjectDescriptor, Part, flags};

/// The digest of a descriptor's content fields and its parts.
#[must_use]
pub fn digest(desc: &ObjectDescriptor, parts: &BTreeMap<Part, Vec<u8>>) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(b"QS400OBJ");
    h.update(desc.type_code.to_be_bytes());
    h.update(desc.attribute);
    h.update([desc.domain, desc.state, u8::from(desc.has(flags::ADOPTS))]);
    for (part, bytes) in parts.iter().filter(|(_, b)| !b.is_empty()) {
        h.update([part.0]);
        h.update((bytes.len() as u64).to_be_bytes());
        h.update(bytes);
    }
    h.finalize().into()
}

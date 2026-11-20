// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `QSICSL`: the consoles pinned, at most [`MAX_CONSOLES`], in the order
//! they paired. Each is the name it gave, its certificate's SHA-256,
//! when it paired, when it last had a session and from where.
//!
//! ```text
//! 0   magic "QS400QSC"   8
//! 8   version u16        2   = 1
//! 10  Vec<Console>           ipc wire
//! ```

use ipc::wire_struct;
use qsitrust::Fingerprint;

use super::{StoreError, body, file};

/// The file's name under [`super::DIR`].
pub const NAME: &str = "QSICSL";
/// Its magic.
pub const MAGIC: &[u8; 8] = b"QS400QSC";
/// Its format version.
pub const FORMAT_VERSION: u16 = 1;
/// The most consoles paired at once.
pub const MAX_CONSOLES: usize = 2;
/// The longest console name kept.
pub const MAX_NAME: usize = 64;

/// One paired console.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Console {
    /// The name it gave when it paired.
    pub name: String,
    /// Its certificate's SHA-256.
    pub fingerprint: [u8; 32],
    /// When it paired, Unix seconds.
    pub paired: u64,
    /// When it last had a session, Unix seconds; 0 for never.
    pub last_contact: u64,
    /// The address it paired or last connected from.
    pub address: String,
}
wire_struct!(Console {
    name,
    fingerprint,
    paired,
    last_contact,
    address
});

impl Console {
    /// Its fingerprint.
    #[must_use]
    pub fn print(&self) -> Fingerprint {
        Fingerprint(self.fingerprint)
    }
}

/// A console's name as kept: no control characters, at most
/// [`MAX_NAME`] bytes.
#[must_use]
pub fn kept_name(name: &str) -> String {
    let clean: String = name.chars().filter(|c| !c.is_control()).collect();
    let mut end = clean.len().min(MAX_NAME);
    while !clean.is_char_boundary(end) {
        end -= 1;
    }
    clean[..end].to_string()
}

/// The file's bytes.
#[must_use]
pub fn encode(consoles: &[Console]) -> Vec<u8> {
    file(*MAGIC, FORMAT_VERSION, &ipc::encode(&consoles.to_vec()))
}

/// The consoles a file holds.
///
/// # Errors
/// Not the file, another version, or more consoles than may be, or one
/// paired twice.
pub fn decode(bytes: &[u8]) -> Result<Vec<Console>, StoreError> {
    let b = body(bytes, *MAGIC, FORMAT_VERSION)?;
    let list: Vec<Console> = ipc::decode(b).map_err(|e| StoreError::Body(e.to_string()))?;
    if list.len() > MAX_CONSOLES {
        return Err(StoreError::Invalid("more consoles than may be paired"));
    }
    if list.len() == 2 && list[0].fingerprint == list[1].fingerprint {
        return Err(StoreError::Invalid("a console paired twice"));
    }
    if list.iter().any(|c| c.name.len() > MAX_NAME) {
        return Err(StoreError::Invalid("a console's name is too long"));
    }
    Ok(list)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consoles_round_trip_and_are_checked() {
        let c = Console {
            name: "QS/MC".into(),
            fingerprint: [7; 32],
            paired: 1,
            last_contact: 2,
            address: "192.0.2.1:5000".into(),
        };
        let bytes = encode(std::slice::from_ref(&c));
        assert_eq!(&bytes[..8], MAGIC);
        assert_eq!(decode(&bytes).unwrap(), vec![c.clone()]);
        assert_eq!(decode(&encode(&[])).unwrap(), Vec::new());
        assert_eq!(decode(b"junk"), Err(StoreError::Magic));
        let twice = encode(&[c.clone(), c.clone()]);
        assert!(decode(&twice).is_err());
        let three = encode(&[c.clone(), c.clone(), c]);
        assert!(decode(&three).is_err());
        let mut v2 = bytes.clone();
        v2[9] = 2;
        assert_eq!(decode(&v2), Err(StoreError::Version(2)));
        assert_eq!(kept_name("a\u{7}b"), "ab");
        assert_eq!(kept_name(&"é".repeat(40)).len(), 64);
    }
}

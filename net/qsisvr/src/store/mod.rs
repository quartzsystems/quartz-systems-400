// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What the QSI server keeps, in three stream files under [`DIR`]
//! (`docs/QSID.md` section 12, frozen at V1R2M0): the consoles pinned
//! ([`consoles`]), the system's identity sealed by the LIC's key slot
//! ([`identity`]), and the event log with what it last said
//! ([`events`]). Each is an eight-byte magic, a version, then its body
//! in the `ipc` wire form.

pub mod consoles;
pub mod events;
pub mod identity;

/// Where the files live.
pub const DIR: &str = "/QIBM/UserData/OS400/QSI";

/// Why a file could not be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StoreError {
    /// It does not start with the file's magic.
    Magic,
    /// A version this release cannot read.
    Version(u32),
    /// Its body does not read.
    Body(String),
    /// What it holds is not valid.
    Invalid(&'static str),
    /// The sealed part does not open (another LIC, a damaged file).
    Sealed,
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Magic => f.write_str("not a QSI server file"),
            Self::Version(v) => write!(f, "format version {v} is not one this release reads"),
            Self::Body(e) => write!(f, "its body does not read: {e}"),
            Self::Invalid(why) => write!(f, "not valid: {why}"),
            Self::Sealed => f.write_str("its sealed part does not open with this LIC's key"),
        }
    }
}

impl std::error::Error for StoreError {}

/// The bytes after `magic` and a two-byte version that must be
/// `version`.
pub(crate) fn body(bytes: &[u8], magic: [u8; 8], version: u16) -> Result<&[u8], StoreError> {
    if bytes.len() < 10 || bytes[..8] != magic {
        return Err(StoreError::Magic);
    }
    let v = u16::from_be_bytes([bytes[8], bytes[9]]);
    if v != version {
        return Err(StoreError::Version(u32::from(v)));
    }
    Ok(&bytes[10..])
}

/// `magic`, `version`, then `body`.
pub(crate) fn file(magic: [u8; 8], version: u16, body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(10 + body.len());
    out.extend_from_slice(&magic);
    out.extend_from_slice(&version.to_be_bytes());
    out.extend_from_slice(body);
    out
}

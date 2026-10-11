// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The binary module format that `*MODULE` objects store.
//!
//! Layout: the magic `QIRM`, a `u16` format version, a `u16` of zero,
//! then the module as tagged, length-free fields in a fixed order.
//! Integers are little-endian; strings and byte arrays carry a `u32`
//! length; lists carry a `u32` count; options carry a `u8` presence
//! flag; enumerations carry a `u8` tag. Within V1R the format only
//! grows: a new instruction or type gets a new tag, a new field goes
//! at the end of its record behind a version bump, and a reader
//! accepts every version up to its own.

mod read;
mod write;

pub use read::read;
pub use write::write;

/// The four bytes a module starts with.
pub const MAGIC: &[u8; 4] = b"QIRM";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::module::FORMAT_VERSION;
    use crate::{Module, Proc};

    #[test]
    fn header_and_version() {
        let mut m = Module::new("T");
        m.procs.push(Proc::new("P"));
        let bytes = write(&m);
        assert_eq!(&bytes[..4], MAGIC);
        assert_eq!(u16::from_le_bytes([bytes[4], bytes[5]]), FORMAT_VERSION);
        assert_eq!(read(&bytes).unwrap(), m);

        let mut future = bytes.clone();
        future[4..6].copy_from_slice(&(FORMAT_VERSION + 1).to_le_bytes());
        assert!(read(&future).unwrap_err().to_string().contains("version"));

        let mut bad = bytes.clone();
        bad[0] = b'X';
        assert!(read(&bad).unwrap_err().to_string().contains("magic"));

        assert!(read(&bytes[..bytes.len() - 1]).is_err());
    }
}

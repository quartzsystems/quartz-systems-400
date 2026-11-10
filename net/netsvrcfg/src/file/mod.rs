// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `QAZLSCFG` and `QAZLSSHR`, the two stream files under
//! `/QIBM/UserData/OS400/NetServer/` (`docs/NETSERVER.md` 4.4, a frozen
//! format). Unknown records are kept and written back.

pub mod cfg;
pub mod shr;
pub mod tlv;

/// The files' format version.
pub const FORMAT_VERSION: u16 = 1;

/// Where the files live.
pub const DIR: &str = "/QIBM/UserData/OS400/NetServer";
/// The attribute file.
pub const CFG_PATH: &str = "/QIBM/UserData/OS400/NetServer/QAZLSCFG";
/// The share file.
pub const SHR_PATH: &str = "/QIBM/UserData/OS400/NetServer/QAZLSSHR";

use crate::model::config::Config;

/// Why a file could not be read or written.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum FileError {
    /// It does not start with the file's magic.
    #[error("not a NetServer configuration file")]
    Magic,
    /// A version this release cannot read.
    #[error("format version {0} is not one this release reads")]
    Version(u16),
    /// It ends inside a record at this offset.
    #[error("the file ends inside a record at offset {0}")]
    Truncated(usize),
    /// Bytes after the last record.
    #[error("{0} bytes follow the last record")]
    Trailing(usize),
    /// A record's value is not valid for its tag.
    #[error("record {0:#06x} has a value that is not valid")]
    Value(u16),
    /// More records than the count holds.
    #[error("{0} records do not fit the file")]
    TooMany(usize),
    /// A value too long for a record.
    #[error("record {0:#06x} is too long")]
    TooLong(u16),
    /// A share record before any share name.
    #[error("record {0:#06x} comes before any share")]
    Orphan(u16),
}

/// Both files' bytes.
pub fn write(c: &Config) -> Result<(Vec<u8>, Vec<u8>), FileError> {
    Ok((cfg::write(c)?, shr::write(c)?))
}

/// The configuration of both files; `system_name` names the server
/// when the attribute file has no name.
pub fn read(cfg_bytes: &[u8], shr_bytes: &[u8], system_name: &str) -> Result<Config, FileError> {
    let (attrs, unknown) = cfg::read(cfg_bytes, system_name)?;
    let (shares, unknown_shares) = shr::read(shr_bytes)?;
    Ok(Config {
        attrs,
        shares,
        unknown,
        unknown_shares,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_shipped_configuration_round_trips() {
        let c = Config::shipped("QS400");
        let (a, s) = write(&c).expect("writes");
        assert_eq!(read(&a, &s, "OTHER").expect("reads"), c);
    }
}

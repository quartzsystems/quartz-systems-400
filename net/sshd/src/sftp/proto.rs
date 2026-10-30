// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! SFTP version 3 (draft-ietf-secsh-filexfer-02, the version OpenSSH
//! speaks): packet types, status codes, open flags and `ATTRS`.

use crate::wire::{Reader, WireResult, Writer};

/// `SSH_FXP_INIT`.
pub const INIT: u8 = 1;
/// `SSH_FXP_VERSION`.
pub const VERSION: u8 = 2;
/// `SSH_FXP_OPEN`.
pub const OPEN: u8 = 3;
/// `SSH_FXP_CLOSE`.
pub const CLOSE: u8 = 4;
/// `SSH_FXP_READ`.
pub const READ: u8 = 5;
/// `SSH_FXP_WRITE`.
pub const WRITE: u8 = 6;
/// `SSH_FXP_LSTAT`.
pub const LSTAT: u8 = 7;
/// `SSH_FXP_FSTAT`.
pub const FSTAT: u8 = 8;
/// `SSH_FXP_SETSTAT`.
pub const SETSTAT: u8 = 9;
/// `SSH_FXP_FSETSTAT`.
pub const FSETSTAT: u8 = 10;
/// `SSH_FXP_OPENDIR`.
pub const OPENDIR: u8 = 11;
/// `SSH_FXP_READDIR`.
pub const READDIR: u8 = 12;
/// `SSH_FXP_REMOVE`.
pub const REMOVE: u8 = 13;
/// `SSH_FXP_MKDIR`.
pub const MKDIR: u8 = 14;
/// `SSH_FXP_RMDIR`.
pub const RMDIR: u8 = 15;
/// `SSH_FXP_REALPATH`.
pub const REALPATH: u8 = 16;
/// `SSH_FXP_STAT`.
pub const STAT: u8 = 17;
/// `SSH_FXP_RENAME`.
pub const RENAME: u8 = 18;
/// `SSH_FXP_READLINK`.
pub const READLINK: u8 = 19;
/// `SSH_FXP_SYMLINK`.
pub const SYMLINK: u8 = 20;
/// `SSH_FXP_STATUS`.
pub const STATUS: u8 = 101;
/// `SSH_FXP_HANDLE`.
pub const HANDLE: u8 = 102;
/// `SSH_FXP_DATA`.
pub const DATA: u8 = 103;
/// `SSH_FXP_NAME`.
pub const NAME: u8 = 104;
/// `SSH_FXP_ATTRS`.
pub const ATTRS: u8 = 105;
/// `SSH_FXP_EXTENDED`.
pub const EXTENDED: u8 = 200;
/// `SSH_FXP_EXTENDED_REPLY`.
pub const EXTENDED_REPLY: u8 = 201;

/// Status codes.
pub mod status {
    /// `SSH_FX_OK`.
    pub const OK: u32 = 0;
    /// `SSH_FX_EOF`.
    pub const EOF: u32 = 1;
    /// `SSH_FX_NO_SUCH_FILE`.
    pub const NO_SUCH_FILE: u32 = 2;
    /// `SSH_FX_PERMISSION_DENIED`.
    pub const PERMISSION_DENIED: u32 = 3;
    /// `SSH_FX_FAILURE`.
    pub const FAILURE: u32 = 4;
    /// `SSH_FX_BAD_MESSAGE`.
    pub const BAD_MESSAGE: u32 = 5;
    /// `SSH_FX_OP_UNSUPPORTED`.
    pub const OP_UNSUPPORTED: u32 = 8;
}

/// Open flags.
pub mod pflags {
    /// `SSH_FXF_READ`.
    pub const READ: u32 = 0x01;
    /// `SSH_FXF_WRITE`.
    pub const WRITE: u32 = 0x02;
    /// `SSH_FXF_APPEND`.
    pub const APPEND: u32 = 0x04;
    /// `SSH_FXF_CREAT`.
    pub const CREAT: u32 = 0x08;
    /// `SSH_FXF_TRUNC`.
    pub const TRUNC: u32 = 0x10;
    /// `SSH_FXF_EXCL`.
    pub const EXCL: u32 = 0x20;
}

const SIZE: u32 = 0x1;
const UIDGID: u32 = 0x2;
const PERMISSIONS: u32 = 0x4;
const ACMODTIME: u32 = 0x8;
const EXTENDED_ATTR: u32 = 0x8000_0000;

/// `ATTRS`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Attrs {
    /// Size in bytes.
    pub size: Option<u64>,
    /// Owner and group IDs.
    pub uid_gid: Option<(u32, u32)>,
    /// `st_mode`: type and permission bits.
    pub permissions: Option<u32>,
    /// Access and modification times, Unix seconds.
    pub times: Option<(u32, u32)>,
}

impl Attrs {
    /// Encode.
    pub fn encode(&self, w: &mut Writer) {
        let mut flags = 0;
        if self.size.is_some() {
            flags |= SIZE;
        }
        if self.uid_gid.is_some() {
            flags |= UIDGID;
        }
        if self.permissions.is_some() {
            flags |= PERMISSIONS;
        }
        if self.times.is_some() {
            flags |= ACMODTIME;
        }
        w.u32(flags);
        if let Some(s) = self.size {
            w.u64(s);
        }
        if let Some((u, g)) = self.uid_gid {
            w.u32(u).u32(g);
        }
        if let Some(p) = self.permissions {
            w.u32(p);
        }
        if let Some((a, m)) = self.times {
            w.u32(a).u32(m);
        }
    }

    /// Decode (extended pairs read and dropped).
    pub fn decode(r: &mut Reader<'_>) -> WireResult<Self> {
        let flags = r.u32()?;
        let mut a = Self::default();
        if flags & SIZE != 0 {
            a.size = Some(r.u64()?);
        }
        if flags & UIDGID != 0 {
            a.uid_gid = Some((r.u32()?, r.u32()?));
        }
        if flags & PERMISSIONS != 0 {
            a.permissions = Some(r.u32()?);
        }
        if flags & ACMODTIME != 0 {
            a.times = Some((r.u32()?, r.u32()?));
        }
        if flags & EXTENDED_ATTR != 0 {
            let n = r.u32()?;
            for _ in 0..n {
                r.bytes()?;
                r.bytes()?;
            }
        }
        Ok(a)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attrs_round_trip() {
        let a = Attrs {
            size: Some(5),
            uid_gid: Some((1, 2)),
            permissions: Some(0o100_644),
            times: Some((3, 4)),
        };
        let mut w = Writer::new();
        a.encode(&mut w);
        let b = w.finish();
        assert_eq!(Attrs::decode(&mut Reader::new(&b)).unwrap(), a);
    }
}

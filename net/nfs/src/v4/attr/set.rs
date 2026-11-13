// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Decoding the attributes a client sends: `SETATTR`, `CREATE` and
//! `OPEN` (what may be set), `VERIFY` and `NVERIFY` (anything
//! supported). An attribute this server does not support is
//! `NFS4ERR_ATTRNOTSUPP`; one it supports but a client may not set is
//! `NFS4ERR_INVAL`; values that do not decode, or bytes left over, are
//! `NFS4ERR_BADXDR`.

use super::{Bits, WRITABLE, WRITE_ONLY, supported};
use crate::server::vfs::{SetTime, Time};
use crate::types::nfs4::{self as t, bitmap4, fattr4, nfsstat4, nfstime4, settime4};
use crate::xdr::{Decoder, Xdr};

/// The values to set.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SetVals {
    /// `size`.
    pub size: Option<u64>,
    /// `mode`.
    pub mode: Option<u32>,
    /// `owner`, as sent.
    pub owner: Option<Vec<u8>>,
    /// `owner_group`, as sent.
    pub group: Option<Vec<u8>>,
    /// `time_access_set`.
    pub atime: SetTime,
    /// `time_modify_set`.
    pub mtime: SetTime,
}

/// The bits of `f` checked against what minor version `minor` supports.
///
/// # Errors
///
/// `NFS4ERR_ATTRNOTSUPP` for a bit not supported.
pub fn check_supported(f: &fattr4, minor: u32) -> Result<Bits, nfsstat4> {
    let b = Bits::from(&f.attrmask);
    if !b.minus(&supported(minor)).is_empty() {
        return Err(nfsstat4::NFS4ERR_ATTRNOTSUPP);
    }
    Ok(b)
}

fn bad<E>(_: E) -> nfsstat4 {
    nfsstat4::NFS4ERR_BADXDR
}

/// A time as set: a client time must have nanoseconds below a second.
fn settime(d: &mut Decoder<'_>) -> Result<SetTime, nfsstat4> {
    match settime4::decode(d).map_err(bad)? {
        settime4::SET_TO_CLIENT_TIME4(nfstime4 { seconds, nseconds }) => {
            if nseconds >= 1_000_000_000 {
                return Err(nfsstat4::NFS4ERR_INVAL);
            }
            Ok(SetTime::Client(Time {
                secs: seconds,
                nsecs: nseconds,
            }))
        }
        settime4::default(_) => Ok(SetTime::Server),
    }
}

/// The values of a set (`SETATTR`, `CREATE`, `OPEN`).
///
/// # Errors
///
/// `NFS4ERR_ATTRNOTSUPP`, `NFS4ERR_INVAL`, `NFS4ERR_BADXDR`.
pub fn decode_set(f: &fattr4, minor: u32) -> Result<(SetVals, Bits), nfsstat4> {
    let b = check_supported(f, minor)?;
    if !b.minus(&Bits::of(WRITABLE)).is_empty() {
        return Err(nfsstat4::NFS4ERR_INVAL);
    }
    let mut d = Decoder::new(&f.attr_vals.0);
    let mut s = SetVals::default();
    for n in b.iter() {
        match n {
            t::FATTR4_SIZE => s.size = Some(d.get_u64().map_err(bad)?),
            t::FATTR4_MODE => s.mode = Some(d.get_u32().map_err(bad)?),
            t::FATTR4_OWNER => s.owner = Some(d.get_opaque(u32::MAX).map_err(bad)?),
            t::FATTR4_OWNER_GROUP => s.group = Some(d.get_opaque(u32::MAX).map_err(bad)?),
            t::FATTR4_TIME_ACCESS_SET => s.atime = settime(&mut d)?,
            t::FATTR4_TIME_MODIFY_SET => s.mtime = settime(&mut d)?,
            _ => return Err(nfsstat4::NFS4ERR_INVAL),
        }
    }
    if !d.is_empty() {
        return Err(nfsstat4::NFS4ERR_BADXDR);
    }
    Ok((s, b))
}

/// Steps over one value of attribute `n`.
fn skip(n: u32, d: &mut Decoder<'_>) -> Result<(), nfsstat4> {
    match n {
        t::FATTR4_SUPPORTED_ATTRS | t::FATTR4_SUPPATTR_EXCLCREAT => {
            bitmap4::decode(d).map_err(bad)?;
        }
        t::FATTR4_TYPE
        | t::FATTR4_FH_EXPIRE_TYPE
        | t::FATTR4_LINK_SUPPORT
        | t::FATTR4_SYMLINK_SUPPORT
        | t::FATTR4_NAMED_ATTR
        | t::FATTR4_UNIQUE_HANDLES
        | t::FATTR4_LEASE_TIME
        | t::FATTR4_RDATTR_ERROR
        | t::FATTR4_ACLSUPPORT
        | t::FATTR4_CANSETTIME
        | t::FATTR4_CASE_INSENSITIVE
        | t::FATTR4_CASE_PRESERVING
        | t::FATTR4_CHOWN_RESTRICTED
        | t::FATTR4_HOMOGENEOUS
        | t::FATTR4_MAXLINK
        | t::FATTR4_MAXNAME
        | t::FATTR4_MODE
        | t::FATTR4_NO_TRUNC
        | t::FATTR4_NUMLINKS => {
            d.get_u32().map_err(bad)?;
        }
        t::FATTR4_CHANGE
        | t::FATTR4_SIZE
        | t::FATTR4_FILEID
        | t::FATTR4_FILES_AVAIL
        | t::FATTR4_FILES_FREE
        | t::FATTR4_FILES_TOTAL
        | t::FATTR4_MAXFILESIZE
        | t::FATTR4_MAXREAD
        | t::FATTR4_MAXWRITE
        | t::FATTR4_SPACE_AVAIL
        | t::FATTR4_SPACE_FREE
        | t::FATTR4_SPACE_TOTAL
        | t::FATTR4_SPACE_USED
        | t::FATTR4_MOUNTED_ON_FILEID => {
            d.get_u64().map_err(bad)?;
        }
        t::FATTR4_FSID => {
            d.get_u64().map_err(bad)?;
            d.get_u64().map_err(bad)?;
        }
        t::FATTR4_RAWDEV => {
            d.get_u32().map_err(bad)?;
            d.get_u32().map_err(bad)?;
        }
        t::FATTR4_FILEHANDLE => {
            d.get_opaque(t::NFS4_FHSIZE).map_err(bad)?;
        }
        t::FATTR4_OWNER | t::FATTR4_OWNER_GROUP => {
            d.get_opaque(u32::MAX).map_err(bad)?;
        }
        t::FATTR4_TIME_ACCESS
        | t::FATTR4_TIME_CREATE
        | t::FATTR4_TIME_DELTA
        | t::FATTR4_TIME_METADATA
        | t::FATTR4_TIME_MODIFY => {
            nfstime4::decode(d).map_err(bad)?;
        }
        _ => return Err(nfsstat4::NFS4ERR_ATTRNOTSUPP),
    }
    Ok(())
}

/// The bits of a `VERIFY` or `NVERIFY`, its values checked to decode.
///
/// # Errors
///
/// `NFS4ERR_ATTRNOTSUPP`; `NFS4ERR_INVAL` for `rdattr_error` and the
/// write-only times; `NFS4ERR_BADXDR`.
pub fn decode_verify(f: &fattr4, minor: u32) -> Result<Bits, nfsstat4> {
    let b = check_supported(f, minor)?;
    if b.has(t::FATTR4_RDATTR_ERROR) || WRITE_ONLY.iter().any(|n| b.has(*n)) {
        return Err(nfsstat4::NFS4ERR_INVAL);
    }
    let mut d = Decoder::new(&f.attr_vals.0);
    for n in b.iter() {
        skip(n, &mut d)?;
    }
    if !d.is_empty() {
        return Err(nfsstat4::NFS4ERR_BADXDR);
    }
    Ok(b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::nfs4::{attrlist4, time_how4};
    use crate::xdr::Encoder;

    fn fattr(bits: &[u32], vals: impl FnOnce(&mut Encoder)) -> fattr4 {
        let mut e = Encoder::new();
        vals(&mut e);
        fattr4 {
            attrmask: Bits::of(bits).to_bitmap(),
            attr_vals: attrlist4(e.into_bytes()),
        }
    }

    #[test]
    fn sets_decode_in_bit_order() {
        let f = fattr(
            &[t::FATTR4_SIZE, t::FATTR4_MODE, t::FATTR4_TIME_MODIFY_SET],
            |e| {
                e.put_u64(10);
                e.put_u32(0o640);
                settime4::default(time_how4::SET_TO_SERVER_TIME4)
                    .encode(e)
                    .unwrap();
            },
        );
        let (s, b) = decode_set(&f, 0).unwrap();
        assert_eq!(s.size, Some(10));
        assert_eq!(s.mode, Some(0o640));
        assert_eq!(s.mtime, SetTime::Server);
        assert!(b.has(t::FATTR4_MODE));
    }

    #[test]
    fn refusals() {
        let f = fattr(&[t::FATTR4_ACL], |_| {});
        assert_eq!(decode_set(&f, 0), Err(nfsstat4::NFS4ERR_ATTRNOTSUPP));
        let f = fattr(&[t::FATTR4_TYPE], |e| e.put_u32(1));
        assert_eq!(decode_set(&f, 0), Err(nfsstat4::NFS4ERR_INVAL));
        let f = fattr(&[t::FATTR4_SIZE], |e| e.put_u32(1));
        assert_eq!(decode_set(&f, 0), Err(nfsstat4::NFS4ERR_BADXDR));
        let f = fattr(&[t::FATTR4_SIZE], |e| {
            e.put_u64(1);
            e.put_u32(0);
        });
        assert_eq!(decode_set(&f, 0), Err(nfsstat4::NFS4ERR_BADXDR));
        let f = fattr(&[t::FATTR4_SUPPATTR_EXCLCREAT], |_| {});
        assert_eq!(decode_verify(&f, 0), Err(nfsstat4::NFS4ERR_ATTRNOTSUPP));
        let f = fattr(&[t::FATTR4_TIME_ACCESS_SET], |_| {});
        assert_eq!(decode_verify(&f, 0), Err(nfsstat4::NFS4ERR_INVAL));
        let f = fattr(&[t::FATTR4_SIZE, t::FATTR4_FSID], |e| {
            e.put_u64(1);
            e.put_u64(2);
            e.put_u64(3);
        });
        assert_eq!(decode_verify(&f, 0).unwrap().iter().count(), 2);
    }
}

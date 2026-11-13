// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! NFS version 4 attributes (RFC 7530 section 5, RFC 8881 section 5): the
//! `bitmap4` and the values of the attributes this client asks for, in
//! attribute-number order inside `attrlist4`.

use super::xdr::{R, W};
use crate::Error;

/// Attribute numbers.
pub mod a {
    /// `supported_attrs`.
    pub const SUPPORTED_ATTRS: u32 = 0;
    /// `type`.
    pub const TYPE: u32 = 1;
    /// `fh_expire_type`.
    pub const FH_EXPIRE_TYPE: u32 = 2;
    /// `change`.
    pub const CHANGE: u32 = 3;
    /// `size`.
    pub const SIZE: u32 = 4;
    /// `link_support`.
    pub const LINK_SUPPORT: u32 = 5;
    /// `symlink_support`.
    pub const SYMLINK_SUPPORT: u32 = 6;
    /// `named_attr`.
    pub const NAMED_ATTR: u32 = 7;
    /// `fsid`.
    pub const FSID: u32 = 8;
    /// `unique_handles`.
    pub const UNIQUE_HANDLES: u32 = 9;
    /// `lease_time`.
    pub const LEASE_TIME: u32 = 10;
    /// `rdattr_error`.
    pub const RDATTR_ERROR: u32 = 11;
    /// `acl`.
    pub const ACL: u32 = 12;
    /// `filehandle`.
    pub const FILEHANDLE: u32 = 19;
    /// `fileid`.
    pub const FILEID: u32 = 20;
    /// `mode`.
    pub const MODE: u32 = 33;
    /// `numlinks`.
    pub const NUMLINKS: u32 = 35;
    /// `owner`.
    pub const OWNER: u32 = 36;
    /// `owner_group`.
    pub const OWNER_GROUP: u32 = 37;
    /// `time_modify`.
    pub const TIME_MODIFY: u32 = 53;
}

/// The attributes RFC 7530 section 5.6 calls required (all but `supported_attrs` decoded too).
pub const REQUIRED: &[u32] = &[
    a::SUPPORTED_ATTRS,
    a::TYPE,
    a::FH_EXPIRE_TYPE,
    a::CHANGE,
    a::SIZE,
    a::LINK_SUPPORT,
    a::SYMLINK_SUPPORT,
    a::NAMED_ATTR,
    a::FSID,
    a::UNIQUE_HANDLES,
    a::LEASE_TIME,
    a::RDATTR_ERROR,
    a::FILEHANDLE,
];

/// A `bitmap4` from attribute numbers.
pub fn bitmap(attrs: &[u32]) -> Vec<u32> {
    let words = attrs.iter().map(|n| n / 32 + 1).max().unwrap_or(0) as usize;
    let mut v = vec![0u32; words];
    for n in attrs {
        v[(n / 32) as usize] |= 1 << (n % 32);
    }
    v
}

/// The attribute numbers of a `bitmap4`.
pub fn numbers(bm: &[u32]) -> Vec<u32> {
    let mut out = Vec::new();
    for (i, w) in bm.iter().enumerate() {
        for b in 0..32 {
            if w & (1 << b) != 0 {
                out.push(i as u32 * 32 + b);
            }
        }
    }
    out
}

/// Writes a `bitmap4`.
pub fn put_bitmap(w: &mut W, bm: &[u32]) {
    w.u32(bm.len() as u32);
    for x in bm {
        w.u32(*x);
    }
}

/// Reads a `bitmap4`.
pub fn get_bitmap(r: &mut R<'_>) -> Result<Vec<u32>, Error> {
    let n = r.count(8)?;
    (0..n).map(|_| r.u32()).collect()
}

/// The values this client decodes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Attrs {
    /// The attribute numbers present.
    pub present: Vec<u32>,
    /// `supported_attrs`.
    pub supported: Option<Vec<u32>>,
    /// `type` (`nfs_ftype4`: 1 regular, 2 directory).
    pub ftype: Option<u32>,
    /// `fh_expire_type`.
    pub fh_expire_type: Option<u32>,
    /// `change`.
    pub change: Option<u64>,
    /// `size`.
    pub size: Option<u64>,
    /// `link_support`.
    pub link_support: Option<bool>,
    /// `symlink_support`.
    pub symlink_support: Option<bool>,
    /// `named_attr`.
    pub named_attr: Option<bool>,
    /// `fsid` (major, minor).
    pub fsid: Option<(u64, u64)>,
    /// `unique_handles`.
    pub unique_handles: Option<bool>,
    /// `lease_time`.
    pub lease_time: Option<u32>,
    /// `rdattr_error`.
    pub rdattr_error: Option<u32>,
    /// `filehandle`.
    pub filehandle: Option<Vec<u8>>,
    /// `fileid`.
    pub fileid: Option<u64>,
    /// `mode`.
    pub mode: Option<u32>,
    /// `numlinks`.
    pub numlinks: Option<u32>,
    /// `owner`.
    pub owner: Option<String>,
    /// `owner_group`.
    pub owner_group: Option<String>,
    /// `time_modify` (seconds, nanoseconds).
    pub time_modify: Option<(i64, u32)>,
}

/// Decodes a `fattr4` (bitmap, then the values' opaque).
pub fn get_fattr(r: &mut R<'_>) -> Result<Attrs, Error> {
    let bm = get_bitmap(r)?;
    let vals = r.opaque(1 << 20)?;
    decode_values(&numbers(&bm), vals)
}

/// Decodes the values of `present` from `vals`.
pub fn decode_values(present: &[u32], vals: &[u8]) -> Result<Attrs, Error> {
    let mut r = R::new(vals);
    let mut x = Attrs {
        present: present.to_vec(),
        ..Attrs::default()
    };
    let utf8 = |r: &mut R<'_>| -> Result<String, Error> {
        Ok(String::from_utf8_lossy(r.opaque(1024)?).into_owned())
    };
    for n in present {
        match *n {
            a::SUPPORTED_ATTRS => x.supported = Some(numbers(&get_bitmap(&mut r)?)),
            a::TYPE => x.ftype = Some(r.u32()?),
            a::FH_EXPIRE_TYPE => x.fh_expire_type = Some(r.u32()?),
            a::CHANGE => x.change = Some(r.u64()?),
            a::SIZE => x.size = Some(r.u64()?),
            a::LINK_SUPPORT => x.link_support = Some(r.bool()?),
            a::SYMLINK_SUPPORT => x.symlink_support = Some(r.bool()?),
            a::NAMED_ATTR => x.named_attr = Some(r.bool()?),
            a::FSID => x.fsid = Some((r.u64()?, r.u64()?)),
            a::UNIQUE_HANDLES => x.unique_handles = Some(r.bool()?),
            a::LEASE_TIME => x.lease_time = Some(r.u32()?),
            a::RDATTR_ERROR => x.rdattr_error = Some(r.u32()?),
            a::FILEHANDLE => x.filehandle = Some(r.opaque(128)?.to_vec()),
            a::FILEID => x.fileid = Some(r.u64()?),
            a::MODE => x.mode = Some(r.u32()?),
            a::NUMLINKS => x.numlinks = Some(r.u32()?),
            a::OWNER => x.owner = Some(utf8(&mut r)?),
            a::OWNER_GROUP => x.owner_group = Some(utf8(&mut r)?),
            a::TIME_MODIFY => x.time_modify = Some((r.u64()?.cast_signed(), r.u32()?)),
            other => {
                return Err(Error::Decode(format!(
                    "attribute {other} came back but was not asked for"
                )));
            }
        }
    }
    r.end()?;
    Ok(x)
}

/// A `fattr4` setting `mode` and/or `size`.
pub fn put_fattr_set(w: &mut W, mode: Option<u32>, size: Option<u64>) {
    let mut nums = Vec::new();
    let mut vals = W::new();
    if let Some(s) = size {
        nums.push(a::SIZE);
        vals.u64(s);
    }
    if let Some(m) = mode {
        nums.push(a::MODE);
        vals.u32(m);
    }
    put_bitmap(w, &bitmap(&nums));
    w.opaque(&vals.done());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bitmaps_and_values() {
        let bm = bitmap(&[a::TYPE, a::SIZE, a::MODE]);
        assert_eq!(bm, vec![0b10010, 0b10]);
        assert_eq!(numbers(&bm), vec![1, 4, 33]);
        let vals = W::new().u32(2).u64(77).u32(0o755).done();
        let x = decode_values(&[a::TYPE, a::SIZE, a::MODE], &vals).unwrap();
        assert_eq!((x.ftype, x.size, x.mode), (Some(2), Some(77), Some(0o755)));
        assert!(decode_values(&[a::ACL], &vals).is_err());
        assert!(decode_values(&[a::TYPE], &vals).is_err());
    }
}

// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The program's storage: fat pointers, big-endian integers, strings in
//! the job's CCSID or UTF-8, and the images of the structures the API
//! hands back (`berval`, `LDAPControl`, string arrays), laid out as the
//! QS/400 headers declare them.

use mi::{FatPtr, Job};

/// Longest string read from the program (1 MiB).
pub const MAX_STR: usize = 1 << 20;
/// Most elements read from a NULL-terminated array.
pub const MAX_ARRAY: usize = 1 << 16;

/// Size of a pointer.
pub const PTR: usize = 16;
/// `struct berval`: `bv_len` (4 bytes), then `bv_val` at 16.
pub const BERVAL: usize = 32;
/// `LDAPControl`: `ldctl_oid` at 0, `ldctl_value` at 16, `ldctl_iscritical` at 48.
pub const CONTROL: usize = 64;
/// `LDAPMod`: `mod_op` at 0, `mod_type` at 16, `mod_vals` at 32.
pub const MOD: usize = 48;
/// `LDAPSortKey`: `attributeType` at 0, `orderingRule` at 16, `reverseOrder` at 32.
pub const SORTKEY: usize = 48;

/// A pointer argument.
#[must_use]
pub fn fat(v: u128) -> FatPtr {
    FatPtr::from_bytes(&v.to_le_bytes())
}

/// A pointer result.
#[must_use]
pub fn raw(p: FatPtr) -> u128 {
    u128::from_le_bytes(p.to_bytes())
}

/// Whether a pointer argument is null.
#[must_use]
pub fn is_null(p: u128) -> bool {
    fat(p).is_null()
}

/// The low 32 bits of an integer argument.
#[must_use]
#[allow(clippy::cast_possible_truncation)]
pub fn int(v: i64) -> i32 {
    v as i32
}

/// The low 32 bits, unsigned.
#[must_use]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub fn uint(v: i64) -> u32 {
    v as u32
}

/// `len` bytes behind a pointer; none for null, dead or short.
pub fn bytes(j: &Job, p: u128, len: usize) -> Option<&'static mut [u8]> {
    let (at, avail) = j.spaces.extent(fat(p)).ok()?;
    if (len as u64) > avail {
        return None;
    }
    Some(mi::storage::view(at, len))
}

/// The native address a pointer reaches.
pub fn native(j: &Job, p: u128) -> Option<u64> {
    if is_null(p) {
        return None;
    }
    j.spaces.deref(fat(p)).ok()
}

/// The bytes of a NUL-terminated string.
pub fn cbytes(j: &Job, p: u128) -> Option<Vec<u8>> {
    if is_null(p) {
        return None;
    }
    let (at, avail) = j.spaces.extent(fat(p)).ok()?;
    let n = usize::try_from(avail).unwrap_or(usize::MAX).min(MAX_STR);
    let b = mi::storage::view(at, n);
    let end = b.iter().position(|&c| c == 0).unwrap_or(n);
    Some(b[..end].to_vec())
}

/// A big-endian `int` behind a pointer.
pub fn get_i32(j: &Job, p: u128) -> Option<i32> {
    let b = bytes(j, p, 4)?;
    Some(i32::from_be_bytes([b[0], b[1], b[2], b[3]]))
}

/// Store a big-endian `int`; false if the pointer is null or short.
pub fn put_i32(j: &Job, p: u128, v: i32) -> bool {
    match bytes(j, p, 4) {
        Some(b) => {
            b.copy_from_slice(&v.to_be_bytes());
            true
        }
        None => false,
    }
}

/// Store a big-endian `unsigned long` (4 bytes).
pub fn put_u32(j: &Job, p: u128, v: u32) -> bool {
    match bytes(j, p, 4) {
        Some(b) => {
            b.copy_from_slice(&v.to_be_bytes());
            true
        }
        None => false,
    }
}

/// A big-endian `unsigned long` behind a pointer.
pub fn get_u32(j: &Job, p: u128) -> Option<u32> {
    let b = bytes(j, p, 4)?;
    Some(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
}

/// The pointer stored behind a pointer.
pub fn get_ptr(j: &Job, p: u128) -> Option<u128> {
    let b = bytes(j, p, PTR)?;
    Some(u128::from_le_bytes(b.try_into().ok()?))
}

/// Store a pointer; false if the target is null or short.
pub fn put_ptr(j: &Job, p: u128, v: FatPtr) -> bool {
    match bytes(j, p, PTR) {
        Some(b) => {
            b.copy_from_slice(&v.to_bytes());
            true
        }
        None => false,
    }
}

/// The pointer `off` bytes past `p`.
pub fn at(j: &Job, p: u128, off: usize) -> u128 {
    j.spaces
        .add(fat(p), i64::try_from(off).unwrap_or(0))
        .map_or(0, raw)
}

/// The pointers of a NULL-terminated array (none for a null array).
pub fn ptr_array(j: &Job, p: u128) -> Vec<u128> {
    let mut out = Vec::new();
    if is_null(p) {
        return out;
    }
    for i in 0..MAX_ARRAY {
        let Some(e) = get_ptr(j, at(j, p, i * PTR)) else {
            break;
        };
        if is_null(e) {
            break;
        }
        out.push(e);
    }
    out
}

/// A `struct berval`'s bytes; none for a null pointer.
pub fn berval(j: &Job, p: u128) -> Option<Vec<u8>> {
    let b = bytes(j, p, BERVAL)?;
    let len = u32::from_be_bytes([b[0], b[1], b[2], b[3]]) as usize;
    let val = u128::from_le_bytes(b[16..32].try_into().ok()?);
    if len == 0 {
        return Some(Vec::new());
    }
    bytes(j, val, len.min(1 << 24)).map(|v| v.to_vec())
}

/// How strings cross: in the job's CCSID, or UTF-8 (`LDAP_OPT_UTF8_IO`
/// and the `_utf8` functions).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Codec {
    /// The local CCSID.
    pub ccsid: u16,
    /// UTF-8 at the interface.
    pub utf8: bool,
}

impl Codec {
    /// The program's bytes as text.
    #[must_use]
    pub fn text(self, b: &[u8]) -> String {
        if self.utf8 || self.ccsid == 1208 {
            return String::from_utf8_lossy(b).into_owned();
        }
        ccsid::conv::to_string(self.ccsid, b).unwrap_or_else(|_| ccsid::from_ebcdic(b))
    }

    /// Text as the program's bytes.
    #[must_use]
    pub fn bytes(self, s: &str) -> Vec<u8> {
        if self.utf8 || self.ccsid == 1208 {
            return s.as_bytes().to_vec();
        }
        ccsid::conv::from_str(self.ccsid, s)
            .unwrap_or_else(|_| ccsid::to_ebcdic(s).unwrap_or_default())
    }

    /// A NUL-terminated string argument as text.
    pub fn arg(self, j: &Job, p: u128) -> Option<String> {
        cbytes(j, p).map(|b| self.text(&b))
    }

    /// The strings of a NULL-terminated `char **`.
    pub fn strings(self, j: &Job, p: u128) -> Vec<String> {
        ptr_array(j, p)
            .into_iter()
            .filter_map(|e| self.arg(j, e))
            .collect()
    }
}

/// Bytes being laid out for the program, with the pointer slots to fill
/// once the block's address is known.
#[derive(Debug, Default, Clone)]
pub struct Image {
    /// The bytes.
    pub bytes: Vec<u8>,
    /// `(slot, target)`: a pointer at `slot` to offset `target`.
    pub ptrs: Vec<(usize, usize)>,
}

impl Image {
    /// An image of `n` zero bytes.
    #[must_use]
    pub fn new(n: usize) -> Self {
        Self {
            bytes: vec![0; n],
            ptrs: Vec::new(),
        }
    }

    /// Room for `n` zero bytes at a 16-byte boundary; its offset.
    pub fn reserve(&mut self, n: usize) -> usize {
        let off = self.bytes.len().next_multiple_of(16);
        self.bytes.resize(off + n.max(1), 0);
        off
    }

    /// Bytes and a NUL; their offset.
    pub fn cstr(&mut self, b: &[u8]) -> usize {
        let off = self.bytes.len();
        self.bytes.extend_from_slice(b);
        self.bytes.push(0);
        off
    }

    /// Raw bytes (no NUL) at a 16-byte boundary; their offset.
    pub fn data(&mut self, b: &[u8]) -> usize {
        let off = self.reserve(b.len());
        self.bytes[off..off + b.len()].copy_from_slice(b);
        off
    }

    /// A pointer at `slot` to `target`.
    pub fn ptr(&mut self, slot: usize, target: usize) {
        self.ptrs.push((slot, target));
    }

    /// A big-endian `int` at `off`.
    pub fn i32(&mut self, off: usize, v: i32) {
        self.bytes[off..off + 4].copy_from_slice(&v.to_be_bytes());
    }

    /// A big-endian `unsigned long` at `off`.
    pub fn u32(&mut self, off: usize, v: u32) {
        self.bytes[off..off + 4].copy_from_slice(&v.to_be_bytes());
    }

    /// A `berval` at `off` over `data`.
    pub fn berval_at(&mut self, off: usize, data: &[u8]) {
        self.u32(off, u32::try_from(data.len()).unwrap_or(u32::MAX));
        let d = self.data(data);
        self.ptr(off + 16, d);
    }

    /// A NULL-terminated array of strings; the array's offset.
    pub fn strings(&mut self, items: &[Vec<u8>]) -> usize {
        let arr = self.reserve((items.len() + 1) * PTR);
        for (i, s) in items.iter().enumerate() {
            let o = self.cstr(s);
            self.ptr(arr + i * PTR, o);
        }
        arr
    }
}

/// An image of one `berval`.
#[must_use]
pub fn berval_image(data: &[u8]) -> Image {
    let mut im = Image::new(BERVAL);
    im.berval_at(0, data);
    im
}

/// An image of one `LDAPControl` with its OID and value.
#[must_use]
pub fn control_image(c: &ldap::control::Control, codec: Codec) -> Image {
    let mut im = Image::new(CONTROL);
    let oid = im.cstr(&codec.bytes(&c.oid));
    im.ptr(0, oid);
    if let Some(v) = &c.value {
        im.berval_at(16, v);
    }
    im.bytes[48] = u8::from(c.critical);
    im
}

/// A program's `LDAPControl`.
pub fn read_control(j: &Job, p: u128, codec: Codec) -> Option<ldap::control::Control> {
    let b = bytes(j, p, CONTROL)?;
    let oid_p = u128::from_le_bytes(b[0..16].try_into().ok()?);
    let crit = b[48] != 0;
    let vlen = u32::from_be_bytes([b[16], b[17], b[18], b[19]]) as usize;
    let vptr = u128::from_le_bytes(b[32..48].try_into().ok()?);
    let raw_oid = cbytes(j, oid_p)?;
    // An OID is digits and dots: one in ASCII is read as it is.
    let oid = if raw_oid.iter().all(|c| c.is_ascii_digit() || *c == b'.') {
        String::from_utf8_lossy(&raw_oid).into_owned()
    } else {
        codec.text(&raw_oid)
    };
    let value = if is_null(vptr) {
        None
    } else {
        Some(bytes(j, vptr, vlen.min(1 << 24))?.to_vec())
    };
    Some(ldap::control::Control {
        oid,
        critical: crit,
        value,
    })
}

/// A program's NULL-terminated `LDAPControl **`.
pub fn read_controls(j: &Job, p: u128, codec: Codec) -> Vec<ldap::control::Control> {
    ptr_array(j, p)
        .into_iter()
        .filter_map(|c| read_control(j, c, codec))
        .collect()
}

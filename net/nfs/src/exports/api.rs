// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The entries `QZNFRTVE` returns (`QSYSINC/H(QZNFRTVE)`): `EXPE0100`,
//! every attribute of an export, and `EXPE0200`, its path alone. The
//! numbers are big-endian; each host entry starts on a 4-byte boundary.
//!
//! Where the header leaves a value open QS/400 fills it so (docs/NFS.md
//! section 8, `NEEDS-CODY` in section 12): the version flag has bit 1
//! for version 2, 2 for 3 and 4 for 4; the write mode flag is 1 for
//! `*SYNC` and 0 for `*ASYNC`; the export name follows the path, and
//! the security flavors follow it from the next 4-byte boundary.

use super::model::{Export, HostOpt, HostSpec};
use super::options::print_hosts;

/// The fixed part of an `EXPE0100` entry.
pub const EXPE0100_FIXED: usize = 102;
/// The fixed part of an `EXPE0200` entry.
pub const EXPE0200_FIXED: usize = 16;

/// What the service resolves for an entry before it is encoded.
pub struct Resolved<'a> {
    /// The export.
    pub export: &'a Export,
    /// The anonymous UID (`ANON=-1` is `0xFFFFFFFF`).
    pub anon_uid: u32,
    /// The profile that UID is (`*NONE` when refused or unknown).
    pub anon_profile: &'a str,
    /// The CCSID the path, the export name and the host names are in.
    pub ccsid: u32,
    /// Text in that CCSID.
    pub encode: &'a dyn Fn(&str) -> Vec<u8>,
    /// A host option's data and path name CCSIDs as numbers.
    pub host_ccsids: &'a dyn Fn(&HostOpt) -> (u32, u32),
}

fn put(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_be_bytes());
}

fn set(out: &mut [u8], at: usize, v: usize) {
    let v = u32::try_from(v).unwrap_or(u32::MAX);
    out[at..at + 4].copy_from_slice(&v.to_be_bytes());
}

fn align(out: &mut Vec<u8>) {
    while !out.len().is_multiple_of(4) {
        out.push(0);
    }
}

fn len32(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// An `EXPE0100` entry.
#[must_use]
#[allow(clippy::many_single_char_names)] // the header's parts
pub fn expe0100(r: &Resolved<'_>) -> Vec<u8> {
    let e = r.export;
    let o = &e.options;
    let mut out = vec![0u8; EXPE0100_FIXED];
    let path = (r.encode)(&e.path);
    let rw: Vec<HostSpec> = o.rw.clone().unwrap_or_default();
    let access: Vec<HostSpec> = o.access.clone().unwrap_or_default();
    // The fixed part, in the header's order.
    set(&mut out, 4, EXPE0100_FIXED);
    set(&mut out, 8, path.len());
    set(&mut out, 12, r.ccsid as usize);
    set(&mut out, 16, usize::from(o.ro));
    set(&mut out, 20, usize::from(o.nosuid));
    out[56..60].copy_from_slice(&r.anon_uid.to_be_bytes());
    let mut name = (r.encode)(&format!("{:<10}", r.anon_profile));
    name.resize(10, (r.encode)(" ").first().copied().unwrap_or(0x40));
    out[60..70].copy_from_slice(&name[..10]);
    let v = o.versions();
    set(&mut out, 70, usize::from(v.v3) * 2 + usize::from(v.v4) * 4);
    set(&mut out, 82, usize::from(o.pseudo_root));
    set(&mut out, 86, usize::from(o.public));
    out.extend_from_slice(&path);
    if let Some(x) = &o.exname {
        let n = (r.encode)(x);
        let here = out.len();
        set(&mut out, 90, here);
        set(&mut out, 94, n.len());
        set(&mut out, 98, r.ccsid as usize);
        out.extend_from_slice(&n);
    }
    let flavors = o.flavors();
    align(&mut out);
    let here = out.len();
    set(&mut out, 74, here);
    set(&mut out, 78, flavors.len());
    for f in &flavors {
        put(&mut out, f.number());
    }
    let list = |out: &mut Vec<u8>, at: usize, hosts: &[HostSpec]| {
        align(out);
        if !hosts.is_empty() {
            let here = out.len();
            set(out, at, here);
        }
        set(out, at + 4, hosts.len());
        for h in hosts {
            let n = (r.encode)(&print_hosts(std::slice::from_ref(h)));
            let entry = (8 + n.len()).div_ceil(4) * 4;
            put(out, len32(entry));
            put(out, len32(n.len()));
            out.extend_from_slice(&n);
            align(out);
        }
    };
    list(&mut out, 24, &rw);
    list(&mut out, 32, &o.root_hosts);
    list(&mut out, 40, &access);
    align(&mut out);
    if !e.hosts.is_empty() {
        let here = out.len();
        set(&mut out, 48, here);
    }
    set(&mut out, 52, e.hosts.len());
    for h in &e.hosts {
        let n = (r.encode)(&print_hosts(std::slice::from_ref(&h.host)));
        let (data, path) = (r.host_ccsids)(h);
        let entry = (20 + n.len()).div_ceil(4) * 4;
        put(&mut out, len32(entry));
        put(&mut out, data);
        put(&mut out, path);
        put(&mut out, u32::from(h.sync));
        put(&mut out, len32(n.len()));
        out.extend_from_slice(&n);
        align(&mut out);
    }
    let total = out.len();
    set(&mut out, 0, total);
    out
}

/// An `EXPE0200` entry: its length, the path's displacement, length and
/// CCSID, and the path.
#[must_use]
pub fn expe0200(r: &Resolved<'_>) -> Vec<u8> {
    let path = (r.encode)(&r.export.path);
    let mut out = vec![0u8; EXPE0200_FIXED];
    set(&mut out, 4, EXPE0200_FIXED);
    set(&mut out, 8, path.len());
    set(&mut out, 12, r.ccsid as usize);
    out.extend_from_slice(&path);
    align(&mut out);
    let total = out.len();
    set(&mut out, 0, total);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exports::file;

    fn be(b: &[u8], at: usize) -> u32 {
        u32::from_be_bytes(b[at..at + 4].try_into().unwrap())
    }

    #[test]
    #[allow(clippy::many_single_char_names)]
    fn expe0100_layout() {
        let e = file::entry(
            1,
            "/home/joe -ro,rw=alpha,root=beta:gamma,access=alpha:beta:gamma,anon=-1,nosuid,vers=4,root,exname=/joe HOSTOPT alpha:819:850:*ASYNC",
        )
        .unwrap();
        let enc = |s: &str| s.as_bytes().to_vec();
        let cc = |_: &HostOpt| (819, 850);
        let r = Resolved {
            export: &e,
            anon_uid: u32::MAX,
            anon_profile: "*NONE",
            ccsid: 1208,
            encode: &enc,
            host_ccsids: &cc,
        };
        let b = expe0100(&r);
        assert_eq!(be(&b, 0) as usize, b.len());
        assert_eq!(b.len() % 4, 0);
        let disp = be(&b, 4) as usize;
        let n = be(&b, 8) as usize;
        assert_eq!(&b[disp..disp + n], b"/home/joe");
        assert_eq!(be(&b, 12), 1208);
        assert_eq!((be(&b, 16), be(&b, 20)), (1, 1));
        assert_eq!(be(&b, 28), 1);
        assert_eq!(be(&b, 36), 2);
        assert_eq!(be(&b, 44), 3);
        assert_eq!(be(&b, 52), 1);
        assert_eq!(be(&b, 56), u32::MAX);
        assert_eq!(&b[60..70], b"*NONE     ");
        assert_eq!(be(&b, 70), 4);
        assert_eq!((be(&b, 82), be(&b, 86)), (1, 0));
        let x = be(&b, 90) as usize;
        assert_eq!(&b[x..x + be(&b, 94) as usize], b"/joe");
        assert_eq!(be(&b, 98), 1208);
        // The flavors: one, AUTH_SYS.
        let f = be(&b, 74) as usize;
        assert_eq!((be(&b, 78), be(&b, f)), (1, 1));
        // The root list's second host.
        let mut at = be(&b, 32) as usize;
        at += be(&b, at) as usize;
        let len = be(&b, at + 4) as usize;
        assert_eq!(&b[at + 8..at + 8 + len], b"gamma");
        // The host option.
        let h = be(&b, 48) as usize;
        assert_eq!(
            (be(&b, h + 4), be(&b, h + 8), be(&b, h + 12)),
            (819, 850, 0)
        );
        assert_eq!(&b[h + 20..h + 25], b"alpha");
    }

    #[test]
    fn expe0200_layout() {
        let e = Export::new("/a/b");
        let enc = |s: &str| s.as_bytes().to_vec();
        let cc = |_: &HostOpt| (0, 0);
        let r = Resolved {
            export: &e,
            anon_uid: 0,
            anon_profile: "QNFSANON",
            ccsid: 819,
            encode: &enc,
            host_ccsids: &cc,
        };
        let b = expe0200(&r);
        assert_eq!(b.len(), 20);
        assert_eq!(
            (be(&b, 0), be(&b, 4), be(&b, 8), be(&b, 12)),
            (20, 16, 4, 819)
        );
        assert_eq!(&b[16..20], b"/a/b");
    }
}

// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The NFS options of `QZNFNFSO`, kept in
//! `/QIBM/UserData/OS400/NFS/options` (docs/NFS.md section 6.2): the
//! magic `QSNFSOPT`, a big-endian `u16` version, then the flavors (`u16`
//! count, each `u32`), the `NFSv4` domain (`u16` length, UTF-8), the most
//! server threads (`u32`), the client handle cache (`u32`) and the
//! UTF-8 check (`u8`, 0 or 1).

use stg::nfs::NfsOptions;

/// The file.
pub const PATH: &str = "/QIBM/UserData/OS400/NFS/options";
/// The first eight bytes.
pub const MAGIC: &[u8; 8] = b"QSNFSOPT";
/// The format's version.
pub const FORMAT_VERSION: u16 = 1;

/// The bytes of `o`.
#[must_use]
pub fn encode(o: &NfsOptions) -> Vec<u8> {
    let mut b = MAGIC.to_vec();
    b.extend_from_slice(&FORMAT_VERSION.to_be_bytes());
    let n = u16::try_from(o.flavors.len()).unwrap_or(u16::MAX);
    b.extend_from_slice(&n.to_be_bytes());
    for f in o.flavors.iter().take(usize::from(n)) {
        b.extend_from_slice(&f.to_be_bytes());
    }
    let d = o.domain.as_bytes();
    let d = &d[..d.len().min(usize::from(u16::MAX))];
    b.extend_from_slice(&u16::try_from(d.len()).unwrap_or(0).to_be_bytes());
    b.extend_from_slice(d);
    b.extend_from_slice(&o.max_threads.to_be_bytes());
    b.extend_from_slice(&o.handle_cache.to_be_bytes());
    b.push(u8::from(o.utf8_validation));
    b
}

/// The options in `b`; `None` when it is not this format.
#[must_use]
pub fn decode(b: &[u8]) -> Option<NfsOptions> {
    let mut r = Reader { b, at: 0 };
    if r.take(8)? != MAGIC || r.u16()? != FORMAT_VERSION {
        return None;
    }
    let n = r.u16()?;
    let mut flavors = Vec::new();
    for _ in 0..n {
        flavors.push(r.u32()?);
    }
    let len = r.u16()?;
    let domain = String::from_utf8(r.take(usize::from(len))?.to_vec()).ok()?;
    let max_threads = r.u32()?;
    let handle_cache = r.u32()?;
    let utf8_validation = match r.take(1)?[0] {
        0 => false,
        1 => true,
        _ => return None,
    };
    (r.at == b.len()).then_some(NfsOptions {
        flavors,
        domain,
        max_threads,
        handle_cache,
        utf8_validation,
    })
}

struct Reader<'a> {
    b: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn take(&mut self, n: usize) -> Option<&[u8]> {
        let s = self.b.get(self.at..self.at.checked_add(n)?)?;
        self.at += n;
        Some(s)
    }
    fn u16(&mut self) -> Option<u16> {
        Some(u16::from_be_bytes(self.take(2)?.try_into().ok()?))
    }
    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_be_bytes(self.take(4)?.try_into().ok()?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_refusals() {
        let o = NfsOptions {
            flavors: vec![1],
            domain: "lab.example".into(),
            max_threads: 16,
            handle_cache: 512,
            utf8_validation: false,
        };
        let b = encode(&o);
        assert_eq!(&b[..10], b"QSNFSOPT\x00\x01");
        assert_eq!(decode(&b), Some(o));
        assert_eq!(decode(&b[..b.len() - 1]), None);
        let mut bad = b.clone();
        bad[9] = 2;
        assert_eq!(decode(&bad), None);
        let mut long = b;
        long.push(0);
        assert_eq!(decode(&long), None);
        assert_eq!(
            decode(&encode(&NfsOptions::default())),
            Some(NfsOptions::default())
        );
    }
}

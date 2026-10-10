// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Deterministic newc (SVR4) cpio writer.

use std::fmt::Write as _;

const MAGIC: &str = "070701";
const TRAILER: &str = "TRAILER!!!";
const S_IFDIR: u32 = 0o040_000;
const S_IFREG: u32 = 0o100_000;
const S_IFCHR: u32 = 0o020_000;

/// What an entry is.
pub enum Kind {
    /// Directory.
    Dir,
    /// Regular file with contents.
    File(Vec<u8>),
    /// Character device.
    CharDev {
        /// Major number.
        major: u32,
        /// Minor number.
        minor: u32,
    },
}

/// One archive member. Paths are relative, no leading slash.
pub struct Entry {
    /// Archive path.
    pub name: String,
    /// Permission bits (type bits are added from `kind`).
    pub perm: u32,
    /// Entry kind.
    pub kind: Kind,
}

impl Entry {
    /// A 0755 directory.
    pub fn dir(name: &str) -> Self {
        Self {
            name: name.into(),
            perm: 0o755,
            kind: Kind::Dir,
        }
    }
    /// A 0755 regular file.
    pub fn file(name: &str, data: Vec<u8>) -> Self {
        Self {
            name: name.into(),
            perm: 0o755,
            kind: Kind::File(data),
        }
    }
    /// A 0600 character device.
    pub fn chardev(name: &str, major: u32, minor: u32) -> Self {
        Self {
            name: name.into(),
            perm: 0o600,
            kind: Kind::CharDev { major, minor },
        }
    }
}

/// Serialize entries as a newc archive. Entries are sorted by path, inodes
/// are sequential, uid/gid/mtime are zero. Same input, same bytes.
pub fn write_newc(mut entries: Vec<Entry>) -> Vec<u8> {
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    let mut out = Vec::new();
    for (i, e) in entries.iter().enumerate() {
        let ino = u32::try_from(i + 1).expect("fewer than 2^32 entries");
        let (mode, nlink, size, rmaj, rmin, data): (u32, u32, usize, u32, u32, &[u8]) =
            match &e.kind {
                Kind::Dir => (S_IFDIR | e.perm, 2, 0, 0, 0, &[]),
                Kind::File(d) => (S_IFREG | e.perm, 1, d.len(), 0, 0, d),
                Kind::CharDev { major, minor } => (S_IFCHR | e.perm, 1, 0, *major, *minor, &[]),
            };
        header(&mut out, ino, mode, nlink, size, rmaj, rmin, &e.name);
        out.extend_from_slice(data);
        pad4(&mut out);
    }
    header(&mut out, 0, 0, 1, 0, 0, 0, TRAILER);
    out
}

#[allow(clippy::too_many_arguments)]
fn header(
    out: &mut Vec<u8>,
    ino: u32,
    mode: u32,
    nlink: u32,
    size: usize,
    rmaj: u32,
    rmin: u32,
    name: &str,
) {
    let size = u32::try_from(size).expect("cpio member under 4 GiB");
    let namesize = u32::try_from(name.len() + 1).expect("cpio name under 4 GiB");
    let mut h = String::with_capacity(110);
    h.push_str(MAGIC);
    for field in [
        ino, mode, 0, 0, nlink, 0, size, 0, 0, rmaj, rmin, namesize, 0,
    ] {
        let _ = write!(h, "{field:08x}");
    }
    out.extend_from_slice(h.as_bytes());
    out.extend_from_slice(name.as_bytes());
    out.push(0);
    pad4(out);
}

fn pad4(out: &mut Vec<u8>) {
    while !out.len().is_multiple_of(4) {
        out.push(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_is_aligned_and_sorted() {
        let a = write_newc(vec![
            Entry::dir("sys"),
            Entry::file("init", b"hello".to_vec()),
            Entry::dir("dev"),
        ]);
        assert_eq!(a.len() % 4, 0);
        assert!(a.starts_with(b"070701"));
        let text = String::from_utf8_lossy(&a);
        let dev = text.find("dev\0").unwrap();
        let init = text.find("init\0").unwrap();
        let sys = text.find("sys\0").unwrap();
        assert!(dev < init && init < sys);
        assert!(text.ends_with("TRAILER!!!\0\0\0\0"));
    }

    #[test]
    fn header_fields() {
        let a = write_newc(vec![Entry::chardev("console", 5, 1)]);
        let h = std::str::from_utf8(&a[..110]).unwrap();
        assert_eq!(&h[6..14], "00000001"); // ino
        assert_eq!(&h[14..22], format!("{:08x}", 0o020_600)); // mode
        assert_eq!(&h[78..86], "00000005"); // rdev major
        assert_eq!(&h[86..94], "00000001"); // rdev minor
        assert_eq!(&h[94..102], "00000008"); // namesize incl. NUL
    }

    #[test]
    fn deterministic() {
        let mk = || write_newc(vec![Entry::dir("run"), Entry::file("init", vec![1, 2, 3])]);
        assert_eq!(mk(), mk());
    }
}

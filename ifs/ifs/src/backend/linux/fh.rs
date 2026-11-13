// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Nodes by xfs handle (`fhandle`). An xfs handle without a parent is the
//! inode number and the inode's generation: 12 bytes of type 0x81 on a
//! file system with 64-bit inode numbers, 8 of type 1 with 32-bit ones,
//! native-endian. The form is read once from the handle of the top and
//! checked against its inode number; any other form (another file
//! system) turns the fast path off. A node opened by handle is named by
//! its `/proc/self/fd` link, which must lie under the top: a file the
//! kernel has no path for (not in its cache since the mount) gives none.

use std::os::fd::{AsFd, AsRawFd, BorrowedFd};

/// The handle form of the volume.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Form {
    kind: i32,
    /// 64-bit inode numbers.
    wide: bool,
}

/// The form of the handle of `top`, whose inode number is `ino`.
pub(super) fn form_of(top: BorrowedFd<'_>, ino: u64) -> Option<Form> {
    let h = fhandle::handle_at(top, "").ok()?;
    let f = match (h.kind, h.bytes.len()) {
        (0x81, 12) => Form {
            kind: 0x81,
            wide: true,
        },
        (1, 8) => Form {
            kind: 1,
            wide: false,
        },
        _ => return None,
    };
    let (i, _) = f.parse(&h)?;
    (i == ino).then_some(f)
}

impl Form {
    /// Inode number and generation of a handle of this form.
    fn parse(self, h: &fhandle::Handle) -> Option<(u64, u32)> {
        if h.kind != self.kind {
            return None;
        }
        let b = &h.bytes;
        if self.wide {
            let ino = u64::from_ne_bytes(b.get(..8)?.try_into().ok()?);
            let generation = u32::from_ne_bytes(b.get(8..12)?.try_into().ok()?);
            Some((ino, generation))
        } else {
            let ino = u32::from_ne_bytes(b.get(..4)?.try_into().ok()?);
            let generation = u32::from_ne_bytes(b.get(4..8)?.try_into().ok()?);
            Some((u64::from(ino), generation))
        }
    }

    /// The handle of inode `ino`, generation `generation`.
    fn build(self, ino: u64, generation: u32) -> Option<fhandle::Handle> {
        let mut bytes = Vec::with_capacity(12);
        if self.wide {
            bytes.extend_from_slice(&ino.to_ne_bytes());
        } else {
            bytes.extend_from_slice(&u32::try_from(ino).ok()?.to_ne_bytes());
        }
        bytes.extend_from_slice(&generation.to_ne_bytes());
        Some(fhandle::Handle {
            kind: self.kind,
            bytes,
        })
    }

    /// The generation of the node `name` under `dir`.
    pub(super) fn generation(self, dir: BorrowedFd<'_>, name: &str) -> Option<u32> {
        let h = fhandle::handle_at(dir, if name == "." { "" } else { name }).ok()?;
        self.parse(&h).map(|(_, g)| g)
    }

    /// The path under `base` (the top's absolute path) of inode `ino`,
    /// generation `generation`, relative to the top.
    pub(super) fn path(
        self,
        top: BorrowedFd<'_>,
        base: &str,
        ino: u64,
        generation: u32,
    ) -> Option<String> {
        let h = self.build(ino, generation)?;
        let fd = fhandle::open_by(top, &h, fhandle::O_PATH).ok()?;
        let link = std::fs::read_link(format!("/proc/self/fd/{}", fd.as_fd().as_raw_fd())).ok()?;
        let link = link.to_str()?;
        if link.ends_with(" (deleted)") {
            return None;
        }
        if link == base {
            return Some(String::new());
        }
        link.strip_prefix(base)?
            .strip_prefix('/')
            .map(str::to_string)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handles_of_both_forms() {
        let wide = Form {
            kind: 0x81,
            wide: true,
        };
        let h = wide.build(0x1_0000_0083, 77).unwrap();
        assert_eq!((h.kind, h.bytes.len()), (0x81, 12));
        assert_eq!(wide.parse(&h), Some((0x1_0000_0083, 77)));
        let narrow = Form {
            kind: 1,
            wide: false,
        };
        let h = narrow.build(131, 5).unwrap();
        assert_eq!(h.bytes.len(), 8);
        assert_eq!(narrow.parse(&h), Some((131, 5)));
        assert_eq!(narrow.build(1 << 40, 5), None);
        assert_eq!(wide.parse(&h), None, "another form");
    }
}

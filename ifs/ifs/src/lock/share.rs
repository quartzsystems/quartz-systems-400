// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `open()` share modes: what an open lets other opens do. A new open
//! is refused when it would do what an existing open forbids, or
//! forbid what an existing open does.

use crate::ops::flags;

/// What an open reads and writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Access {
    /// Reads.
    pub read: bool,
    /// Writes.
    pub write: bool,
}

/// A share mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Share {
    /// `O_SHARE_RDONLY`: others may only read.
    ReadOnly,
    /// `O_SHARE_WRONLY`: others may only write.
    WriteOnly,
    /// `O_SHARE_RDWR`: others may read and write (the default).
    ReadWrite,
    /// `O_SHARE_NONE`: others may do neither.
    None,
}

impl Share {
    /// From open flags; none given is `O_SHARE_RDWR`. More than one is
    /// `None` (`EINVAL`).
    #[must_use]
    pub fn from_flags(oflag: u32) -> Option<Self> {
        let bits = oflag
            & (flags::O_SHARE_RDONLY
                | flags::O_SHARE_WRONLY
                | flags::O_SHARE_RDWR
                | flags::O_SHARE_NONE);
        Some(match bits {
            0 | flags::O_SHARE_RDWR => Self::ReadWrite,
            flags::O_SHARE_RDONLY => Self::ReadOnly,
            flags::O_SHARE_WRONLY => Self::WriteOnly,
            flags::O_SHARE_NONE => Self::None,
            _ => return None,
        })
    }

    /// Whether others may read.
    #[must_use]
    pub fn lets_read(self) -> bool {
        matches!(self, Self::ReadOnly | Self::ReadWrite)
    }

    /// Whether others may write.
    #[must_use]
    pub fn lets_write(self) -> bool {
        matches!(self, Self::WriteOnly | Self::ReadWrite)
    }

    /// The IBM name for displays.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::ReadOnly => "O_SHARE_RDONLY",
            Self::WriteOnly => "O_SHARE_WRONLY",
            Self::ReadWrite => "O_SHARE_RDWR",
            Self::None => "O_SHARE_NONE",
        }
    }
}

/// Whether an existing open (`a`, `sa`) and a new one (`b`, `sb`)
/// cannot both be open.
#[must_use]
pub fn conflicts(a: Access, sa: Share, b: Access, sb: Share) -> bool {
    (a.read && !sb.lets_read())
        || (a.write && !sb.lets_write())
        || (b.read && !sa.lets_read())
        || (b.write && !sa.lets_write())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every pair of access and share mode against every other.
    #[test]
    fn share_matrix() {
        use Share::{None as N, ReadOnly as RO, ReadWrite as RWS, WriteOnly as WO};
        let r = Access {
            read: true,
            write: false,
        };
        let w = Access {
            read: false,
            write: true,
        };
        let rw = Access {
            read: true,
            write: true,
        };
        // (existing access, existing share, new access, new share, conflict)
        let rows = [
            (r, RO, r, RO, false),
            (r, RO, w, RWS, true),
            (r, RO, r, WO, true),
            (r, RWS, rw, RWS, false),
            (rw, RWS, r, RO, true),
            (w, WO, w, WO, false),
            (w, WO, r, RWS, true),
            (r, N, r, RWS, true),
            (r, RWS, r, N, true),
            (w, RO, r, RWS, false),
            (w, RO, w, RWS, true),
            (r, WO, w, RWS, false),
            (r, WO, r, RWS, true),
            (w, RWS, w, WO, false),
        ];
        for (a, sa, b, sb, want) in rows {
            assert_eq!(conflicts(a, sa, b, sb), want, "{a:?} {sa:?} / {b:?} {sb:?}");
            // The rule is symmetric.
            assert_eq!(conflicts(b, sb, a, sa), want);
        }
        assert_eq!(Share::from_flags(0), Some(Share::ReadWrite));
        assert_eq!(Share::from_flags(flags::O_SHARE_NONE), Some(Share::None));
        assert_eq!(
            Share::from_flags(flags::O_SHARE_NONE | flags::O_SHARE_RDONLY),
            None
        );
    }
}

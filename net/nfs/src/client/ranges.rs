// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The byte ranges a lock client holds, kept to take them again after
//! the server restarted.

/// One range held.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Range {
    /// A write lock.
    pub exclusive: bool,
    /// The first byte.
    pub off: u64,
    /// Bytes; 0: to the end.
    pub len: u64,
}

impl Range {
    /// One past the last byte.
    #[must_use]
    pub fn end(&self) -> u64 {
        if self.len == 0 {
            u64::MAX
        } else {
            self.off.saturating_add(self.len)
        }
    }
}

/// Take `[off, off + len)` (0: to the end) out of `held`.
pub fn cut(held: &mut Vec<Range>, off: u64, len: u64) {
    let end = if len == 0 {
        u64::MAX
    } else {
        off.saturating_add(len)
    };
    let mut out = Vec::with_capacity(held.len());
    for h in held.drain(..) {
        if h.end() <= off || h.off >= end {
            out.push(h);
            continue;
        }
        if h.off < off {
            out.push(Range {
                len: off - h.off,
                ..h
            });
        }
        if h.end() > end {
            out.push(Range {
                off: end,
                len: if h.len == 0 { 0 } else { h.end() - end },
                ..h
            });
        }
    }
    *held = out;
}

/// Hold `r` (what it covers is replaced).
pub fn add(held: &mut Vec<Range>, r: Range) {
    cut(held, r.off, r.len);
    held.push(r);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(off: u64, len: u64) -> Range {
        Range {
            exclusive: true,
            off,
            len,
        }
    }

    #[test]
    fn ranges_are_cut() {
        let mut v = vec![r(0, 100)];
        cut(&mut v, 10, 10);
        assert_eq!(v, vec![r(0, 10), r(20, 80)]);
        let mut v = vec![r(50, 0)];
        cut(&mut v, 60, 10);
        assert_eq!(v, vec![r(50, 10), r(70, 0)]);
        let mut v = vec![r(0, 0)];
        cut(&mut v, 0, 0);
        assert!(v.is_empty());
        let mut v = vec![r(0, 10)];
        add(&mut v, r(5, 10));
        assert_eq!(v, vec![r(0, 5), r(5, 10)]);
    }
}

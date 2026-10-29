// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Permission modes: the symbolic `chmod` language and the `ls -l`
//! mode string.

use qsh::sys::{FileType, Stat};

/// Apply a `chmod` mode operand (octal or symbolic clauses) to the
/// current bits. `None` when the operand is not a mode.
pub fn parse(spec: &str, current: u32, is_dir: bool, umask: u32) -> Option<u32> {
    if spec.bytes().all(|b| (b'0'..=b'7').contains(&b)) && !spec.is_empty() {
        return u32::from_str_radix(spec, 8).ok().filter(|m| *m <= 0o7777);
    }
    let mut mode = current & 0o7777;
    for clause in spec.split(',') {
        let b = clause.as_bytes();
        let mut i = 0;
        let mut who = 0u32;
        while i < b.len() {
            match b[i] {
                b'u' => who |= 0o4700,
                b'g' => who |= 0o2070,
                b'o' => who |= 0o1007,
                b'a' => who |= 0o7777,
                _ => break,
            }
            i += 1;
        }
        let explicit = who != 0;
        if !explicit {
            who = 0o7777;
        }
        if i >= b.len() {
            return None;
        }
        let mut any = false;
        while i < b.len() {
            let op = b[i];
            if !matches!(op, b'+' | b'-' | b'=') {
                return None;
            }
            i += 1;
            any = true;
            let mut perm = 0u32;
            if i < b.len() && matches!(b[i], b'u' | b'g' | b'o') {
                let src = match b[i] {
                    b'u' => (mode >> 6) & 7,
                    b'g' => (mode >> 3) & 7,
                    _ => mode & 7,
                };
                perm = src << 6 | src << 3 | src;
                i += 1;
            } else {
                while i < b.len() && matches!(b[i], b'r' | b'w' | b'x' | b'X' | b's' | b't') {
                    perm |= match b[i] {
                        b'r' => 0o444,
                        b'w' => 0o222,
                        b'x' => 0o111,
                        b'X' if is_dir || mode & 0o111 != 0 => 0o111,
                        b'X' => 0,
                        b's' => 0o6000,
                        _ => 0o1000,
                    };
                    i += 1;
                }
            }
            let mut bits = perm & who;
            if !explicit && op != b'-' {
                bits &= !umask;
            }
            match op {
                b'+' => mode |= bits,
                b'-' => mode &= !bits,
                _ => {
                    let keep = if explicit { !who } else { 0 };
                    mode = (mode & keep) | bits;
                }
            }
        }
        if !any {
            return None;
        }
    }
    Some(mode)
}

/// The type letter of `ls -l`.
pub fn type_char(ft: FileType) -> char {
    match ft {
        FileType::File | FileType::Other => '-',
        FileType::Dir => 'd',
        FileType::Symlink => 'l',
        FileType::Fifo => 'p',
        FileType::Char => 'c',
        FileType::Block => 'b',
        FileType::Socket => 's',
    }
}

/// `-rwxr-xr-x` for a status.
pub fn string(st: &Stat) -> String {
    let m = st.mode;
    let mut s = String::with_capacity(10);
    s.push(type_char(st.ftype));
    let bit = |mask: u32, c: char| if m & mask != 0 { c } else { '-' };
    s.push(bit(0o400, 'r'));
    s.push(bit(0o200, 'w'));
    s.push(match (m & 0o100 != 0, m & 0o4000 != 0) {
        (true, true) => 's',
        (false, true) => 'S',
        (true, false) => 'x',
        _ => '-',
    });
    s.push(bit(0o040, 'r'));
    s.push(bit(0o020, 'w'));
    s.push(match (m & 0o010 != 0, m & 0o2000 != 0) {
        (true, true) => 's',
        (false, true) => 'S',
        (true, false) => 'x',
        _ => '-',
    });
    s.push(bit(0o004, 'r'));
    s.push(bit(0o002, 'w'));
    s.push(match (m & 0o001 != 0, m & 0o1000 != 0) {
        (true, true) => 't',
        (false, true) => 'T',
        (true, false) => 'x',
        _ => '-',
    });
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn symbolic() {
        assert_eq!(parse("755", 0, false, 0o22), Some(0o755));
        assert_eq!(parse("u+x", 0o644, false, 0o22), Some(0o744));
        assert_eq!(parse("go-w", 0o666, false, 0o22), Some(0o644));
        assert_eq!(parse("a=rw", 0o755, false, 0o22), Some(0o666));
        assert_eq!(parse("+x", 0o644, false, 0o22), Some(0o755));
        assert_eq!(parse("+w", 0o644, false, 0o22), Some(0o644));
        assert_eq!(parse("=r", 0o777, false, 0o22), Some(0o444));
        assert_eq!(parse("u=g", 0o654, false, 0o22), Some(0o554));
        assert_eq!(parse("a+X", 0o644, false, 0o22), Some(0o644));
        assert_eq!(parse("a+X", 0o644, true, 0o22), Some(0o755));
        assert_eq!(parse("u+s,g+s,+t", 0o755, false, 0o22), Some(0o7755));
        assert_eq!(parse("o=", 0o777, false, 0), Some(0o770));
        assert_eq!(parse("u+rw,g-x", 0o077, false, 0), Some(0o667));
        assert_eq!(parse("q", 0, false, 0), None);
        assert_eq!(parse("u", 0, false, 0), None);
        assert_eq!(parse("999", 0, false, 0), None);
    }
}

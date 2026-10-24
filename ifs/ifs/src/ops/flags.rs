// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! IBM's constants for the IFS APIs, from `QSYSINC/H(FCNTL)`,
//! `SYS(STAT)` and `H(UNISTD)`: open flags, `fcntl` commands, mode
//! bits, `access()` modes, `lseek` origins and `pathconf` names. The
//! octal values are written as the header writes them.

#![allow(clippy::unreadable_literal)]

/// `O_RDONLY`.
pub const O_RDONLY: u32 = 0o1;
/// `O_WRONLY`.
pub const O_WRONLY: u32 = 0o2;
/// `O_RDWR`.
pub const O_RDWR: u32 = 0o4;
/// `O_ACCMODE`.
pub const O_ACCMODE: u32 = O_RDONLY | O_WRONLY | O_RDWR;
/// `O_CREAT`.
pub const O_CREAT: u32 = 0o10;
/// `O_EXCL`.
pub const O_EXCL: u32 = 0o20;
/// `O_CCSID`: the conversion id is a CCSID.
pub const O_CCSID: u32 = 0o40;
/// `O_TRUNC`.
pub const O_TRUNC: u32 = 0o100;
/// `O_NONBLOCK`.
pub const O_NONBLOCK: u32 = 0o200;
/// `O_APPEND`.
pub const O_APPEND: u32 = 0o400;
/// `FASYNC`.
pub const FASYNC: u32 = 0o1000;
/// `O_SYNC`.
pub const O_SYNC: u32 = 0o2000;
/// `O_DSYNC`.
pub const O_DSYNC: u32 = 0o4000;
/// `O_RSYNC`.
pub const O_RSYNC: u32 = 0o10000;
/// `O_NOFOLLOW`.
pub const O_NOFOLLOW: u32 = 0o40000;
/// `O_NOCTTY`.
pub const O_NOCTTY: u32 = 0o100000;
/// `O_SHARE_RDONLY`.
pub const O_SHARE_RDONLY: u32 = 0o200000;
/// `O_SHARE_WRONLY`.
pub const O_SHARE_WRONLY: u32 = 0o400000;
/// `O_SHARE_RDWR`.
pub const O_SHARE_RDWR: u32 = 0o1000000;
/// `O_SHARE_NONE`.
pub const O_SHARE_NONE: u32 = 0o2000000;
/// `O_CODEPAGE`: the conversion id is a code page.
pub const O_CODEPAGE: u32 = 0o40000000;
/// `O_TEXTDATA`: convert data between the file's and the open's CCSID.
pub const O_TEXTDATA: u32 = 0o100000000;
/// `O_TEXT_CREAT`: a new file gets the conversion id as its CCSID and
/// data converts from the fifth argument.
pub const O_TEXT_CREAT: u32 = 0o200000000;
/// `O_FORCE_SCAN`.
pub const O_FORCE_SCAN: u32 = 0o400000000;
/// `O_INHERITMODE`: data authorities come from the parent directory.
pub const O_INHERITMODE: u32 = 0o1000000000;
/// `O_LARGEFILE`.
pub const O_LARGEFILE: u32 = 0o4000000000;

/// `F_DUPFD`.
pub const F_DUPFD: i32 = 0;
/// `F_GETFD`.
pub const F_GETFD: i32 = 1;
/// `F_SETFD`.
pub const F_SETFD: i32 = 2;
/// `F_GETLK`.
pub const F_GETLK: i32 = 3;
/// `F_SETLK`.
pub const F_SETLK: i32 = 4;
/// `F_SETLKW`.
pub const F_SETLKW: i32 = 5;
/// `F_GETFL`.
pub const F_GETFL: i32 = 6;
/// `F_SETFL`.
pub const F_SETFL: i32 = 7;
/// `F_GETOWN`.
pub const F_GETOWN: i32 = 8;
/// `F_SETOWN`.
pub const F_SETOWN: i32 = 9;
/// `F_GETLK64`.
pub const F_GETLK64: i32 = 12;
/// `F_SETLK64`.
pub const F_SETLK64: i32 = 13;
/// `F_SETLKW64`.
pub const F_SETLKW64: i32 = 14;
/// `FD_CLOEXEC`.
pub const FD_CLOEXEC: i32 = 1;

/// `S_IFIFO`.
pub const S_IFIFO: u32 = 0o010000;
/// `S_IFCHR`.
pub const S_IFCHR: u32 = 0o020000;
/// `S_IFDIR`.
pub const S_IFDIR: u32 = 0o040000;
/// `S_IFBLK`.
pub const S_IFBLK: u32 = 0o060000;
/// `S_IFREG`.
pub const S_IFREG: u32 = 0o100000;
/// `S_IFLNK`.
pub const S_IFLNK: u32 = 0o120000;
/// `S_IFSOCK`.
pub const S_IFSOCK: u32 = 0o140000;
/// `_S_IFNATIVE`: a QSYS.LIB object that is neither directory nor file.
pub const S_IFNATIVE: u32 = 0o200000;
/// The type bits.
pub const S_IFMT: u32 = 0o370000;

/// `R_OK`.
pub const R_OK: i32 = 4;
/// `W_OK`.
pub const W_OK: i32 = 2;
/// `X_OK`.
pub const X_OK: i32 = 1;
/// `F_OK`.
pub const F_OK: i32 = 0;

/// `SEEK_SET`.
pub const SEEK_SET: i32 = 0;
/// `SEEK_CUR`.
pub const SEEK_CUR: i32 = 1;
/// `SEEK_END`.
pub const SEEK_END: i32 = 2;

/// `_PC_CHOWN_RESTRICTED`.
pub const PC_CHOWN_RESTRICTED: i32 = 0;
/// `_PC_LINK_MAX`.
pub const PC_LINK_MAX: i32 = 1;
/// `_PC_MAX_CANON`.
pub const PC_MAX_CANON: i32 = 2;
/// `_PC_MAX_INPUT`.
pub const PC_MAX_INPUT: i32 = 3;
/// `_PC_NAME_MAX`.
pub const PC_NAME_MAX: i32 = 4;
/// `_PC_NO_TRUNC`.
pub const PC_NO_TRUNC: i32 = 5;
/// `_PC_PATH_MAX`.
pub const PC_PATH_MAX: i32 = 6;
/// `_PC_PIPE_BUF`.
pub const PC_PIPE_BUF: i32 = 7;
/// `_PC_VDISABLE`.
pub const PC_VDISABLE: i32 = 8;
/// `_PC_THREAD_SAFE`.
pub const PC_THREAD_SAFE: i32 = 9;

#[cfg(test)]
mod tests {
    use super::*;

    /// The values against the captured headers.
    #[test]
    fn header_capture_agrees() {
        let fcntl = include_str!("../../../../tools/conformance/oracle/qsysinc/H/FCNTL.txt");
        let stat = include_str!("../../../../tools/conformance/oracle/qsysinc/SYS/STAT.txt");
        let unistd = include_str!("../../../../tools/conformance/oracle/qsysinc/H/UNISTD.txt");
        let find = |hdr: &str, name: &str| -> i64 {
            for line in hdr.lines() {
                let mut w = line.split_whitespace();
                if w.next() == Some("#define") && w.next() == Some(name) {
                    let v = w.next().expect("value");
                    return if v.len() > 1 && v.starts_with('0') {
                        i64::from_str_radix(&v[1..], 8).expect("octal")
                    } else {
                        v.parse().expect("decimal")
                    };
                }
            }
            panic!("{name} not in header");
        };
        let oflags: &[(&str, u32)] = &[
            ("O_RDONLY", O_RDONLY),
            ("O_WRONLY", O_WRONLY),
            ("O_RDWR", O_RDWR),
            ("O_CREAT", O_CREAT),
            ("O_EXCL", O_EXCL),
            ("O_TRUNC", O_TRUNC),
            ("O_NOFOLLOW", O_NOFOLLOW),
            ("O_NOCTTY", O_NOCTTY),
            ("O_NONBLOCK", O_NONBLOCK),
            ("O_APPEND", O_APPEND),
            ("O_LARGEFILE", O_LARGEFILE),
            ("O_SYNC", O_SYNC),
            ("O_DSYNC", O_DSYNC),
            ("O_RSYNC", O_RSYNC),
            ("FASYNC", FASYNC),
            ("O_CODEPAGE", O_CODEPAGE),
            ("O_TEXTDATA", O_TEXTDATA),
            ("O_FORCE_SCAN", O_FORCE_SCAN),
            ("O_INHERITMODE", O_INHERITMODE),
            ("O_CCSID", O_CCSID),
            ("O_TEXT_CREAT", O_TEXT_CREAT),
            ("O_SHARE_RDONLY", O_SHARE_RDONLY),
            ("O_SHARE_WRONLY", O_SHARE_WRONLY),
            ("O_SHARE_RDWR", O_SHARE_RDWR),
            ("O_SHARE_NONE", O_SHARE_NONE),
        ];
        for (n, v) in oflags {
            assert_eq!(find(fcntl, n), i64::from(*v), "{n}");
        }
        for (n, v) in [
            ("F_DUPFD", F_DUPFD),
            ("F_GETFD", F_GETFD),
            ("F_SETFD", F_SETFD),
            ("F_GETFL", F_GETFL),
            ("F_SETFL", F_SETFL),
            ("F_GETOWN", F_GETOWN),
            ("F_SETOWN", F_SETOWN),
            ("F_GETLK64", F_GETLK64),
            ("F_SETLK64", F_SETLK64),
            ("F_SETLKW64", F_SETLKW64),
            ("FD_CLOEXEC", FD_CLOEXEC),
        ] {
            assert_eq!(find(fcntl, n), i64::from(v), "{n}");
        }
        for (n, v) in [
            ("_S_IFFIFO", S_IFIFO),
            ("_S_IFCHR", S_IFCHR),
            ("_S_IFDIR", S_IFDIR),
            ("_S_IFBLK", S_IFBLK),
            ("_S_IFREG", S_IFREG),
            ("_S_IFLNK", S_IFLNK),
            ("_S_IFSOCK", S_IFSOCK),
            ("_S_IFNATIVE", S_IFNATIVE),
        ] {
            assert_eq!(find(stat, n), i64::from(v), "{n}");
        }
        for (n, v) in [
            ("R_OK", R_OK),
            ("W_OK", W_OK),
            ("X_OK", X_OK),
            ("F_OK", F_OK),
            ("SEEK_SET", SEEK_SET),
            ("SEEK_CUR", SEEK_CUR),
            ("SEEK_END", SEEK_END),
            ("_PC_LINK_MAX", PC_LINK_MAX),
            ("_PC_NAME_MAX", PC_NAME_MAX),
            ("_PC_PATH_MAX", PC_PATH_MAX),
            ("_PC_THREAD_SAFE", PC_THREAD_SAFE),
        ] {
            assert_eq!(find(unistd, n), i64::from(v), "{n}");
        }
    }
}

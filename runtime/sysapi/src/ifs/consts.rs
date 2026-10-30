// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The constants a program passes and reads at the IFS APIs, by name:
//! the open flags, modes, `fcntl` commands and `pathconf` names of
//! `ifs::ops::flags`, and those only the API layer needs (lock types,
//! `statvfs` flags, `QP0L_*` values). [`ALL`] is what
//! `docs/IFS.md` "APIs" lists and `tests/ifs_spec.rs` compares.

#![allow(clippy::unreadable_literal)]

use ifs::ops::flags as fl;

/// `F_RDLCK`.
pub const F_RDLCK: i32 = 1;
/// `F_WRLCK`.
pub const F_WRLCK: i32 = 2;
/// `F_UNLCK`.
pub const F_UNLCK: i32 = 3;
/// `AT_FDCWD` for `openat()`: IBM's headers have none; the Linux value.
pub const AT_FDCWD: i32 = -100;

/// `ST_RDONLY`.
pub const ST_RDONLY: u32 = 0o1;
/// `ST_NOSUID`.
pub const ST_NOSUID: u32 = 0o2;
/// `ST_CASE_SENSITIVE`.
pub const ST_CASE_SENSITIVE: u32 = 0o4;
/// `ST_CHOWN_RESTRICTED`.
pub const ST_CHOWN_RESTRICTED: u32 = 0o10;
/// `ST_THREAD_SAFE`.
pub const ST_THREAD_SAFE: u32 = 0o20;

/// `S_IRWXU` and the other permission bits.
pub const S_IRUSR: u32 = 0o400;
/// `S_IWUSR`.
pub const S_IWUSR: u32 = 0o200;
/// `S_IXUSR`.
pub const S_IXUSR: u32 = 0o100;
/// `S_IRGRP`.
pub const S_IRGRP: u32 = 0o40;
/// `S_IWGRP`.
pub const S_IWGRP: u32 = 0o20;
/// `S_IXGRP`.
pub const S_IXGRP: u32 = 0o10;
/// `S_IROTH`.
pub const S_IROTH: u32 = 0o4;
/// `S_IWOTH`.
pub const S_IWOTH: u32 = 0o2;
/// `S_IXOTH`.
pub const S_IXOTH: u32 = 0o1;
/// `S_ISUID`.
pub const S_ISUID: u32 = 0o4000;
/// `S_ISGID`.
pub const S_ISGID: u32 = 0o2000;
/// `S_ISVTX`.
pub const S_ISVTX: u32 = 0o1000;

/// `QLG_CHAR_SINGLE`: the name follows the structure, one-byte delimiter.
pub const QLG_CHAR_SINGLE: u32 = 0;
/// `QLG_PTR_SINGLE`: a pointer to the name follows the structure.
pub const QLG_PTR_SINGLE: u32 = 1;
/// `QLG_CHAR_DOUBLE`: as `QLG_CHAR_SINGLE`, two-byte delimiter.
pub const QLG_CHAR_DOUBLE: u32 = 2;
/// `QLG_PTR_DOUBLE`: as `QLG_PTR_SINGLE`, two-byte delimiter.
pub const QLG_PTR_DOUBLE: u32 = 3;

/// `QP0L_DONOT_FOLLOW_SYMLNK`.
pub const QP0L_DONOT_FOLLOW_SYMLNK: u32 = 0;
/// `QP0L_FOLLOW_SYMLNK`.
pub const QP0L_FOLLOW_SYMLNK: u32 = 1;
/// `QP0L_USER_FUNCTION_PTR`.
pub const QP0L_USER_FUNCTION_PTR: u32 = 0;
/// `QP0L_USER_FUNCTION_PGM`.
pub const QP0L_USER_FUNCTION_PGM: u32 = 1;
/// `QP0L_SUBTREE_YES`.
pub const QP0L_SUBTREE_YES: u32 = 0;
/// `QP0L_SUBTREE_NO`.
pub const QP0L_SUBTREE_NO: u32 = 1;
/// `QP0L_PASS_WITH_ERRORID`.
pub const QP0L_PASS_WITH_ERRORID: u32 = 0;
/// `QP0L_BYPASS_NO_ERRORID`.
pub const QP0L_BYPASS_NO_ERRORID: u32 = 1;
/// `QP0L_END_PROCESS_SUBTREE`.
pub const QP0L_END_PROCESS_SUBTREE: u32 = 4;
/// `QP0L_SELECT_OK`.
pub const QP0L_SELECT_OK: u32 = 0;
/// `QP0L_SELECT_DONE`.
pub const QP0L_SELECT_DONE: u32 = 1;
/// `QP0L_SELECT_NOT_OK`.
pub const QP0L_SELECT_NOT_OK: u32 = 2;

macro_rules! table {
    ($($name:literal = $v:expr),* $(,)?) => {
        /// Every constant by its header name.
        pub const ALL: &[(&str, i64)] = &[$(($name, $v as i64)),*];
    };
}

table! {
    "O_RDONLY" = fl::O_RDONLY,
    "O_WRONLY" = fl::O_WRONLY,
    "O_RDWR" = fl::O_RDWR,
    "O_CREAT" = fl::O_CREAT,
    "O_EXCL" = fl::O_EXCL,
    "O_CCSID" = fl::O_CCSID,
    "O_TRUNC" = fl::O_TRUNC,
    "O_NONBLOCK" = fl::O_NONBLOCK,
    "O_APPEND" = fl::O_APPEND,
    "O_SYNC" = fl::O_SYNC,
    "O_DSYNC" = fl::O_DSYNC,
    "O_RSYNC" = fl::O_RSYNC,
    "O_NOFOLLOW" = fl::O_NOFOLLOW,
    "O_NOCTTY" = fl::O_NOCTTY,
    "O_SHARE_RDONLY" = fl::O_SHARE_RDONLY,
    "O_SHARE_WRONLY" = fl::O_SHARE_WRONLY,
    "O_SHARE_RDWR" = fl::O_SHARE_RDWR,
    "O_SHARE_NONE" = fl::O_SHARE_NONE,
    "O_CODEPAGE" = fl::O_CODEPAGE,
    "O_TEXTDATA" = fl::O_TEXTDATA,
    "O_TEXT_CREAT" = fl::O_TEXT_CREAT,
    "O_INHERITMODE" = fl::O_INHERITMODE,
    "O_LARGEFILE" = fl::O_LARGEFILE,
    "F_DUPFD" = fl::F_DUPFD,
    "F_GETFD" = fl::F_GETFD,
    "F_SETFD" = fl::F_SETFD,
    "F_GETLK" = fl::F_GETLK,
    "F_SETLK" = fl::F_SETLK,
    "F_SETLKW" = fl::F_SETLKW,
    "F_GETFL" = fl::F_GETFL,
    "F_SETFL" = fl::F_SETFL,
    "F_GETLK64" = fl::F_GETLK64,
    "F_SETLK64" = fl::F_SETLK64,
    "F_SETLKW64" = fl::F_SETLKW64,
    "FD_CLOEXEC" = fl::FD_CLOEXEC,
    "F_RDLCK" = F_RDLCK,
    "F_WRLCK" = F_WRLCK,
    "F_UNLCK" = F_UNLCK,
    "AT_FDCWD" = AT_FDCWD,
    "S_IRUSR" = S_IRUSR,
    "S_IWUSR" = S_IWUSR,
    "S_IXUSR" = S_IXUSR,
    "S_IRGRP" = S_IRGRP,
    "S_IWGRP" = S_IWGRP,
    "S_IXGRP" = S_IXGRP,
    "S_IROTH" = S_IROTH,
    "S_IWOTH" = S_IWOTH,
    "S_IXOTH" = S_IXOTH,
    "S_ISUID" = S_ISUID,
    "S_ISGID" = S_ISGID,
    "S_ISVTX" = S_ISVTX,
    "_S_IFFIFO" = fl::S_IFIFO,
    "_S_IFCHR" = fl::S_IFCHR,
    "_S_IFDIR" = fl::S_IFDIR,
    "_S_IFBLK" = fl::S_IFBLK,
    "_S_IFREG" = fl::S_IFREG,
    "_S_IFLNK" = fl::S_IFLNK,
    "_S_IFSOCK" = fl::S_IFSOCK,
    "_S_IFNATIVE" = fl::S_IFNATIVE,
    "R_OK" = fl::R_OK,
    "W_OK" = fl::W_OK,
    "X_OK" = fl::X_OK,
    "F_OK" = fl::F_OK,
    "SEEK_SET" = fl::SEEK_SET,
    "SEEK_CUR" = fl::SEEK_CUR,
    "SEEK_END" = fl::SEEK_END,
    "_PC_CHOWN_RESTRICTED" = fl::PC_CHOWN_RESTRICTED,
    "_PC_LINK_MAX" = fl::PC_LINK_MAX,
    "_PC_MAX_CANON" = fl::PC_MAX_CANON,
    "_PC_MAX_INPUT" = fl::PC_MAX_INPUT,
    "_PC_NAME_MAX" = fl::PC_NAME_MAX,
    "_PC_NO_TRUNC" = fl::PC_NO_TRUNC,
    "_PC_PATH_MAX" = fl::PC_PATH_MAX,
    "_PC_PIPE_BUF" = fl::PC_PIPE_BUF,
    "_PC_VDISABLE" = fl::PC_VDISABLE,
    "_PC_THREAD_SAFE" = fl::PC_THREAD_SAFE,
    "ST_RDONLY" = ST_RDONLY,
    "ST_NOSUID" = ST_NOSUID,
    "ST_CASE_SENSITIVE" = ST_CASE_SENSITIVE,
    "ST_CHOWN_RESTRICTED" = ST_CHOWN_RESTRICTED,
    "ST_THREAD_SAFE" = ST_THREAD_SAFE,
    "QLG_CHAR_SINGLE" = QLG_CHAR_SINGLE,
    "QLG_PTR_SINGLE" = QLG_PTR_SINGLE,
    "QLG_CHAR_DOUBLE" = QLG_CHAR_DOUBLE,
    "QLG_PTR_DOUBLE" = QLG_PTR_DOUBLE,
    "QP0L_DONOT_FOLLOW_SYMLNK" = QP0L_DONOT_FOLLOW_SYMLNK,
    "QP0L_FOLLOW_SYMLNK" = QP0L_FOLLOW_SYMLNK,
    "QP0L_USER_FUNCTION_PTR" = QP0L_USER_FUNCTION_PTR,
    "QP0L_USER_FUNCTION_PGM" = QP0L_USER_FUNCTION_PGM,
    "QP0L_SUBTREE_YES" = QP0L_SUBTREE_YES,
    "QP0L_SUBTREE_NO" = QP0L_SUBTREE_NO,
    "QP0L_PASS_WITH_ERRORID" = QP0L_PASS_WITH_ERRORID,
    "QP0L_BYPASS_NO_ERRORID" = QP0L_BYPASS_NO_ERRORID,
    "QP0L_END_PROCESS_SUBTREE" = QP0L_END_PROCESS_SUBTREE,
    "QP0L_SELECT_OK" = QP0L_SELECT_OK,
    "QP0L_SELECT_DONE" = QP0L_SELECT_DONE,
    "QP0L_SELECT_NOT_OK" = QP0L_SELECT_NOT_OK,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every value but `AT_FDCWD` against the captured headers.
    #[test]
    fn header_capture_agrees() {
        let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tools/conformance/oracle/qsysinc/");
        let text: String = [
            "H/FCNTL.txt",
            "SYS/STAT.txt",
            "H/UNISTD.txt",
            "SYS/STATVFS.txt",
            "H/QLG.txt",
            "H/QP0LSTDI.txt",
        ]
        .iter()
        .map(|f| std::fs::read_to_string(format!("{root}{f}")).expect("header"))
        .collect();
        let find = |name: &str| -> Option<i64> {
            let mut lines = text.lines().peekable();
            while let Some(line) = lines.next() {
                let mut w = line.split_whitespace();
                if w.next() != Some("#define") || w.next() != Some(name) {
                    continue;
                }
                let v = match w.next() {
                    Some(v) => v.to_string(),
                    None => lines.peek()?.split_whitespace().next()?.to_string(),
                };
                return if v.len() > 1 && v.starts_with('0') {
                    i64::from_str_radix(&v[1..], 8).ok()
                } else {
                    v.parse().ok()
                };
            }
            None
        };
        for (n, v) in ALL {
            if *n == "AT_FDCWD" {
                continue;
            }
            match find(n) {
                Some(h) => assert_eq!(h, *v, "{n}"),
                // Defined as an expression of others or in a header
                // not captured: the ifs crate's own test covers them.
                None => assert!(n.starts_with("O_") || n.starts_with("F_"), "{n} not found"),
            }
        }
    }
}

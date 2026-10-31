// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Save order (`SAVE_FORMAT.md` §6): what an object depends on is saved
//! before it, so a restore in stream order always finds it.

use store::obj::types;

/// A rank: journals, receivers, physical files, logical files, the
/// rest by type code.
#[must_use]
pub fn rank(type_code: u16, logical: bool) -> (u8, u16) {
    match type_code {
        types::LIB => (0, 0),
        JRN => (1, 0),
        JRNRCV => (2, 0),
        types::FILE if logical => (4, 0),
        types::FILE => (3, 0),
        t => (5, t),
    }
}

/// `*JRN`.
pub const JRN: u16 = 0x0018;
/// `*JRNRCV`.
pub const JRNRCV: u16 = 0x0019;
/// `*SAVF`.
pub const SAVF: u16 = 0x001A;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn journals_then_files_then_the_rest() {
        let mut v = [
            rank(types::DTAARA, false),
            rank(types::FILE, true),
            rank(types::FILE, false),
            rank(JRNRCV, false),
            rank(JRN, false),
        ];
        v.sort_unstable();
        assert_eq!(
            v,
            [
                rank(JRN, false),
                rank(JRNRCV, false),
                rank(types::FILE, false),
                rank(types::FILE, true),
                rank(types::DTAARA, false)
            ]
        );
    }
}

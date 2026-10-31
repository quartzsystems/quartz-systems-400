// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Save order (`SAVE_FORMAT.md` §6): what an object depends on is saved
//! before it, so a restore in stream order always finds it.

use store::obj::types;

/// A rank in IBM's save order (`DSPSAVF` lists it): the library,
/// journals, programs, service programs, modules, data queues, output
/// queues, message files, user indexes, save files, job descriptions,
/// commands, data areas, user spaces, the other types, physical files,
/// logical files, then receivers.
#[must_use]
pub fn rank(type_code: u16, logical: bool) -> (u8, u16) {
    const ORDER: &[u16] = &[
        types::LIB,
        JRN,
        types::PGM,
        types::SRVPGM,
        types::MODULE,
        types::DTAQ,
        types::OUTQ,
        types::MSGF,
        types::USRIDX,
        SAVF,
        types::JOBD,
        types::CMD,
        types::DTAARA,
        types::USRSPC,
    ];
    match type_code {
        types::FILE if logical => (31, 0),
        types::FILE => (30, 0),
        JRNRCV => (40, 0),
        t => match ORDER.iter().position(|x| *x == t) {
            Some(i) => (u8::try_from(i).expect("small"), 0),
            None => (20, t),
        },
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
    fn ibm_save_order() {
        let mut v = [
            rank(JRNRCV, false),
            rank(types::DTAARA, false),
            rank(types::FILE, true),
            rank(types::FILE, false),
            rank(JRN, false),
            rank(types::PGM, false),
        ];
        v.sort_unstable();
        assert_eq!(
            v,
            [
                rank(JRN, false),
                rank(types::PGM, false),
                rank(types::DTAARA, false),
                rank(types::FILE, false),
                rank(types::FILE, true),
                rank(JRNRCV, false)
            ]
        );
    }
}

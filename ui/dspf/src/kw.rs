// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The display keyword table: where each keyword may appear, how far
//! the compiler takes it, how many parameters it takes, which subfile
//! record it belongs on and whether option indicators may condition it.
//! `docs/DSPF.md` prints this table as the support matrix.

/// A level a keyword may be written at.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Level {
    /// Before the first record format.
    File,
    /// On an `R` specification.
    Record,
    /// On a field or constant.
    Field,
}

impl Level {
    /// For messages.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Record => "record",
            Self::Field => "field",
        }
    }

    fn bit(self) -> u8 {
        match self {
            Self::File => F,
            Self::Record => R,
            Self::Field => D,
        }
    }
}

/// How far the compiler takes a keyword.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Support {
    /// Parsed into the model and checked by its rules.
    Supported,
    /// Level and parameter count checked, kept with its conditioning
    /// for the runtime; nothing else is done with it.
    Stored,
    /// Refused with a message.
    Rejected,
}

impl Support {
    /// For the matrix.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Supported => "supported",
            Self::Stored => "accepted and stored",
            Self::Rejected => "rejected",
        }
    }
}

/// Which subfile record a `SFL*` keyword belongs on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SflSite {
    /// Any record.
    Any,
    /// The subfile record (`SFL`).
    Sfl,
    /// The subfile control record (`SFLCTL`).
    Ctl,
}

/// One keyword.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KwInfo {
    /// The name; `CAnn` and `CFnn` stand for the 48 command keys.
    pub name: &'static str,
    /// Levels allowed, as [`F`], [`R`], [`D`] bits.
    pub levels: u8,
    /// How far it is taken.
    pub support: Support,
    /// Fewest and most parameters; 255 for no limit.
    pub parms: (u8, u8),
    /// Which subfile record it belongs on.
    pub sfl: SflSite,
    /// Whether option indicators may condition it. Display size
    /// conditions (`*DSn`) are allowed on every keyword.
    pub cond: bool,
}

impl KwInfo {
    /// Whether the keyword may be written at `level`.
    #[must_use]
    pub fn allows(&self, level: Level) -> bool {
        self.levels & level.bit() != 0
    }

    /// The levels as `file, record, field`.
    #[must_use]
    pub fn level_names(&self) -> String {
        let mut v = Vec::new();
        for l in [Level::File, Level::Record, Level::Field] {
            if self.allows(l) {
                v.push(l.name());
            }
        }
        v.join(", ")
    }
}

/// File level.
pub const F: u8 = 1;
/// Record level.
pub const R: u8 = 2;
/// Field level.
pub const D: u8 = 4;

const fn kw(
    name: &'static str,
    levels: u8,
    support: Support,
    parms: (u8, u8),
    sfl: SflSite,
    cond: bool,
) -> KwInfo {
    KwInfo {
        name,
        levels,
        support,
        parms,
        sfl,
        cond,
    }
}

use SflSite::{Any, Ctl, Sfl};
use Support::{Rejected, Stored, Supported};

/// Every display keyword, alphabetically.
pub const TABLE: &[KwInfo] = &[
    kw("ALARM", R, Supported, (0, 0), Any, true),
    kw("ALIAS", D, Stored, (1, 1), Any, false),
    kw("ALTHELP", F, Supported, (0, 1), Any, false),
    kw("ALTPAGEDWN", F, Supported, (0, 1), Any, false),
    kw("ALTPAGEUP", F, Supported, (0, 1), Any, false),
    kw("ASSUME", R, Supported, (0, 0), Any, false),
    kw("AUTO", D, Supported, (1, 1), Any, true),
    kw("BLANKS", D, Supported, (1, 2), Any, false),
    kw("BLINK", R, Supported, (0, 0), Any, true),
    kw("CAnn", F | R, Supported, (0, 2), Any, true),
    kw("CCSID", F | D, Stored, (1, 2), Any, false),
    kw("CFnn", F | R, Supported, (0, 2), Any, true),
    kw("CHANGE", R | D, Supported, (1, 2), Any, true),
    kw("CHCACCEL", D, Stored, (2, 2), Any, true),
    kw("CHCAVAIL", F | R | D, Stored, (1, 3), Any, true),
    kw("CHCCTL", D, Stored, (2, 3), Any, true),
    kw("CHCSLT", F | R | D, Stored, (1, 3), Any, true),
    kw("CHCUNAVAIL", F | R | D, Stored, (1, 3), Any, true),
    kw("CHECK", D, Supported, (1, 9), Any, true),
    kw("CHGINPDFT", F | R | D, Supported, (0, 9), Any, false),
    kw("CHOICE", D, Stored, (2, 3), Any, true),
    kw("CHRID", D, Stored, (0, 0), Any, false),
    kw("CLEAR", F | R, Supported, (0, 2), Any, true),
    kw("CLRL", R, Supported, (1, 1), Any, false),
    kw("CMP", D, Supported, (2, 2), Any, false),
    kw("CNTFLD", D, Stored, (1, 1), Any, false),
    kw("COLOR", D, Supported, (1, 1), Any, true),
    kw("COMP", D, Supported, (2, 2), Any, false),
    kw("CSRINPONLY", F | R, Supported, (0, 0), Any, true),
    kw("CSRLOC", R, Supported, (2, 2), Any, true),
    kw("DATE", D, Supported, (0, 2), Any, false),
    kw("DATFMT", D, Supported, (1, 1), Any, false),
    kw("DATSEP", D, Supported, (1, 1), Any, false),
    kw("DFT", D, Supported, (1, 1), Any, false),
    kw("DLTCHK", D, Stored, (0, 0), Any, false),
    kw("DLTEDT", D, Stored, (0, 0), Any, false),
    kw("DSPATR", D, Supported, (1, 9), Any, true),
    kw("DSPMOD", R | D, Stored, (1, 1), Any, true),
    kw("DSPRL", F | R | D, Supported, (0, 0), Any, false),
    kw("DSPSIZ", F, Supported, (2, 6), Any, false),
    kw("DUP", D, Supported, (0, 2), Any, true),
    kw("EDTCDE", D, Supported, (1, 2), Any, false),
    kw("EDTMSK", D, Stored, (1, 1), Any, false),
    kw("EDTWRD", D, Supported, (1, 1), Any, false),
    kw("ENTFLDATR", F | R | D, Stored, (0, 3), Any, true),
    kw("ERASE", R, Supported, (1, 255), Any, true),
    kw("ERASEINP", R, Supported, (0, 1), Any, true),
    kw("ERRMSG", D, Supported, (1, 2), Any, true),
    kw("ERRMSGID", D, Supported, (2, 4), Any, true),
    kw("ERRSFL", F, Supported, (0, 0), Any, false),
    kw("FLDCSRPRG", D, Stored, (0, 0), Any, false),
    kw("FLTFIXDEC", D, Stored, (0, 0), Any, false),
    kw("FLTPCN", D, Stored, (1, 1), Any, false),
    kw("FRCDTA", R, Supported, (0, 0), Any, true),
    kw("GETRETAIN", R, Stored, (0, 0), Any, true),
    kw("HELP", F | R, Supported, (0, 2), Any, true),
    kw("HLPARA", R, Stored, (1, 4), Any, false),
    kw("HLPCLR", F | R, Stored, (0, 0), Any, false),
    kw("HLPCMDKEY", F | R, Stored, (1, 2), Any, false),
    kw("HLPPNLGRP", F | R, Stored, (2, 2), Any, true),
    kw("HLPRCD", R, Stored, (1, 2), Any, true),
    kw("HLPRTN", F | R, Stored, (0, 2), Any, true),
    kw("HLPSCHIDX", F, Stored, (1, 1), Any, false),
    kw("HLPSEQ", F | R, Stored, (2, 2), Any, false),
    kw("HLPSHELF", F, Stored, (1, 1), Any, false),
    kw("HLPTITLE", F | R, Stored, (1, 1), Any, false),
    kw("HOME", F | R, Supported, (0, 2), Any, true),
    kw("HTML", R | D, Rejected, (0, 255), Any, false),
    kw("IGCALTTYP", F, Rejected, (0, 0), Any, false),
    kw("IGCCNV", F, Rejected, (0, 255), Any, false),
    kw("INDARA", F, Supported, (0, 0), Any, false),
    kw("INDTXT", F | R | D, Supported, (2, 2), Any, false),
    kw("INVITE", F | R, Supported, (0, 0), Any, true),
    kw("INZINP", R, Supported, (0, 0), Any, true),
    kw("INZRCD", R, Supported, (0, 0), Any, false),
    kw("KEEP", R, Supported, (0, 0), Any, false),
    kw("LOCK", R, Supported, (0, 0), Any, true),
    kw("LOGINP", R, Stored, (0, 0), Any, false),
    kw("LOGOUT", R, Stored, (0, 0), Any, false),
    kw("MAPVAL", D, Stored, (1, 255), Any, false),
    kw("MDTOFF", R, Stored, (0, 1), Any, true),
    kw("MLTCHCFLD", D, Stored, (0, 5), Any, true),
    kw("MNUBAR", R, Supported, (0, 1), Any, false),
    kw("MNUBARCHC", D, Supported, (2, 4), Any, true),
    kw("MNUBARDSP", R, Supported, (0, 3), Any, true),
    kw("MNUBARSEP", F | R, Stored, (1, 2), Any, true),
    kw("MNUBARSW", F | R, Stored, (1, 1), Any, false),
    kw("MNUCNL", F | R, Supported, (1, 2), Any, false),
    kw("MOUBTN", D, Stored, (1, 3), Any, false),
    kw("MSGCON", D, Supported, (2, 3), Any, false),
    kw("MSGID", D, Supported, (2, 4), Any, true),
    kw("MSGLOC", F | R, Supported, (1, 1), Any, false),
    kw("NOCCSID", D, Stored, (0, 0), Any, false),
    kw("OPENPRT", F, Stored, (0, 0), Any, false),
    kw("OVERLAY", R, Supported, (0, 0), Any, true),
    kw("OVRATR", R | D, Supported, (0, 0), Any, true),
    kw("OVRDTA", R | D, Supported, (0, 0), Any, true),
    kw("PAGEDOWN", F | R, Supported, (0, 2), Any, true),
    kw("PAGEUP", F | R, Supported, (0, 2), Any, true),
    kw("PRINT", F | R, Supported, (0, 2), Any, true),
    kw("PROTECT", R, Supported, (0, 0), Any, true),
    kw("PSHBTNCHC", D, Stored, (2, 4), Any, true),
    kw("PSHBTNFLD", D, Stored, (0, 5), Any, true),
    kw("PULLDOWN", R, Supported, (0, 2), Any, false),
    kw("PULLINPUT", R, Stored, (0, 2), Any, false),
    kw("PUTOVR", R, Supported, (0, 0), Any, true),
    kw("PUTRETAIN", R | D, Supported, (0, 0), Any, true),
    kw("RANGE", D, Supported, (2, 2), Any, false),
    kw("REF", F, Supported, (1, 2), Any, false),
    kw("REFFLD", D, Supported, (1, 2), Any, false),
    kw("RETCMDKEY", R, Stored, (0, 0), Any, true),
    kw("RETKEY", R, Stored, (0, 0), Any, true),
    kw("RETLCKSTS", R, Stored, (0, 0), Any, true),
    kw("RMVWDW", R, Supported, (0, 0), Any, true),
    kw("ROLLDOWN", F | R, Supported, (0, 2), Any, true),
    kw("ROLLUP", F | R, Supported, (0, 2), Any, true),
    kw("RSTDSP", F | R, Supported, (0, 0), Any, true),
    kw("RTNCSRLOC", R, Supported, (2, 3), Any, false),
    kw("RTNDTA", R, Supported, (0, 0), Any, false),
    kw("SETOF", R | D, Supported, (1, 2), Any, true),
    kw("SETOFF", R | D, Supported, (1, 2), Any, true),
    kw("SFL", R, Supported, (0, 0), Sfl, false),
    kw("SFLCHCCTL", D, Stored, (0, 0), Sfl, false),
    kw("SFLCLR", R, Supported, (0, 0), Ctl, true),
    kw("SFLCSRPRG", D, Stored, (0, 0), Sfl, false),
    kw("SFLCSRRRN", R, Supported, (1, 1), Ctl, false),
    kw("SFLCTL", R, Supported, (1, 1), Ctl, false),
    kw("SFLDLT", R, Supported, (0, 0), Ctl, true),
    kw("SFLDROP", R, Supported, (1, 1), Ctl, true),
    kw("SFLDSP", R, Supported, (0, 0), Ctl, true),
    kw("SFLDSPCTL", R, Supported, (0, 0), Ctl, true),
    kw("SFLEND", R, Supported, (0, 2), Ctl, true),
    kw("SFLENTER", R, Stored, (1, 1), Ctl, false),
    kw("SFLFOLD", R, Supported, (1, 1), Ctl, true),
    kw("SFLINZ", R, Supported, (0, 0), Ctl, true),
    kw("SFLLIN", R, Supported, (1, 1), Ctl, false),
    kw("SFLMLTCHC", R, Stored, (0, 2), Ctl, true),
    kw("SFLMODE", R, Supported, (1, 1), Ctl, false),
    kw("SFLMSG", R, Supported, (1, 2), Ctl, true),
    kw("SFLMSGID", R, Supported, (2, 4), Ctl, true),
    kw("SFLMSGKEY", D, Supported, (0, 0), Sfl, false),
    kw("SFLMSGQ", D, Stored, (0, 1), Sfl, false),
    kw("SFLMSGRCD", R, Supported, (1, 1), Sfl, false),
    kw("SFLNXTCHG", R, Supported, (0, 0), Sfl, true),
    kw("SFLPAG", R, Supported, (1, 1), Ctl, false),
    kw("SFLPGMQ", D, Supported, (0, 1), Any, false),
    kw("SFLRCDNBR", D, Supported, (0, 2), Ctl, false),
    kw("SFLRNA", R, Supported, (0, 0), Ctl, false),
    kw("SFLROLVAL", D, Supported, (0, 0), Ctl, false),
    kw("SFLRTNSEL", R, Stored, (0, 0), Ctl, false),
    kw("SFLSCROLL", R, Supported, (1, 1), Ctl, false),
    kw("SFLSIZ", R, Supported, (1, 1), Ctl, false),
    kw("SFLSNGCHC", R, Stored, (0, 3), Ctl, true),
    kw("SLNO", R, Supported, (1, 1), Any, false),
    kw("SNGCHCFLD", D, Stored, (0, 5), Any, true),
    kw("SYSNAME", D, Supported, (0, 0), Any, false),
    kw("TEXT", R | D, Supported, (1, 1), Any, false),
    kw("TIME", D, Supported, (0, 1), Any, false),
    kw("TIMFMT", D, Supported, (1, 1), Any, false),
    kw("TIMSEP", D, Supported, (1, 1), Any, false),
    kw("UNLOCK", R, Supported, (0, 1), Any, true),
    kw("USER", D, Supported, (0, 0), Any, false),
    kw("USRDFN", F | R, Stored, (0, 0), Any, true),
    kw("USRDSPMGT", F, Supported, (0, 0), Any, false),
    kw("USRRSTDSP", R, Supported, (0, 0), Any, false),
    kw("VALUES", D, Supported, (1, 100), Any, false),
    kw("VLDCMDKEY", F | R, Supported, (1, 2), Any, true),
    kw("WDWBORDER", F | R, Stored, (1, 3), Any, true),
    kw("WDWTITLE", R, Stored, (1, 3), Any, true),
    kw("WINDOW", R, Supported, (1, 6), Any, false),
    kw("WRDWRAP", D, Stored, (0, 0), Any, true),
];

/// The table entry for a keyword name, `CAnn` and `CFnn` included.
#[must_use]
pub fn lookup(name: &str) -> Option<&'static KwInfo> {
    if let Some(key) = cmd_key_number(name) {
        let generic = if name.starts_with("CA") {
            "CAnn"
        } else {
            "CFnn"
        };
        return (1..=24)
            .contains(&key)
            .then(|| TABLE.iter().find(|k| k.name == generic))
            .flatten();
    }
    TABLE.iter().find(|k| k.name == name)
}

/// The number of `CAnn` or `CFnn`, valid or not; `None` for other
/// names.
#[must_use]
pub fn cmd_key_number(name: &str) -> Option<u8> {
    let digits = name
        .strip_prefix("CA")
        .or_else(|| name.strip_prefix("CF"))?;
    if digits.len() != 2 {
        return None;
    }
    digits.parse().ok()
}

/// Whether the name has the shape of a command key, in range or not.
#[must_use]
pub fn is_cmd_key(name: &str) -> bool {
    cmd_key_number(name).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_is_sorted_and_unique() {
        for pair in TABLE.windows(2) {
            assert!(
                pair[0].name < pair[1].name,
                "{} before {}",
                pair[0].name,
                pair[1].name
            );
        }
        for k in TABLE {
            assert!(k.parms.0 <= k.parms.1, "{}", k.name);
            assert!(k.levels != 0, "{}", k.name);
        }
    }

    #[test]
    fn command_keys() {
        assert_eq!(lookup("CA03").unwrap().name, "CAnn");
        assert_eq!(lookup("CF24").unwrap().name, "CFnn");
        assert!(lookup("CF25").is_none());
        assert!(lookup("CF1").is_none());
        assert_eq!(cmd_key_number("CF25"), Some(25));
        assert!(is_cmd_key("CA00"));
        assert!(!is_cmd_key("CAT"));
        assert!(lookup("SFLDSP").unwrap().allows(Level::Record));
        assert!(!lookup("SFLDSP").unwrap().allows(Level::Field));
        assert_eq!(lookup("DSPATR").unwrap().level_names(), "field");
    }
}

// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `SET OPTION`: the options an RPG program may set, in the order IBM
//! lists their `SQL5066` messages, the values each takes, and those
//! QS/400 cannot honour (refused with `SQL0084`). Settled on pub400
//! (`tools/conformance/rpg/b11sql/SQL061`, `SQL062`, `SQL064`).

/// The options, in IBM's order of their `SQL5066` messages.
pub const OPTIONS: &[&str] = &[
    "NAMING", "COMMIT", "DECMPT", "DATFMT", "DATSEP", "TIMFMT", "TIMSEP", "RDBCNNMTH", "ALWBLK", "DLYPRP", "ALWCPYDTA",
    "CLOSQLCSR", "USRPRF", "DYNUSRPRF", "DFTRDBCOL", "OPTLOB", "DYNDFTCOL", "DECRESULT", "COMPILEOPT", "SQLPATH",
    "SQLCURRULE", "MONITOR", "DECFLTRND", "EXTIND", "CONACC", "SQLCA", "SRTSEQ", "LANGID", "CNULRQD", "CNULIGN",
    "SYSTIME", "EVENTF",
];

/// Where an option comes in the listing's messages.
#[must_use]
pub fn rank(name: &str) -> usize {
    OPTIONS.iter().position(|o| *o == name).unwrap_or(OPTIONS.len())
}

pub use sql::options::{Check, check};

/// Options of an SQL routine that a program cannot set.
#[must_use]
pub fn routine_only(name: &str) -> bool {
    matches!(name, "DBGVIEW" | "OUTPUT" | "TGTRLS")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_are_checked() {
        assert_eq!(check("DATFMT", "*XYZ"), Check::Invalid);
        assert_eq!(check("DATFMT", "*ISO"), Check::Ok);
        assert!(matches!(check("DATFMT", "*USA"), Check::Unsupported(_)));
        assert_eq!(check("DECRESULT", "(31,31,0)"), Check::Ok);
        assert_eq!(check("ALWBLK", "*ALLREAD"), Check::Ok);
        assert!(rank("NAMING") < rank("COMMIT"));
        assert!(routine_only("DBGVIEW"));
    }
}

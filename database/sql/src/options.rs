// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The SQL options a precompile (`SET OPTION`, `CRTSQLRPGI`) or a run
//! (`RUNSQL`, `RUNSQLSTM`) takes: the values each allows and those
//! QS/400 does not honour, refused with `SQL0084` rather than ignored.
//! Settled on pub400 (`tools/conformance/rpg/b11sql/SQL061`, `SQL062`).

/// The special values an option takes (without the `*`); empty when it
/// takes any value.
fn values(name: &str) -> &'static [&'static str] {
    match name {
        "NAMING" => &["SYS", "SQL"],
        "COMMIT" => &["NONE", "NC", "CHG", "UR", "CS", "ALL", "RS", "RR"],
        "DECMPT" => &["PERIOD", "COMMA", "SYSVAL", "JOB"],
        "DATFMT" => &["JOB", "USA", "ISO", "EUR", "JIS", "MDY", "DMY", "YMD", "JUL"],
        "TIMFMT" => &["HMS", "USA", "ISO", "EUR", "JIS"],
        "RDBCNNMTH" => &["DUW", "RUW"],
        "ALWBLK" => &["NONE", "READ", "ALLREAD"],
        "DLYPRP" | "OPTLOB" | "DYNDFTCOL" | "EXTIND" | "SQLCA" | "CNULRQD" | "CNULIGN" | "SYSTIME" => &["YES", "NO"],
        "ALWCPYDTA" => &["NO", "YES", "OPTIMIZE"],
        "CLOSQLCSR" => &["ENDACTGRP", "ENDMOD"],
        "USRPRF" => &["NAMING", "OWNER", "USER"],
        "DYNUSRPRF" => &["USER", "OWNER"],
        "SQLCURRULE" => &["DB2", "STD"],
        "MONITOR" => &["USER", "SYSTEM"],
        "DECFLTRND" => &["HALFEVEN", "HALFUP", "DOWN", "CEILING", "FLOOR", "HALFDOWN", "UP"],
        "CONACC" => &["DFT", "CURCMT", "WAIT"],
        _ => &[],
    }
}

/// What an option's value is.
#[derive(Debug, PartialEq, Eq)]
pub enum Check {
    /// A value QS/400 runs as given.
    Ok,
    /// Not one of the option's values: `SQL0104` at the value.
    Invalid,
    /// A value QS/400 does not honour: `SQL0084` with what it is.
    Unsupported(String),
}

/// Check `value` (as written: `*ISO`, `'-'`, `(31, 31, 0)`) of `name`.
#[must_use]
#[allow(clippy::match_same_arms)] // each option's values beside its refusal
pub fn check(name: &str, value: &str) -> Check {
    let v = value.trim();
    let special = v.strip_prefix('*');
    let allowed = values(name);
    if !allowed.is_empty() && special.is_some_and(|s| !allowed.contains(&s)) {
        return Check::Invalid;
    }
    let refuse = |why: &str| Check::Unsupported(format!("{name}({v}): {why}"));
    match (name, special.unwrap_or(v)) {
        // SQL dates and times are formatted ISO.
        ("DATFMT", "JOB" | "ISO") | ("TIMFMT", "HMS" | "ISO") | ("DATSEP", "JOB" | "'-'" | "-") | ("TIMSEP", "JOB" | "'.'" | ".") => Check::Ok,
        ("DATFMT" | "TIMFMT" | "DATSEP" | "TIMSEP", _) => refuse("SQL dates and times are kept in ISO form"),
        ("DECMPT", "PERIOD" | "JOB" | "SYSVAL") => Check::Ok,
        ("DECMPT", _) => refuse("SQL numbers take a decimal point"),
        ("USRPRF", "NAMING" | "USER") | ("DYNUSRPRF", "USER") => Check::Ok,
        ("USRPRF" | "DYNUSRPRF", _) => refuse("SQL statements run with the user's authority"),
        ("DFTRDBCOL", "NONE") | ("DYNDFTCOL", "NO") => Check::Ok,
        ("DFTRDBCOL" | "DYNDFTCOL", _) => refuse("a default schema for embedded SQL is not kept"),
        ("SQLPATH", "LIBL" | "NAMING") => Check::Ok,
        ("SQLPATH", _) => refuse("the path is the library list"),
        ("SQLCURRULE", "DB2") => Check::Ok,
        ("SQLCURRULE", _) => refuse("only DB2 rules are kept"),
        ("DECRESULT", d) if d.split(|c: char| !c.is_ascii_digit()).filter(|x| !x.is_empty()).collect::<Vec<_>>() == ["31", "31", "0"] => Check::Ok,
        ("DECRESULT", _) => refuse("decimal results follow the 31-digit rules"),
        ("DECFLTRND", "HALFEVEN") => Check::Ok,
        ("DECFLTRND", _) => refuse("DECFLOAT rounds half even in embedded SQL"),
        ("CONACC", "DFT" | "WAIT") => Check::Ok,
        ("CONACC", _) => refuse("rows locked by others are waited for"),
        ("EXTIND", "NO") | ("SQLCA", "YES") => Check::Ok,
        ("EXTIND", _) => refuse("extended indicators are not kept"),
        ("SQLCA", _) => refuse("the SQLCA is always kept"),
        ("SRTSEQ", "JOB" | "HEX" | "JOBRUN") => Check::Ok,
        ("SRTSEQ", _) => refuse("SQL compares by hexadecimal value"),
        ("COMPILEOPT", c) if c.trim_matches('\'').trim_start_matches('*').eq_ignore_ascii_case("NONE") => Check::Ok,
        ("COMPILEOPT", _) => refuse("no options are passed to the compiler"),
        ("EVENTF", _) => refuse("no event file is written"),
        // Hints and options without an effect here: the precompile runs
        // inside the compile, the connection is the local one.
        _ => Check::Ok,
    }
}


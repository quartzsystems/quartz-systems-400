// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The session attributes Change Session Attributes shows (F13, option
//! 1): statement processing, where a query's rows go, commitment
//! control, the formats, naming and the list selection. Kept as
//! `KEY=VALUE` lines in the saved session; `STRSQL`'s parameters set
//! them when a session starts.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::IsqlError;

/// How a value is checked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Check {
    /// One of these words.
    Set(&'static [&'static str]),
    /// One of these words or a name.
    NameOr(&'static [&'static str]),
    /// One character of these, `*JOB` or `*BLANK`.
    Char(&'static str),
    /// Anything, blank included.
    Any,
}

/// One attribute: its key (the `STRSQL` keyword), its label on the
/// display, its default and its check.
#[derive(Clone, Copy, Debug)]
pub struct Field {
    /// The keyword.
    pub key: &'static str,
    /// The label.
    pub label: &'static str,
    /// The default.
    pub default: &'static str,
    /// The check.
    pub check: Check,
}

const fn f(key: &'static str, label: &'static str, default: &'static str, check: Check) -> Field {
    Field {
        key,
        label,
        default,
        check,
    }
}

/// The attributes: Change Session Attributes' two pages in their
/// order, then the ones only `STRSQL` sets.
pub const FIELDS: &[Field] = &[
    f(
        "PROCESS",
        "Statement processing",
        "*RUN",
        Check::Set(&["*RUN", "*VLD", "*SYN"]),
    ),
    f("OUTPUT", "SELECT output", "1", Check::Set(&["1", "2", "3"])),
    f(
        "COMMIT",
        "Commitment control",
        "*CHG",
        Check::Set(&["*NONE", "*CHG", "*CS", "*ALL", "*RR", "*NC", "*UR", "*RS"]),
    ),
    f(
        "DATFMT",
        "Date format",
        "*JOB",
        Check::Set(&[
            "*JOB", "*USA", "*ISO", "*EUR", "*JIS", "*MDY", "*DMY", "*YMD", "*JUL",
        ]),
    ),
    f("DATSEP", "Date separator", "*JOB", Check::Char("/.,- ")),
    f(
        "TIMFMT",
        "Time format",
        "*HMS",
        Check::Set(&["*HMS", "*USA", "*ISO", "*EUR", "*JIS"]),
    ),
    f("TIMSEP", "Time separator", "*JOB", Check::Char(":., ")),
    f(
        "REFRESH",
        "Data refresh",
        "*ALWAYS",
        Check::Set(&["*ALWAYS", "*FORWARD"]),
    ),
    f(
        "ALWCPYDTA",
        "Allow copy data",
        "*OPTIMIZE",
        Check::Set(&["*YES", "*OPTIMIZE", "*NO"]),
    ),
    f(
        "NAMING",
        "Naming convention",
        "*SYS",
        Check::Set(&["*SYS", "*SQL"]),
    ),
    f(
        "LIBOPT",
        "List of libraries",
        "*LIBL",
        Check::NameOr(&["*LIBL", "*USRLIBL", "*ALLUSR", "*ALL", "*CURLIB"]),
    ),
    f(
        "LISTTYPE",
        "List type",
        "*ALL",
        Check::Set(&["*ALL", "*SQL"]),
    ),
    f(
        "DECPNT",
        "Decimal point",
        "*JOB",
        Check::Set(&["*PERIOD", "*COMMA", "*JOB", "*SYSVAL"]),
    ),
    f(
        "SRTSEQ",
        "Sort sequence",
        "*JOB",
        Check::NameOr(&["*JOB", "*JOBRUN", "*LANGIDUNQ", "*LANGIDSHR", "*HEX"]),
    ),
    f(
        "LANGID",
        "Language identifier",
        "*JOB",
        Check::NameOr(&["*JOB", "*JOBRUN"]),
    ),
    f(
        "SQLCURRULE",
        "SQL rules",
        "*DB2",
        Check::Set(&["*DB2", "*STD"]),
    ),
    f(
        "DSPPWD",
        "Display password",
        "*NO",
        Check::Set(&["*NO", "*YES"]),
    ),
    f(
        "PGMLNG",
        "Programming language",
        "*NONE",
        Check::Set(&["*NONE", "*C", "*CBL", "*PLI", "*RPG", "*FTN"]),
    ),
    f(
        "SQLSTRDLM",
        "SQL string delimiter",
        "*APOSTSQL",
        Check::Set(&["*APOSTSQL", "*QUOTESQL"]),
    ),
    f(
        "DFTRDBCOL",
        "Default collection",
        "*NONE",
        Check::NameOr(&["*NONE"]),
    ),
    f("OUTFILE", "File", "", Check::Any),
    f("OUTLIB", "Library", "", Check::Any),
    f("OUTMBR", "Member", "", Check::Any),
];

/// The field of a key.
#[must_use]
pub fn field(key: &str) -> Option<&'static Field> {
    FIELDS.iter().find(|f| f.key == key)
}

/// Whether `v` is a valid system name.
fn is_name(v: &str) -> bool {
    let mut c = v.chars();
    matches!(c.next(), Some(ch) if ch.is_ascii_alphabetic() || "$#@".contains(ch))
        && v.len() <= 10
        && v.chars()
            .all(|ch| ch.is_ascii_alphanumeric() || "$#@_.".contains(ch))
}

/// The session's attributes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Attrs {
    values: BTreeMap<&'static str, String>,
}

impl Default for Attrs {
    fn default() -> Self {
        Self {
            values: FIELDS
                .iter()
                .map(|f| (f.key, f.default.to_string()))
                .collect(),
        }
    }
}

/// Where a query's rows go.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Output {
    /// Display Data.
    Display,
    /// A spooled file of `QSYSPRT`.
    Printer,
    /// A database file.
    File,
}

/// What the commitment control attribute asks of the database: no
/// definition, or one at a lock level (`*CHG`, `*CS`, `*ALL`).
#[must_use]
pub fn lock_level(commit: &str) -> Option<&'static str> {
    match commit {
        "*CHG" | "*UR" => Some("*CHG"),
        "*CS" => Some("*CS"),
        "*ALL" | "*RS" | "*RR" => Some("*ALL"),
        _ => None,
    }
}

impl Attrs {
    /// A value; blank for an unknown key.
    #[must_use]
    pub fn get(&self, key: &str) -> String {
        self.values.get(key).cloned().unwrap_or_default()
    }

    /// A value as its field shows it: a separator character quoted.
    #[must_use]
    pub fn shown(&self, key: &str) -> String {
        let v = self.get(key);
        match field(key).map(|f| f.check) {
            Some(Check::Char(_)) if v != "*JOB" => format!("'{v}'"),
            _ => v,
        }
    }

    /// Set a value after checking it; upper case except for a
    /// separator character, which may be typed quoted.
    ///
    /// # Errors
    /// The value is not one the attribute takes.
    pub fn set(&mut self, key: &str, value: &str) -> Result<(), IsqlError> {
        let Some(f) = field(key) else {
            return Ok(());
        };
        let raw = value.trim_end();
        let up = raw.trim().to_ascii_uppercase();
        let v = match f.check {
            Check::Char(_) if up == "*JOB" => up,
            Check::Char(_) if up == "*BLANK" => " ".to_string(),
            Check::Char(_) => {
                let t = raw.trim_start();
                let inner = t
                    .strip_prefix('\'')
                    .and_then(|r| r.strip_suffix('\''))
                    .unwrap_or(t);
                inner
                    .chars()
                    .next()
                    .map_or_else(|| " ".to_string(), |c| c.to_string())
            }
            _ => up,
        };
        let ok = match f.check {
            Check::Set(words) => words.contains(&v.as_str()),
            Check::NameOr(words) => {
                words.contains(&v.as_str()) || is_name(&v) || (v.is_empty() && f.default.is_empty())
            }
            Check::Char(set) => v == "*JOB" || set.contains(v.as_str()),
            Check::Any => true,
        };
        if !ok {
            return Err(IsqlError::BadValue {
                field: f.label,
                value: v,
            });
        }
        self.values.insert(f.key, v);
        Ok(())
    }

    /// Where a query's rows go.
    #[must_use]
    pub fn output(&self) -> Output {
        match self.get("OUTPUT").as_str() {
            "2" => Output::Printer,
            "3" => Output::File,
            _ => Output::Display,
        }
    }

    /// `*SQL` naming.
    #[must_use]
    pub fn sql_naming(&self) -> bool {
        self.get("NAMING") == "*SQL"
    }

    /// Statements are only checked, not run.
    #[must_use]
    pub fn syntax_only(&self) -> bool {
        self.get("PROCESS") != "*RUN"
    }

    /// The `KEY=VALUE` lines.
    #[must_use]
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        for f in FIELDS {
            let _ = writeln!(out, "{}={}", f.key, self.shown(f.key));
        }
        out
    }

    /// From `KEY=VALUE` lines; a key the release does not know, or a
    /// value no longer valid, keeps its default.
    #[must_use]
    pub fn from_text(text: &str) -> Self {
        let mut a = Self::default();
        for line in text.lines() {
            if let Some((k, v)) = line.split_once('=') {
                let _ = a.set(k.trim(), v);
            }
        }
        a
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_and_checks() {
        let mut a = Attrs::default();
        assert_eq!(a.get("NAMING"), "*SYS");
        assert!(a.set("NAMING", "*sql").is_ok());
        assert!(a.sql_naming());
        assert!(a.set("NAMING", "*XYZ").is_err());
        assert!(a.set("DATSEP", "/").is_ok());
        assert_eq!(a.get("DATSEP"), "/");
        assert_eq!(a.shown("DATSEP"), "'/'");
        assert!(a.set("DATSEP", "x").is_err());
        assert!(a.set("DATSEP", "'.'").is_ok());
        assert_eq!(a.get("DATSEP"), ".");
        assert!(a.set("DATSEP", "*blank").is_ok());
        assert_eq!(a.get("DATSEP"), " ");
        assert!(a.set("DATSEP", "*JOB").is_ok());
        assert_eq!(a.shown("DATSEP"), "*JOB");
        assert!(a.set("SRTSEQ", "QSYSTRNTBL").is_ok());
        assert!(a.set("SRTSEQ", "9BAD").is_err());
        assert!(a.set("OUTPUT", "2").is_ok());
        assert_eq!(a.output(), Output::Printer);
    }

    #[test]
    fn text_round_trip() {
        let mut a = Attrs::default();
        a.set("COMMIT", "*NONE").unwrap();
        a.set("TIMSEP", "' '").unwrap();
        let b = Attrs::from_text(&a.to_text());
        assert_eq!(a, b);
        assert_eq!(b.get("TIMSEP"), " ");
    }

    #[test]
    fn lock_levels() {
        assert_eq!(lock_level("*NONE"), None);
        assert_eq!(lock_level("*NC"), None);
        assert_eq!(lock_level("*UR"), Some("*CHG"));
        assert_eq!(lock_level("*RR"), Some("*ALL"));
    }
}

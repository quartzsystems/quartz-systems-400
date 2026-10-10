// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What the PF and LF compilers share: options, the result, limits and
//! the keyword parameter helpers.

pub mod field;
pub mod filekw;
pub mod keys;

use qsname::Name;

use crate::diag::{Diagnostic, Diagnostics, Severity, ids};
use crate::lex::{Keyword, Spec};
use crate::model::FileRef;

/// What the listing header shows. Nothing here changes the result.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CompileOpts {
    /// The file being created, `LIB/FILE`.
    pub file: Option<String>,
    /// The source member, `LIB/FILE(MBR)`.
    pub source: Option<String>,
    /// A timestamp for the header; the caller supplies it so listings
    /// are reproducible.
    pub timestamp: Option<String>,
}

/// The outcome of a compile.
#[derive(Clone, Debug)]
pub struct Compilation<T> {
    /// The definition, when nothing at severity 20 or more was found.
    pub definition: Option<T>,
    /// Every message, in source order.
    pub diagnostics: Vec<Diagnostic>,
    /// The compile listing, for the spool file later.
    pub listing: String,
}

impl<T> Compilation<T> {
    /// The worst severity.
    #[must_use]
    pub fn max_severity(&self) -> Option<Severity> {
        self.diagnostics.iter().map(|d| d.severity).max()
    }

    /// The message IDs, in order.
    #[must_use]
    pub fn ids(&self) -> Vec<&'static str> {
        self.diagnostics.iter().map(|d| d.id).collect()
    }
}

/// Which compiler is running; some keywords belong to one file type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileKind {
    /// `CRTPF`.
    Physical,
    /// `CRTLF`.
    Logical,
}

impl FileKind {
    /// For messages.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Physical => "physical",
            Self::Logical => "logical",
        }
    }
}

/// Longest record.
pub const MAX_RECORD_LEN: u32 = 32766;
/// Most fields in a format.
pub const MAX_FIELDS: usize = 8000;
/// Longest key.
pub const MAX_KEY_LEN: u32 = 2000;
/// Most key fields.
pub const MAX_KEY_FIELDS: usize = 120;

/// Every keyword a database file may carry at some level. One of these
/// in the wrong place is "not allowed here"; anything else is unknown.
const KNOWN: &[&str] = &[
    "UNIQUE",
    "FIFO",
    "LIFO",
    "FCFO",
    "REF",
    "REFACCPTH",
    "ALTSEQ",
    "CCSID",
    "TEXT",
    "FORMAT",
    "PFILE",
    "JFILE",
    "JOIN",
    "JFLD",
    "JDUPSEQ",
    "JDFTVAL",
    "JREF",
    "DYNSLT",
    "ALL",
    "VALUES",
    "RANGE",
    "COMP",
    "CMP",
    "CHECK",
    "CONCAT",
    "SST",
    "RENAME",
    "TRNTBL",
    "REFFLD",
    "COLHDG",
    "ALIAS",
    "VARLEN",
    "ALWNULL",
    "DFT",
    "DATFMT",
    "DATSEP",
    "TIMFMT",
    "TIMSEP",
    "EDTCDE",
    "EDTWRD",
    "FLTPCN",
    "DESCEND",
    "ABSVAL",
    "UNSIGNED",
    "SIGNED",
    "ZONE",
    "DIGIT",
    "NOALTSEQ",
    "REFSHIFT",
];

/// Report a keyword the current level does not take.
pub(crate) fn level_error(kw: &Keyword, level: &str, kind: FileKind, diags: &mut Diagnostics) {
    if KNOWN.contains(&kw.name.as_str()) {
        diags.push(ids::not_at_level(kw.line, &kw.name, level, kind.name()));
    } else {
        diags.push(ids::unknown_keyword(kw.line, &kw.name));
    }
}

/// Check the parameter count; report and return false when it is off.
pub(crate) fn count(kw: &Keyword, min: usize, max: usize, diags: &mut Diagnostics) -> bool {
    let n = kw.parms.len();
    if (min..=max).contains(&n) {
        return true;
    }
    let want = if min == max {
        min.to_string()
    } else {
        format!("{min} to {max}")
    };
    diags.push(ids::parm_count(kw.line, &kw.name, &want, n));
    false
}

/// Parameter `i` as quoted text of at most `max_len` characters.
pub(crate) fn quoted_parm(
    kw: &Keyword,
    i: usize,
    max_len: usize,
    diags: &mut Diagnostics,
) -> Option<String> {
    let Some(p) = kw.parms.get(i) else {
        diags.push(ids::parm_count(
            kw.line,
            &kw.name,
            &(i + 1).to_string(),
            kw.parms.len(),
        ));
        return None;
    };
    match p.as_quoted() {
        Some(q) if q.chars().count() <= max_len => Some(q.to_string()),
        Some(_) => {
            diags.push(ids::bad_parm(
                kw.line,
                &kw.name,
                p,
                format!("longer than {max_len} characters"),
            ));
            None
        }
        None => {
            diags.push(ids::bad_parm(kw.line, &kw.name, p, "must be quoted"));
            None
        }
    }
}

/// Parameter `i` as an unsigned number.
pub(crate) fn number_parm(kw: &Keyword, i: usize, diags: &mut Diagnostics) -> Option<u32> {
    let Some(p) = kw.parms.get(i) else {
        diags.push(ids::parm_count(
            kw.line,
            &kw.name,
            &(i + 1).to_string(),
            kw.parms.len(),
        ));
        return None;
    };
    let Some(n) = p.as_word().and_then(|w| w.parse().ok()) else {
        diags.push(ids::bad_parm(kw.line, &kw.name, p, "must be a number"));
        return None;
    };
    Some(n)
}

/// Parameter `i` as `LIB/FILE` or `FILE`.
pub(crate) fn file_ref_parm(kw: &Keyword, i: usize, diags: &mut Diagnostics) -> Option<FileRef> {
    let Some(p) = kw.parms.get(i) else {
        diags.push(ids::parm_count(
            kw.line,
            &kw.name,
            &(i + 1).to_string(),
            kw.parms.len(),
        ));
        return None;
    };
    let parsed = p.as_word().ok_or_else(|| "must be a file name".to_string());
    match parsed.and_then(FileRef::parse) {
        Ok(r) => Some(r),
        Err(why) => {
            diags.push(ids::bad_parm(kw.line, &kw.name, p, why));
            None
        }
    }
}

/// A field, format or key name: the object name rules without quotes.
pub(crate) fn parse_name(text: &str) -> Result<Name, String> {
    if text.starts_with('"') {
        return Err("quoted names are not allowed".to_string());
    }
    Name::parse(text).map_err(|e| e.to_string())
}

/// The name of an `R`, `K`, `J`, `S` or `O` specification.
pub(crate) fn spec_name(spec: &Spec, what: &str, diags: &mut Diagnostics) -> Option<Name> {
    let Some(text) = spec.name.as_deref() else {
        diags.push(ids::name_missing(spec.line, what));
        return None;
    };
    match parse_name(text) {
        Ok(n) => Some(n),
        Err(why) => {
            diags.push(ids::bad_name(spec.line, text, why));
            None
        }
    }
}

/// Database files have no use for positions 8-16 and 39-44.
pub(crate) fn check_db_columns(spec: &Spec, diags: &mut Diagnostics) {
    if !spec.conditioning.is_empty() {
        diags.push(ids::conditioning_ignored(spec.line));
    }
    if !spec.location.is_empty() {
        diags.push(ids::location_not_allowed(spec.line));
    }
}

/// Report a keyword given twice; true when it was.
pub(crate) fn seen_before(already: bool, kw: &Keyword, diags: &mut Diagnostics) -> bool {
    if already {
        diags.push(ids::duplicate_keyword(kw.line, &kw.name));
    }
    already
}

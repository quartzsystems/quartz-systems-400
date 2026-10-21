// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! File-level keywords.

use super::{
    FileKind, count, file_ref_parm, level_error, number_parm, parse_name, quoted_parm, seen_before,
};
use crate::diag::{Diagnostics, ids};
use crate::lex::{Keyword, Spec};
use crate::model::{DupOrder, FileRef, NullKeys};

/// What the lines before the first `R` said. Lines are kept for the
/// checks that need the whole file (keys exist, no conflicts).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FileKeywords {
    /// `UNIQUE` and its line.
    pub unique: Option<(NullKeys, usize)>,
    /// `FIFO`, `LIFO` or `FCFO` and its line.
    pub dup_order: Option<(DupOrder, usize)>,
    /// `REF`.
    pub reference: Option<FileRef>,
    /// `REFACCPTH` and its line.
    pub refaccpth: Option<(FileRef, usize)>,
    /// `ALTSEQ`.
    pub altseq: Option<FileRef>,
    /// `CCSID`.
    pub ccsid: Option<u16>,
    /// `TEXT`.
    pub text: Option<String>,
    /// `DYNSLT` and its line.
    pub dynslt: Option<usize>,
    /// `JDFTVAL` and its line.
    pub jdftval: Option<usize>,
}

/// Apply the keywords of a file-level entry.
pub fn apply(fk: &mut FileKeywords, spec: &Spec, kind: FileKind, diags: &mut Diagnostics) {
    for kw in &spec.keywords {
        match kw.name.as_str() {
            "UNIQUE" => unique(fk, kw, diags),
            "FIFO" | "LIFO" | "FCFO" => dup_order(fk, kw, diags),
            "REF" => reference(fk, kw, diags),
            "REFACCPTH" => {
                if seen_before(fk.refaccpth.is_some(), kw, diags) || !count(kw, 1, 1, diags) {
                    continue;
                }
                if let Some(r) = file_ref_parm(kw, 0, diags) {
                    fk.refaccpth = Some((r, kw.line));
                }
            }
            "ALTSEQ" => {
                if seen_before(fk.altseq.is_some(), kw, diags) || !count(kw, 1, 1, diags) {
                    continue;
                }
                fk.altseq = file_ref_parm(kw, 0, diags);
            }
            "CCSID" => {
                if seen_before(fk.ccsid.is_some(), kw, diags) || !count(kw, 1, 1, diags) {
                    continue;
                }
                match number_parm(kw, 0, diags).map(u16::try_from) {
                    Some(Ok(c)) if c > 0 => fk.ccsid = Some(c),
                    Some(_) => diags.push(ids::bad_parm(
                        kw.line,
                        &kw.name,
                        &kw.parms[0],
                        "must be 1 to 65535",
                    )),
                    None => {}
                }
            }
            "TEXT" => {
                if seen_before(fk.text.is_some(), kw, diags) || !count(kw, 1, 1, diags) {
                    continue;
                }
                fk.text = quoted_parm(kw, 0, 50, diags);
            }
            "DYNSLT" | "JDFTVAL" if kind == FileKind::Logical => {
                count(kw, 0, 0, diags);
                let slot = if kw.name == "DYNSLT" {
                    &mut fk.dynslt
                } else {
                    &mut fk.jdftval
                };
                if seen_before(slot.is_some(), kw, diags) {
                    continue;
                }
                *slot = Some(kw.line);
            }
            _ => level_error(kw, "file", kind, diags),
        }
    }
    if let (Some((_, _)), Some((order, line))) = (fk.unique, fk.dup_order) {
        diags.push(ids::conflict(line, order.name(), "UNIQUE"));
        fk.dup_order = None;
    }
}

fn unique(fk: &mut FileKeywords, kw: &Keyword, diags: &mut Diagnostics) {
    if seen_before(fk.unique.is_some(), kw, diags) || !count(kw, 0, 1, diags) {
        return;
    }
    let nulls = match kw.word(0) {
        None | Some("*INCNULL") => NullKeys::Include,
        Some("*EXCNULL") => NullKeys::Exclude,
        Some(_) => {
            diags.push(ids::bad_parm(
                kw.line,
                &kw.name,
                &kw.parms[0],
                "use *INCNULL or *EXCNULL",
            ));
            return;
        }
    };
    fk.unique = Some((nulls, kw.line));
}

fn dup_order(fk: &mut FileKeywords, kw: &Keyword, diags: &mut Diagnostics) {
    count(kw, 0, 0, diags);
    let order = match kw.name.as_str() {
        "FIFO" => DupOrder::Fifo,
        "LIFO" => DupOrder::Lifo,
        _ => DupOrder::Fcfo,
    };
    if let Some((prev, _)) = fk.dup_order {
        if prev == order {
            diags.push(ids::duplicate_keyword(kw.line, &kw.name));
        } else {
            diags.push(ids::conflict(kw.line, &kw.name, prev.name()));
        }
        return;
    }
    fk.dup_order = Some((order, kw.line));
}

fn reference(fk: &mut FileKeywords, kw: &Keyword, diags: &mut Diagnostics) {
    if seen_before(fk.reference.is_some(), kw, diags) || !count(kw, 1, 2, diags) {
        return;
    }
    let Some(mut r) = file_ref_parm(kw, 0, diags) else {
        return;
    };
    if let Some(p) = kw.parms.get(1) {
        let Ok(n) = p.as_word().ok_or_else(String::new).and_then(parse_name) else {
            diags.push(ids::bad_parm(kw.line, &kw.name, p, "must be a format name"));
            return;
        };
        r.format = Some(n);
    }
    fk.reference = Some(r);
}

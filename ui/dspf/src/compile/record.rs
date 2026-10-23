// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The record level: the `R` specification and its keywords. What a
//! record keyword says about fields or other records is checked once
//! the whole file is built, in `check`.

use dds::diag::{Diagnostics, ids as dds_ids};
use dds::lex::{Keyword, Spec};

use super::file::{add_key, chginpdft, cmd_key, indtxt, mnucnl};
use super::{Cx, accept, classify, field_parm, ind_text, name_of, name_parm, number_in, quoted};
use crate::cond::Entry;
use crate::diag::ids;
use crate::kw::{self, Level, SflSite};
use crate::model::{
    Clrl, FileAttrs, KeyName, KeywordUse, Record, RecordKind, SflEnd, SflSize, Slno, Subfile,
    Window, WindowOrigin,
};

/// Keywords a subfile record cannot carry.
const NOT_IN_SUBFILE: &[&str] = &[
    "WINDOW",
    "OVERLAY",
    "PUTOVR",
    "CLRL",
    "SLNO",
    "ASSUME",
    "KEEP",
    "PROTECT",
    "ERASE",
    "ERASEINP",
    "FRCDTA",
    "INZINP",
    "INZRCD",
    "UNLOCK",
    "INVITE",
    "RTNDTA",
    "RTNCSRLOC",
    "CSRLOC",
    "LOCK",
    "ALARM",
    "BLINK",
    "MNUBAR",
    "PULLDOWN",
    "RMVWDW",
    "USRRSTDSP",
    "MSGLOC",
    "RSTDSP",
    "CSRINPONLY",
    "MNUBARDSP",
    "MNUCNL",
];

/// Start a record from its `R` specification.
#[allow(clippy::too_many_lines)] // one arm per record keyword
pub(crate) fn start(
    cx: &Cx<'_>,
    spec: &Spec,
    entry: Entry,
    file: &mut FileAttrs,
    diags: &mut Diagnostics,
) -> Option<Record> {
    let Some(text) = spec.name.as_deref() else {
        diags.push(dds_ids::name_missing(spec.line, "record format"));
        return None;
    };
    let name = match name_of(text) {
        Ok(n) => n,
        Err(why) => {
            diags.push(dds_ids::bad_name(spec.line, text, why));
            return None;
        }
    };
    if !spec.attributes_blank() {
        diags.push(dds_ids::attributes_not_allowed(spec.line, "record format"));
    }
    if !spec.location.is_empty() {
        diags.push(ids::bad_location(spec.line, &spec.location));
    }
    if !spec.conditioning.is_empty() {
        diags.push(ids::no_option_indicators(spec.line, "record format"));
    }
    let mut r = Record::new(name, spec.line);
    let nsizes = cx.sizes.len();
    for k in &spec.keywords {
        let Some(info) = classify(k, Level::Record, diags) else {
            continue;
        };
        let used = accept(cx, k, info, entry, diags);
        if let Some(key) = KeyName::parse(&k.name) {
            if let Some(ck) = cmd_key(k, key, &used.cond, diags) {
                add_key(&mut r.keys, ck, diags);
            }
            r.keywords.push(used);
            continue;
        }
        if r.has(&k.name)
            && !matches!(
                k.name.as_str(),
                "SFLSIZ" | "SFLPAG" | "ERASE" | "INDTXT" | "SETOF" | "SETOFF" | "CHANGE"
            )
        {
            diags.push(dds_ids::duplicate_keyword(k.line, &k.name));
        }
        match k.name.as_str() {
            "TEXT" => r.text = quoted(k, 0, 50, diags),
            "SFL" => r.kind = RecordKind::Subfile,
            "SFLCTL" => {
                if let Some(n) = name_parm(k, 0, diags) {
                    r.kind = RecordKind::SubfileControl;
                    r.subfile = Some(Subfile {
                        record: n,
                        sizes: vec![SflSize::default(); nsizes],
                        lin: None,
                        end: None,
                        message: false,
                        record_lines: 1,
                        first_line: vec![0; nsizes],
                    });
                }
            }
            "MNUBAR" => r.kind = RecordKind::MenuBar,
            "PULLDOWN" => r.kind = RecordKind::PullDown,
            "WINDOW" => r.window = window(k, diags),
            "CLRL" => r.clrl = clrl(k, diags),
            "SLNO" => r.slno = slno(k, diags),
            "SFLMSGRCD" => {
                r.msgrcd = number_in(k, 0, 1, 27, diags).map(|n| u8::try_from(n).unwrap_or(27));
            }
            "SFLSIZ" | "SFLPAG" => sfl_size(&mut r, k, &used, diags),
            "SFLLIN" => {
                if let Some(s) = r.subfile.as_mut()
                    && let Some(n) = number_in(k, 0, 1, 132, diags)
                {
                    s.lin = Some(u16::try_from(n).unwrap_or(132));
                }
            }
            "SFLEND" => {
                if let Some(s) = r.subfile.as_mut() {
                    s.end = sflend(k, diags);
                }
            }
            "SFLDROP" | "SFLFOLD" => {
                let key = k
                    .parms
                    .first()
                    .and_then(|p| p.as_word())
                    .and_then(KeyName::parse);
                if !matches!(key, Some(KeyName::Ca(_) | KeyName::Cf(_))) {
                    diags.push(ids::bad_value(
                        k.line,
                        &k.name,
                        &k.parms[0].to_string(),
                        "CAnn or CFnn",
                    ));
                }
            }
            "SETOF" | "SETOFF" | "CHANGE" => {
                if let Some((Some(ind), _)) = ind_text(k, 0, diags) {
                    r.responses.push((ind, k.line));
                } else if k.parms.first().is_some_and(|p| p.as_word().is_none()) {
                    diags.push(ids::response_indicator(k.line, &k.name, "is required"));
                }
            }
            "SFLMSG" => {
                quoted(k, 0, 78, diags);
                if let Some((Some(ind), _)) = ind_text(k, 1, diags) {
                    r.responses.push((ind, k.line));
                }
            }
            "SFLMSGID" => {
                name_parm(k, 0, diags);
                super::file_parm(k, 1, diags);
                if let Some(p) = k.parms.get(2)
                    && let Some(w) = p.as_word()
                    && !w.starts_with('&')
                    && let Some(n) = number_in(k, 2, 1, 99, diags)
                {
                    r.responses.push((u8::try_from(n).unwrap_or(99), k.line));
                }
            }
            "SFLCSRRRN" | "SFLMODE" | "SFLSCROLL" => {
                field_parm(k, 0, diags);
            }
            "CSRLOC" => {
                name_parm(k, 0, diags);
                name_parm(k, 1, diags);
            }
            "RTNCSRLOC" => rtncsrloc(k, diags),
            "ERASE" => {
                for i in 0..k.parms.len() {
                    name_parm(k, i, diags);
                }
            }
            "ERASEINP" | "UNLOCK" => {
                if let Some(p) = k.parms.first()
                    && !matches!(p.as_word(), Some("*ALL" | "*MDTON" | "*ERASE" | "*MDTOFF"))
                {
                    diags.push(ids::bad_value(
                        k.line,
                        &k.name,
                        &p.to_string(),
                        "*ALL, *MDTON, *ERASE or *MDTOFF",
                    ));
                }
            }
            "MSGLOC" => {
                number_in(k, 0, 1, 255, diags);
            }
            "INDTXT" => indtxt(&mut file.indtxt, k, diags),
            "CHGINPDFT" => {
                chginpdft(k, diags);
            }
            "MNUCNL" => {
                if let Some(ind) = mnucnl(k, diags) {
                    r.responses.push((ind, k.line));
                }
            }
            "MNUBARDSP" => {
                for (i, p) in k.parms.iter().enumerate() {
                    if p.as_word().is_some_and(|w| w.starts_with('&')) {
                        field_parm(k, i, diags);
                    } else {
                        name_parm(k, i, diags);
                    }
                }
            }
            _ => {}
        }
        r.keywords.push(used);
    }
    sites(&r, diags);
    Some(r)
}

/// Subfile keywords on the wrong record and keywords a subfile record
/// cannot carry.
fn sites(r: &Record, diags: &mut Diagnostics) {
    for k in &r.keywords {
        let Some(info) = kw::lookup(&k.name) else {
            continue;
        };
        match info.sfl {
            SflSite::Ctl if r.kind != RecordKind::SubfileControl => {
                diags.push(ids::sfl_keyword_site(k.line, &k.name, "subfile control"));
            }
            SflSite::Sfl if r.kind != RecordKind::Subfile => {
                diags.push(ids::sfl_keyword_site(k.line, &k.name, "subfile"));
            }
            _ => {}
        }
        if r.kind == RecordKind::Subfile
            && (NOT_IN_SUBFILE.contains(&k.name.as_str()) || KeyName::parse(&k.name).is_some())
        {
            diags.push(ids::not_in_subfile(k.line, &k.name));
        }
    }
}

/// `SFLSIZ(n)` and `SFLPAG(n)`, each for the sizes its condition names.
fn sfl_size(rec: &mut Record, k: &Keyword, used: &KeywordUse, diags: &mut Diagnostics) {
    let Some(value) = number_in(k, 0, 1, 9999, diags) else {
        return;
    };
    let Some(sfl) = rec.subfile.as_mut() else {
        return;
    };
    let value = u16::try_from(value).unwrap_or(9999);
    let targets: Vec<usize> = match used.cond.size {
        Some(i) => vec![usize::from(i)],
        None => (0..sfl.sizes.len()).collect(),
    };
    for i in targets {
        let Some(slot) = sfl.sizes.get_mut(i) else {
            continue;
        };
        let cell = if k.name == "SFLSIZ" {
            &mut slot.size
        } else {
            &mut slot.page
        };
        if *cell != 0 && used.cond.size.is_some() {
            diags.push(dds_ids::duplicate_keyword(k.line, &k.name));
        }
        *cell = value;
    }
}

fn sflend(k: &Keyword, diags: &mut Diagnostics) -> Option<SflEnd> {
    let Some(p) = k.parms.first() else {
        return Some(SflEnd::Plus);
    };
    match p.as_word() {
        Some("*PLUS") => Some(SflEnd::Plus),
        Some("*MORE") => Some(SflEnd::More),
        Some("*SCRBAR") => Some(SflEnd::ScrBar),
        Some("*TEXT") => Some(SflEnd::Text),
        _ => {
            diags.push(ids::bad_value(
                k.line,
                "SFLEND",
                &p.to_string(),
                "*PLUS, *MORE, *SCRBAR or *TEXT",
            ));
            None
        }
    }
}

fn clrl(k: &Keyword, diags: &mut Diagnostics) -> Option<Clrl> {
    match k.parms[0].as_word() {
        Some("*NO") => Some(Clrl::No),
        Some("*END") => Some(Clrl::End),
        Some("*ALL") => Some(Clrl::All),
        _ => number_in(k, 0, 1, 27, diags).map(|n| Clrl::Lines(u8::try_from(n).unwrap_or(27))),
    }
}

fn slno(k: &Keyword, diags: &mut Diagnostics) -> Option<Slno> {
    match k.parms[0].as_word() {
        Some("*VAR") => Some(Slno::Var),
        _ => number_in(k, 0, 1, 27, diags).map(|n| Slno::Line(u8::try_from(n).unwrap_or(27))),
    }
}

/// `RTNCSRLOC(&rec &fld [&pos])` or `RTNCSRLOC(*RECNAME|*WINDOW|*MOUSE
/// &fld &fld [&fld])`.
fn rtncsrloc(k: &Keyword, diags: &mut Diagnostics) {
    let skip = usize::from(k.parms[0].as_word().is_some_and(|w| w.starts_with('*')));
    for i in skip..k.parms.len() {
        field_parm(k, i, diags);
    }
}

/// `WINDOW(record)`, `WINDOW(*DFT lines cols)` or `WINDOW(row col lines
/// cols)` with `&field` for row or column, then `*MSGLIN`-style options.
fn window(k: &Keyword, diags: &mut Diagnostics) -> Option<Window> {
    let words: Vec<&str> = k.parms.iter().filter_map(|p| p.as_word()).collect();
    if words.len() != k.parms.len() {
        diags.push(ids::bad_window(
            k.line,
            "parameters must be names, numbers or special values",
        ));
        return None;
    }
    if words.len() == 1 {
        return match name_of(words[0]) {
            Ok(n) => Some(Window {
                origin: WindowOrigin::From(n),
                lines: 0,
                cols: 0,
            }),
            Err(why) => {
                diags.push(ids::bad_window(k.line, &why));
                None
            }
        };
    }
    let (origin, rest) = if words[0] == "*DFT" {
        (WindowOrigin::Default, &words[1..])
    } else {
        if words.len() < 4 {
            diags.push(ids::bad_window(
                k.line,
                "a placed window takes row, column, lines and columns",
            ));
            return None;
        }
        let pos = |w: &str| -> Result<Result<u8, String>, String> {
            if let Some(f) = w.strip_prefix('&') {
                return name_of(f).map(Err);
            }
            w.parse::<u8>()
                .ok()
                .filter(|n| (1..=132).contains(n))
                .map(Ok)
                .ok_or_else(|| format!("{w} is not a line or column"))
        };
        let (row, col) = match (pos(words[0]), pos(words[1])) {
            (Ok(r), Ok(c)) => (r, c),
            (Err(why), _) | (_, Err(why)) => {
                diags.push(ids::bad_window(k.line, &why));
                return None;
            }
        };
        let origin = match (row, col) {
            (Ok(r), Ok(c)) => WindowOrigin::At(r, c),
            (Err(r), Err(c)) => WindowOrigin::Vars(r, c),
            _ => {
                diags.push(ids::bad_window(
                    k.line,
                    "row and column must both be numbers or both be fields",
                ));
                return None;
            }
        };
        (origin, &words[2..])
    };
    let dims: Vec<u8> = rest
        .iter()
        .take(2)
        .filter_map(|w| w.parse::<u8>().ok())
        .filter(|n| *n >= 1)
        .collect();
    if dims.len() != 2 {
        diags.push(ids::bad_window(
            k.line,
            "lines and columns must be numbers from 1 up",
        ));
        return None;
    }
    for w in &rest[2..] {
        if !matches!(*w, "*MSGLIN" | "*NOMSGLIN" | "*RSTCSR" | "*NORSTCSR") {
            diags.push(ids::bad_window(
                k.line,
                &format!("{w} is not a window option"),
            ));
            return None;
        }
    }
    Some(Window {
        origin,
        lines: dims[0],
        cols: dims[1],
    })
}

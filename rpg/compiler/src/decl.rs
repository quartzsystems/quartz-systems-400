// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Definitions into symbols: the type of a fixed-form `D` spec (its
//! columns, IBM's defaults) or of a free-form declaration (its type
//! keyword), `LIKE`, data structure layout (positions, `POS`,
//! `OVERLAY`, length notation, pointer alignment, `LEN`), named
//! constants, and prototypes with their parameters.

use rpgfront::Diag;
use rpgfront::ast::{Def, DefKind, Expr, Keyword, Lit, Pos};

use crate::diag::ids;
use crate::names;
use crate::syms::{ConstVal, Ds, Kind, Parm, Proto, Storage, Sym, SymId, Table};
use crate::types::{DatFmt, TimFmt, Ty};

/// What declarations need from the rest of the compile.
pub struct Cx<'a> {
    /// The symbols.
    pub t: &'a mut Table,
    /// Diagnostics.
    pub diags: &'a mut Vec<Diag>,
    /// The scope being declared into.
    pub scope: usize,
    /// The procedure's IR prefix for local names.
    pub prefix: String,
    /// The default date format (`DATFMT` on the control spec).
    pub datfmt: DatFmt,
    /// The default time format.
    pub timfmt: TimFmt,
    /// The default date separator (`'\0'` for none).
    pub datsep: char,
    /// The default time separator.
    pub timsep: char,
}

fn diag(cx: &mut Cx<'_>, id: &'static str, sev: u8, at: Pos, arg: &str) {
    cx.diags.push(Diag::new(id, sev, at).arg(arg.to_string()));
}

/// An integer argument of a keyword.
pub fn int_arg(k: &Keyword, i: usize) -> Option<i64> {
    match &k.arg(i)?.expr {
        Some(Expr::Lit(Lit::Num(n), _)) => n.parse().ok(),
        _ => None,
    }
}

/// A name argument (a bare name or a special word) of a keyword.
pub fn word_arg(k: &Keyword, i: usize) -> Option<String> {
    match &k.arg(i)?.expr {
        Some(Expr::Ref { name, steps, .. }) if steps.is_empty() => Some(name.clone()),
        Some(Expr::Special { word, .. }) => Some(word.clone()),
        _ => k.arg(i)?.toks.first().map(rpgfront::lex::Tok::text).map(|s| s.to_ascii_uppercase()),
    }
}

/// `DTAARA({*AUTO}{:*USRCTL}{:name})`: the area (a literal, `*LDA`,
/// `*PDA`, `*GDA` or a name), and whether it is read and written
/// around the program (`*AUTO`).
pub fn dtaara_of(k: &Keyword) -> (Option<String>, bool) {
    let mut area = None;
    let mut auto = false;
    for i in 0..k.args.len() {
        if let Some(s) = str_arg(k, i) {
            area = Some(s.to_ascii_uppercase());
            continue;
        }
        match word_arg(k, i).as_deref() {
            Some("*AUTO") => auto = true,
            Some("*USRCTL" | "*VAR") | None => {}
            Some(w) => area = Some(w.to_ascii_uppercase()),
        }
    }
    (area, auto)
}

/// Which `EXTPROC` argument names the procedure: the second after a
/// language word (`*CL`, `*CWIDEN`, `*CNOWIDEN`, `*JAVA`), else the first.
fn extproc_name_arg(k: &Keyword) -> usize {
    match word_arg(k, 0).as_deref() {
        Some("*CL" | "*CWIDEN" | "*CNOWIDEN" | "*JAVA") if k.arg(1).is_some() => 1,
        _ => 0,
    }
}

/// A character literal argument.
pub fn str_arg(k: &Keyword, i: usize) -> Option<String> {
    match &k.arg(i)?.expr {
        Some(Expr::Lit(Lit::Char(s), _)) => Some(s.clone()),
        _ => None,
    }
}

/// A date format and separator from keyword tokens (`*ISO`, `*MDY/`).
fn datfmt_arg(k: &Keyword) -> Option<(DatFmt, char)> {
    let a = k.arg(0)?;
    let text: String = a.toks.iter().map(rpgfront::lex::Tok::text).collect();
    crate::dtfmt::field_date(&text)
}

fn timfmt_arg(k: &Keyword) -> Option<(TimFmt, char)> {
    let a = k.arg(0)?;
    let text: String = a.toks.iter().map(rpgfront::lex::Tok::text).collect();
    crate::dtfmt::field_time(&text)
}

/// The type a definition's keywords or columns give, and its `DIM`.
/// `None` when the type comes from elsewhere (`LIKEDS`, an external
/// description) or is in error.
pub fn type_of(cx: &mut Cx<'_>, d: &Def, subfield: bool) -> Option<Ty> {
    // Free-form type keywords.
    for k in &d.kws {
        // Numbers, named constants and %LEN, %SIZE or %ELEM of what is
        // declared already.
        let nums: Vec<Option<i64>> = (0..3)
            .map(|i| int_arg(k, i).or_else(|| k.arg(i).and_then(|a| a.expr.as_ref()).and_then(|e| const_int(cx, e))))
            .collect();
        let n = |i: usize| nums.get(i).copied().flatten();
        // A prefix size other than 2 or 4 is taken as 2.
        if matches!(k.name.as_str(), "VARCHAR" | "VARUCS2" | "VARGRAPH") && n(1).is_some_and(|p| p != 2 && p != 4) {
            let at = k.arg(1).and_then(|a| a.toks.first()).map_or(k.at, |t| t.at);
            cx.diags.push(Diag::new(ids::VARYING_PREFIX, 20, at).arg(k.name.clone()));
        }
        let ty = match k.name.as_str() {
            "CHAR" => Ty::Char(len_ok(cx, k, n(0))?),
            "VARCHAR" => Ty::VarChar(len_ok(cx, k, n(0))?, prefix(n(1), n(0))),
            "UCS2" => Ty::Ucs2(len_ok(cx, k, n(0))?),
            "VARUCS2" => Ty::VarUcs2(len_ok(cx, k, n(0))?, prefix(n(1), n(0))),
            "GRAPH" => Ty::Graph(len_ok(cx, k, n(0))?),
            "VARGRAPH" => Ty::VarGraph(len_ok(cx, k, n(0))?, prefix(n(1), n(0))),
            "IND" => Ty::Ind,
            "PACKED" | "ZONED" | "BINDEC" => {
                let digits = u8::try_from(n(0)?).ok()?;
                let mut scale = u8::try_from(n(1).unwrap_or(0)).ok()?;
                // IBM reports the error and goes on with a default.
                let digits = if k.name == "BINDEC" && digits > 9 {
                    diag(cx, ids::BINDEC_DIGITS, 20, k.at, &k.name);
                    9
                } else if digits > 63 {
                    diag(cx, ids::TOO_MANY_DIGITS, 20, k.at, &k.name);
                    63
                } else if digits == 0 {
                    diag(cx, ids::ZERO_DIGITS, 20, k.at, &k.name);
                    1
                } else {
                    digits
                };
                if scale > digits {
                    diag(cx, ids::SCALE_OVER_DIGITS, 20, k.at, &k.name);
                    scale = digits;
                }
                match k.name.as_str() {
                    "PACKED" => Ty::Packed(digits, scale),
                    "ZONED" => Ty::Zoned(digits, scale),
                    _ => Ty::Bin(digits, scale),
                }
            }
            "INT" | "UNS" => {
                let digits = u8::try_from(n(0)?).ok()?;
                let digits = if [3, 5, 10, 20].contains(&digits) {
                    digits
                } else {
                    diag(cx, ids::INT_DIGITS, 20, k.at, &k.name);
                    10
                };
                if k.name == "INT" { Ty::Int(digits) } else { Ty::Uns(digits) }
            }
            "FLOAT" => {
                let b = u8::try_from(n(0)?).ok()?;
                if b != 4 && b != 8 {
                    diag(cx, ids::FLOAT_LEN, 30, k.at, &k.name);
                    return Some(Ty::Float(8));
                }
                Ty::Float(b)
            }
            "DATE" => {
                let (f, s) = match datfmt_arg(k) {
                    Some(fs) => fs,
                    None => {
                        if k.arg(0).is_some() {
                            bad_datfmt(cx, k);
                        }
                        (cx.datfmt, cx.datsep)
                    }
                };
                Ty::Date(f, s)
            }
            "TIME" => {
                let (f, s) = match timfmt_arg(k) {
                    Some(fs) => fs,
                    None => {
                        if k.arg(0).is_some() {
                            skip_bad(cx, k, ids::BAD_TIMFMT);
                        }
                        (cx.timfmt, cx.timsep)
                    }
                };
                Ty::Time(f, s)
            }
            "TIMESTAMP" => match n(0) {
                Some(p) if !(0..=12).contains(&p) => {
                    let at = k.arg(0).and_then(|a| a.toks.first()).map_or(k.at, |t| t.at);
                    cx.diags.push(Diag::new(ids::STAMP_PRECISION, 20, at));
                    Ty::Timestamp(6)
                }
                p => Ty::Timestamp(u8::try_from(p.unwrap_or(6)).unwrap_or(6)),
            },
            "POINTER" => match word_arg(k, 0).as_deref() {
                Some("*PROC") => Ty::ProcPtr,
                None => Ty::Ptr,
                Some(_) => {
                    skip_bad(cx, k, ids::POINTER_PARM);
                    Ty::Ptr
                }
            },
            "OBJECT" => Ty::Object,
            "LIKE" => return like(cx, d, k),
            _ => continue,
        };
        return Some(ty);
    }
    let Some(cols) = &d.fixed else {
        return None;
    };
    // Fixed form: the type column, the length or positions, decimals.
    let from: Option<u32> = cols.from.parse().ok();
    let to: Option<u32> = cols.to.trim_start_matches('+').parse().ok();
    let mut dec: Option<u8> = cols.decimals.parse().ok();
    // A length that is not a number is blanks; a negative one is a
    // LIKE adjustment, no length.
    let adjust = cols.to.strip_prefix('-').is_some_and(|r| !r.is_empty() && r.bytes().all(|b| b.is_ascii_digit()));
    if !cols.to.is_empty() && to.is_none() && !adjust {
        cx.diags.push(Diag::new(ids::BAD_LEN_ENTRY, 20, d.at));
    }
    let len = match (from, to) {
        (Some(f), Some(t)) if t >= f => Some(t - f + 1),
        (Some(_), Some(_)) => {
            cx.diags.push(Diag::new(ids::TO_BEFORE_FROM, 20, d.at));
            Some(1)
        }
        (None, Some(t)) => Some(t),
        _ => None,
    };
    // More decimal positions than digits: as many as the digits (by
    // length), else none.
    if let (Some(dc), Some(n)) = (dec, len)
        && !positional_digits_ok(&cols.ty, from.is_some(), n, dc)
    {
        cx.diags.push(Diag::new(ids::SCALE_OVER_DIGITS, 20, d.at));
        dec = Some(if from.is_none() { u8::try_from(n).unwrap_or(0).min(dc) } else { 0 });
    }
    let positional = from.is_some();
    let varying = d.kw("VARYING").is_some();
    // A data type IBM does not know is blank.
    let ty_col = if cols.ty.is_empty() || "ACGNPSBIUFDTZ*O".contains(cols.ty.as_str()) {
        cols.ty.as_str()
    } else {
        cx.diags.push(Diag::new(ids::BAD_TYPE_ENTRY, 20, d.at));
        ""
    };
    let code = if ty_col.is_empty() {
        if d.kw("LIKE").is_some() {
            return d.kw("LIKE").and_then(|k| like(cx, d, k));
        }
        if len.is_none() {
            return None;
        }
        if dec.is_some() {
            if subfield { "S" } else { "P" }
        } else {
            "A"
        }
    } else {
        ty_col
    };
    let at = d.at;
    let need_len = |cx: &mut Cx<'_>| -> Option<u32> {
        if len.is_none() {
            diag(cx, ids::BAD_LENGTH, 30, at, code);
        }
        len
    };
    Some(match code {
        "A" => {
            let n = need_len(cx)?;
            if varying {
                let (n, p) = if positional { (n.saturating_sub(2), 2) } else { (n, 2) };
                Ty::VarChar(n, p)
            } else {
                Ty::Char(n)
            }
        }
        "C" => {
            let n = need_len(cx)?;
            let n = if positional { n / 2 } else { n };
            if varying { Ty::VarUcs2(n, 2) } else { Ty::Ucs2(n) }
        }
        "G" => {
            let n = need_len(cx)?;
            let n = if positional { n / 2 } else { n };
            if varying { Ty::VarGraph(n, 2) } else { Ty::Graph(n) }
        }
        "N" => Ty::Ind,
        "P" => {
            let n = need_len(cx)?;
            let digits = if positional { 2 * n - 1 } else { n };
            Ty::Packed(u8::try_from(digits).ok()?, dec.unwrap_or(0))
        }
        "S" => Ty::Zoned(u8::try_from(need_len(cx)?).ok()?, dec.unwrap_or(0)),
        "B" => {
            let n = need_len(cx)?;
            let digits = if positional {
                match n {
                    2 => 4,
                    4 => 9,
                    _ => {
                        diag(cx, ids::BAD_LENGTH, 30, at, code);
                        return None;
                    }
                }
            } else {
                n
            };
            Ty::Bin(u8::try_from(digits).ok()?, dec.unwrap_or(0))
        }
        "I" | "U" => {
            let n = need_len(cx)?;
            let digits = if positional {
                match n {
                    1 => 3,
                    2 => 5,
                    4 => 10,
                    8 => 20,
                    _ => 0,
                }
            } else {
                n
            };
            let digits = u8::try_from(digits).ok()?;
            if ![3, 5, 10, 20].contains(&digits) {
                diag(cx, ids::BAD_LENGTH, 30, at, code);
                return None;
            }
            if code == "I" { Ty::Int(digits) } else { Ty::Uns(digits) }
        }
        "F" => {
            let n = need_len(cx)?;
            if n != 4 && n != 8 {
                diag(cx, ids::BAD_LENGTH, 30, at, code);
                return None;
            }
            Ty::Float(u8::try_from(n).ok()?)
        }
        "D" => {
            let (f, s) = d.kw("DATFMT").and_then(datfmt_arg).unwrap_or((cx.datfmt, cx.datsep));
            Ty::Date(f, s)
        }
        "T" => {
            let (f, s) = d.kw("TIMFMT").and_then(timfmt_arg).unwrap_or((cx.timfmt, cx.timsep));
            Ty::Time(f, s)
        }
        "Z" => Ty::Timestamp(match cols.to.trim().parse::<u32>() {
            Ok(n) if n > 20 => u8::try_from(n - 20).unwrap_or(6),
            Ok(19) => 0,
            _ => 6,
        }),
        "*" => {
            if d.kw("PROCPTR").is_some() {
                Ty::ProcPtr
            } else {
                Ty::Ptr
            }
        }
        "O" => Ty::Object,
        _ => {
            diag(cx, ids::BAD_TYPE, 30, at, code);
            return None;
        }
    })
}

/// A length argument: 1 to 16773104; IBM goes on with a default.
fn len_ok(cx: &mut Cx<'_>, k: &Keyword, n: Option<i64>) -> Option<u32> {
    let n = n.or_else(|| signed_arg(k, 0));
    match n {
        Some(v) if (1..=16_773_104).contains(&v) => u32::try_from(v).ok(),
        Some(v) if v > 16_773_104 => {
            diag(cx, ids::LEN_TOO_BIG, 20, k.at, &k.name);
            // A varying field's longest leaves room for its prefix.
            Some(if k.name == "VARCHAR" { 16_773_100 } else { 16_773_104 })
        }
        Some(_) => {
            let id = match k.name.as_str() {
                "UCS2" => ids::ZERO_LEN_UCS2,
                "GRAPH" => ids::ZERO_LEN_GRAPH,
                _ => ids::ZERO_LEN,
            };
            let at = k.arg(0).and_then(|a| a.toks.first()).map_or(k.at, |t| t.at);
            cx.diags.push(Diag::new(id, 20, at).arg(k.name.clone()));
            Some(1)
        }
        None => {
            diag(cx, ids::BAD_LENGTH, 30, k.at, &k.name);
            None
        }
    }
}

/// An integer argument written with a sign (`-1`).
fn signed_arg(k: &Keyword, i: usize) -> Option<i64> {
    let txt: String = k.arg(i)?.toks.iter().map(rpgfront::lex::Tok::text).collect();
    txt.parse().ok()
}

/// A date format that is not valid: IBM uses `*ISO` and skips the rest
/// of the keyword.
pub fn bad_datfmt(cx: &mut Cx<'_>, k: &Keyword) {
    skip_bad(cx, k, ids::BAD_DATFMT);
}

/// A keyword parameter that is not valid, the rest of it skipped.
fn skip_bad(cx: &mut Cx<'_>, k: &Keyword, id: &'static str) {
    let at = k.arg(0).and_then(|a| a.toks.first()).map_or(k.at, |t| t.at);
    cx.diags.push(Diag::new(id, 20, at).arg(k.name.clone()));
    cx.diags.push(Diag::new(ids::TEXT_SKIPPED, 0, at));
}

/// A number known when declaring: a literal, a named constant of
/// one, or `%LEN`, `%SIZE` or `%ELEM` of a literal or of something
/// declared already.
pub fn const_int(cx: &Cx<'_>, e: &Expr) -> Option<i64> {
    match e {
        Expr::Lit(Lit::Num(n), _) => n.parse().ok(),
        Expr::Ref { name, steps, .. } if steps.is_empty() => {
            let id = cx.t.find(cx.scope, name)?;
            match &cx.t.syms[id].kind {
                Some(Kind::Const(ConstVal::Expr(v))) => const_int(cx, &v.clone()),
                _ => None,
            }
        }
        Expr::Bif { name, args, .. } if args.len() == 1 => {
            let a = &args[0];
            if let Expr::Lit(Lit::Char(s), _) = a {
                return matches!(name.as_str(), "%LEN" | "%SIZE").then(|| i64::try_from(s.chars().count()).ok()).flatten();
            }
            let id = cx.t.find(cx.scope, a.name()?)?;
            let s = &cx.t.syms[id];
            match name.as_str() {
                "%ELEM" => Some(i64::from(s.dim)).filter(|&d| d > 0),
                "%SIZE" => Some(i64::from(cx.t.size(&s.ty))),
                "%LEN" => match &s.ty {
                    t if t.is_string() => t.str_len().map(i64::from),
                    t if t.is_numeric() && !matches!(t, Ty::Float(_)) => t.shape().map(|(d, _)| i64::from(d)),
                    _ => None,
                },
                _ => None,
            }
        }
        _ => None,
    }
}

/// A varying field's prefix: given, or 2 up to 65535 and 4 beyond.
fn prefix(given: Option<i64>, len: Option<i64>) -> u8 {
    match given {
        Some(4) => 4,
        Some(_) => 2,
        None if len.unwrap_or(0) > 65535 => 4,
        None => 2,
    }
}

/// `LIKE(name[:adjust])`.
fn like(cx: &mut Cx<'_>, d: &Def, k: &Keyword) -> Option<Ty> {
    let at = k.arg(0).and_then(|a| a.toks.first()).map_or(k.at, |t| t.at);
    // A qualified name: of a qualified structure's subfield.
    if let Some(Some(Expr::Ref { name, steps, .. })) = k.arg(0).map(|a| a.expr.clone())
        && let Some(rpgfront::ast::Step::Field(f)) = steps.first()
    {
        let ds = cx.t.find(cx.scope, &name).and_then(|id| match cx.t.syms[id].kind {
            Some(Kind::Ds(i)) if cx.t.ds[i].qualified => Some(i),
            _ => None,
        });
        let sub = ds.and_then(|i| cx.t.subfield(i, f)).filter(|_| steps.len() == 1);
        return match sub {
            Some(s) => Some(cx.t.syms[s].ty.clone()),
            None => {
                // The subfield, the qualified name and the definition
                // are each not defined.
                cx.diags.push(Diag::new(ids::BAD_SIMPLE_QUALIFIED, 20, at));
                diag(cx, ids::NOT_DEFINED, 30, k.at, f);
                diag(cx, ids::NOT_DEFINED, 30, k.at, &format!("{}.{}", name.to_ascii_uppercase(), f.to_ascii_uppercase()));
                diag(cx, ids::NOT_DEFINED, 30, k.at, &d.name.to_ascii_uppercase());
                None
            }
        };
    }
    let name = word_arg(k, 0)?;
    if name.eq_ignore_ascii_case(&d.name) {
        cx.diags.push(Diag::new(ids::LIKE_SELF, 20, at));
    }
    let Some(id) = cx.t.find(cx.scope, &name) else {
        // And so is the name it would have defined.
        diag(cx, ids::NOT_DEFINED, 30, k.at, &name);
        diag(cx, ids::NOT_DEFINED, 30, k.at, &d.name.to_ascii_uppercase());
        return None;
    };
    let base = cx.t.syms[id].ty.clone();
    // The adjustment is signed; without a sign IBM adds it.
    let adj = k.arg(1).and_then(|a| {
        let txt: String = a.toks.iter().map(rpgfront::lex::Tok::text).collect();
        if !txt.starts_with(['+', '-']) {
            let at = a.toks.first().map_or(k.at, |t| t.at);
            cx.diags.push(Diag::new(ids::LIKE_SIGN, 20, at));
        }
        txt.parse::<i64>().ok()
    });
    let _ = d;
    Some(match (base, adj) {
        (t, None) => t,
        (Ty::Char(n), Some(a)) => Ty::Char(adjust(n, a)?),
        (Ty::VarChar(n, p), Some(a)) => Ty::VarChar(adjust(n, a)?, p),
        (Ty::Packed(dg, s), Some(a)) => Ty::Packed(u8::try_from(adjust(u32::from(dg), a)?).ok()?, s),
        (Ty::Zoned(dg, s), Some(a)) => Ty::Zoned(u8::try_from(adjust(u32::from(dg), a)?).ok()?, s),
        (t, Some(_)) => t,
    })
}

fn adjust(n: u32, a: i64) -> Option<u32> {
    u32::try_from(i64::from(n) + a).ok().filter(|&v| v > 0)
}

/// Definition keywords, the free-form types among them.
const DEF_KEYWORDS: &[&str] = &[
    "ALIAS", "ALIGN", "ALT", "ALTSEQ", "ASCEND", "BASED", "BINDEC", "CCSID", "CHAR", "CLASS", "CONST",
    "CTDATA", "DATE", "DATFMT", "DESCEND", "DIM", "DTAARA", "EXPORT", "EXT", "EXTFLD", "EXTFMT", "EXTNAME",
    "EXTPGM", "EXTPROC", "FLOAT", "FROMFILE", "GRAPH", "IMPORT", "IND", "INT", "INZ", "LEN", "LIKE",
    "LIKEDS", "LIKEFILE", "LIKEREC", "NOOPT", "NULLIND", "OBJECT", "OCCURS", "OPDESC", "OPTIONS",
    "OVERLAY", "OVERLOAD", "PACKED", "PACKEVEN", "PERRCD", "POINTER", "POS", "PREFIX", "PROCPTR", "PSDS",
    "QUALIFIED", "REQPROTO", "RTNPARM", "SAMEPOS", "SQLTYPE", "STATIC", "TEMPLATE", "TIME", "TIMESTAMP",
    "TIMFMT", "TOFILE", "UCS2", "UNS", "VALUE", "VARCHAR", "VARGRAPH", "VARUCS2", "VARYING", "ZONED",
];

/// The special subfields of a program status structure.
const PSDS_SUBFIELDS: &[&str] = &["*PROC", "*STATUS", "*ROUTINE", "*PARMS"];

/// The special subfields of a file information structure.
const INFDS_SUBFIELDS: &[&str] = &["*FILE", "*STATUS", "*OPCODE", "*ROUTINE", "*RECORD", "*SIZE", "*INP", "*OUT", "*MODE"];

/// Keywords not allowed after another, with IBM's identifier.
const EXCLUSIVE: &[(&str, &str, &str)] = &[
    ("EXPORT", "IMPORT", ids::KEYWORD_EXCLUDED),
    ("IMPORT", "EXPORT", ids::KEYWORD_EXCLUDED),
    ("ASCEND", "DESCEND", ids::AFTER_ASCEND),
    ("DESCEND", "ASCEND", ids::AFTER_ASCEND),
    ("BASED", "INZ", ids::AFTER_BASED),
    ("BASED", "EXPORT", ids::AFTER_BASED),
    ("BASED", "IMPORT", ids::AFTER_BASED),
    ("OCCURS", "DIM", ids::AFTER_OCCURS),
    ("POS", "OVERLAY", ids::AFTER_POS),
    ("EXTPGM", "EXTPROC", ids::PR_EXCLUDED),
    ("EXTPROC", "EXTPGM", ids::PR_EXCLUDED),
    ("VALUE", "CONST", ids::AFTER_VALUE),
];

/// The values `OPTIONS` takes.
const PARM_OPTIONS: &[&str] = &["*NOPASS", "*OMIT", "*VARSIZE", "*STRING", "*TRIM", "*RIGHTADJ", "*NULLIND", "*CONVERT", "*EXACT"];

/// Keywords that take a parameter.
const NEEDS_PARM: &[&str] = &[
    "ALT", "BASED", "BINDEC", "CCSID", "CHAR", "CLASS", "DATFMT", "DIM", "EXTFMT", "EXTNAME",
    "FLOAT", "FROMFILE", "GRAPH", "INT", "LEN", "LIKE", "LIKEDS", "LIKEFILE", "LIKEREC", "OCCURS",
    "OPTIONS", "OVERLAY", "PACKED", "PERRCD", "POS", "PREFIX", "SQLTYPE", "TIMFMT", "TOFILE", "UCS2", "UNS",
    "VARCHAR", "VARGRAPH", "VARUCS2", "ZONED",
];

/// Keywords a standalone field does not take.
const NOT_FOR_FIELD: &[&str] = &["EXTFLD", "OVERLAY", "POS", "EXTNAME", "OCCURS", "PREFIX", "PSDS", "QUALIFIED"];

/// A definition's keywords: each known, given once, with a parameter
/// when it takes one, and not excluded by another; a constant takes
/// none. IBM ignores such a keyword: the definition comes back without
/// it. `ds` is the structure of a subfield.
pub fn check_keywords(cx: &mut Cx<'_>, d: &Def, ds: Option<&Def>) -> Def {
    let mut seen: Vec<&str> = Vec::new();
    let mut keep = Vec::new();
    let psds = ds.is_some_and(|p| p.kw("PSDS").is_some() || p.fixed.as_ref().is_some_and(|c| c.dstype == "S"))
        && !cx.t.ds.iter().any(|x| x.psds);
    let infds = ds.is_some_and(|p| cx.t.infds.contains(&p.name.to_ascii_uppercase()));
    for k in &d.kws {
        let n = k.name.as_str();
        let special = d.kind == DefKind::Subfield
            && (psds && PSDS_SUBFIELDS.contains(&n) || infds && INFDS_SUBFIELDS.contains(&n));
        let bad = if d.kind == DefKind::Const {
            // A second value is the constant's value twice.
            let id = if n.starts_with(|c: char| c.is_ascii_alphabetic() || c == '*') { ids::CONST_KEYWORD } else { ids::KEYWORD_TWICE };
            Some(Diag::new(id, 20, k.at))
        } else if !DEF_KEYWORDS.contains(&n) && !special {
            Some(Diag::new(ids::UNKNOWN_DEF_KEYWORD, 20, k.at).arg(n))
        } else if seen.contains(&n) {
            Some(Diag::new(ids::KEYWORD_TWICE, 20, k.at).arg(n))
        } else if let Some((first, _, id)) = EXCLUSIVE.iter().find(|(a, b, _)| *b == n && seen.contains(a)) {
            Some(Diag::new(id, 20, k.at).arg(*first))
        } else if NEEDS_PARM.contains(&n) && k.args.is_empty() {
            Some(Diag::new(ids::NEEDS_PARM, 20, k.at).arg(n))
        } else if d.kind == DefKind::Standalone && NOT_FOR_FIELD.contains(&n) {
            Some(Diag::new(ids::NOT_FOR_FIELD, 20, k.at).arg(n))
        } else if matches!(n, "EXPORT" | "IMPORT") && cx.scope != 0 {
            Some(Diag::new(ids::NOT_IN_PROC, 20, k.at).arg(n))
        } else if n == "OPTIONS" && (0..k.args.len()).any(|i| word_arg(k, i).is_none_or(|w| !PARM_OPTIONS.contains(&w.as_str()))) {
            let at = k.arg(0).and_then(|a| a.toks.first()).map_or(k.at, |t| t.at);
            cx.diags.push(Diag::new(ids::OPTIONS_PARM, 20, at));
            Some(Diag::new(ids::TEXT_SKIPPED, 0, at))
        } else if n == "OPTIONS" && seen.contains(&"VALUE")
            && let Some(i) = (0..k.args.len()).find(|&i| matches!(word_arg(k, i).as_deref(), Some("*OMIT" | "*VARSIZE" | "*NULLIND")))
        {
            let at = k.arg(i).and_then(|a| a.toks.first()).map_or(k.at, |t| t.at);
            Some(Diag::new(ids::OPTIONS_VALUE, 20, at))
        } else if n == "EXTPROC" && matches!(k.arg(extproc_name_arg(k)).and_then(|a| a.expr.as_ref()), Some(Expr::Lit(Lit::Num(_), _))) {
            let at = k.arg(0).and_then(|a| a.toks.first()).map_or(k.at, |t| t.at);
            Some(Diag::new(ids::EXTPROC_PARM, 20, at))
        } else if n == "EXTPGM" && word_arg(k, 0).is_some_and(|w| w.starts_with('*')) {
            let at = k.arg(0).and_then(|a| a.toks.first()).map_or(k.at, |t| t.at);
            cx.diags.push(Diag::new(ids::EXTPGM_PARM, 20, at));
            Some(Diag::new(ids::TEXT_SKIPPED, 0, at))
        } else if n == "STATIC" && cx.scope == 0 {
            Some(Diag::new(ids::NOT_IN_MAIN_SOURCE, 20, k.at).arg(n))
        } else if n == "VARYING" && d.fixed.is_none() {
            Some(Diag::new(ids::FIXED_ONLY_KEYWORD, 20, k.at).arg(n))
        } else if n == "CTDATA" && d.kw("DIM").is_none() {
            Some(Diag::new(ids::CTDATA_NOT_ARRAY, 20, k.at))
        } else if matches!(n, "EXPORT" | "IMPORT") && k.arg(0).is_some() && str_arg(k, 0).is_none() && word_arg(k, 0).as_deref() != Some("*DCLCASE") {
            let at = k.arg(0).and_then(|a| a.toks.first()).map_or(k.at, |t| t.at);
            Some(Diag::new(ids::EXPORT_PARM, 20, at).arg(n))
        } else {
            None
        };
        seen.push(n);
        match bad {
            // A bad external name leaves the keyword without it.
            Some(x) if x.msgid == ids::EXPORT_PARM => {
                cx.diags.push(x);
                keep.push(Keyword { args: Vec::new(), ..k.clone() });
            }
            Some(x) => cx.diags.push(x),
            None => keep.push(k.clone()),
        }
    }
    Def { kws: keep, ..d.clone() }
}

/// Once a parameter is `OPTIONS(*NOPASS)`, every one after it is too.
pub fn nopass_order(cx: &mut Cx<'_>, d: &Def) {
    let nopass = |c: &Def| c.kw("OPTIONS").is_some_and(|k| (0..k.args.len()).any(|i| word_arg(k, i).as_deref() == Some("*NOPASS")));
    let mut after = false;
    for c in &d.children {
        if nopass(c) {
            after = true;
        } else if after {
            cx.diags.push(Diag::new(ids::NOPASS_MISSING, 20, c.at).arg(c.name.to_ascii_uppercase()));
        }
    }
}

/// The IR name of a symbol in the current scope.
fn ir_name(cx: &Cx<'_>, name: &str) -> String {
    format!("${}{}", cx.prefix, names::mangle(name))
}

/// Declare one definition of a scope: a standalone field, constant,
/// data structure (with subfields), or prototype.
pub fn declare(cx: &mut Cx<'_>, d: &Def) -> Option<SymId> {
    let mut d = check_keywords(cx, d, None);
    if matches!(d.kind, DefKind::Ds | DefKind::Pr) {
        let parent = d.clone();
        for c in &mut d.children {
            *c = check_keywords(cx, c, (parent.kind == DefKind::Ds).then_some(&parent));
        }
    }
    if d.kind == DefKind::Pr {
        nopass_order(cx, &d);
    }
    let d = &d;
    match d.kind {
        DefKind::Const => {
            // A constant without a value is ignored.
            let Some(v) = d.value.clone() else {
                diag(cx, ids::CONST_NO_VALUE, 30, d.at, &d.name.to_ascii_uppercase());
                diag(cx, ids::NOT_DEFINED, 30, d.at, &d.name.to_ascii_uppercase());
                return None;
            };
            let s = Sym {
                name: d.name.to_ascii_uppercase(),
                kind: Some(Kind::Const(ConstVal::Expr(v))),
                scope: cx.scope,
                def_line: d.at.line,
                ..Sym::default()
            };
            add(cx, s, d.at)
        }
        DefKind::Standalone => {
            let before = cx.diags.len();
            let ty = type_of(cx, d, false);
            if ty.is_none() && cx.diags[before..].iter().all(|x| x.msgid == ids::BAD_LEN_ENTRY) {
                no_type(cx, d.at, &d.name.to_ascii_uppercase(), d.fixed.is_some());
            }
            let ty = ty.unwrap_or(Ty::Unknown);
            let dim = dim_of(cx, d);
            let storage = storage_of(cx, d);
            let s = Sym {
                name: d.name.to_ascii_uppercase(),
                kind: Some(Kind::Field),
                ty,
                dim,
                ir: ir_name(cx, &d.name),
                scope: cx.scope,
                storage,
                inz: d.kw("INZ").map(|k| k.arg(0).and_then(|a| a.expr.clone())),
                def_line: d.at.line,
                template: d.kw("TEMPLATE").is_some(),
                ctdata: d.kw("CTDATA").is_some(),
                perrcd: d.kw("PERRCD").and_then(|k| int_arg(k, 0)).and_then(|v| u32::try_from(v).ok()).unwrap_or(1),
                table: d.name.to_ascii_uppercase().starts_with("TAB"),
                alt: d.kw("ALT").and_then(|k| word_arg(k, 0)).unwrap_or_default(),
                vardim: match d.kw("DIM").and_then(|k| word_arg(k, 0)).as_deref() {
                    Some("*AUTO") => 1,
                    Some("*VAR") => 2,
                    _ => 0,
                },
                seq: if d.kw("ASCEND").is_some() { 1 } else if d.kw("DESCEND").is_some() { 2 } else { 0 },
                nullind: d.kw("NULLIND").is_some(),
                altseq_none: d.kw("ALTSEQ").and_then(|k| word_arg(k, 0)).as_deref() == Some("*NONE"),
                fromfile: d.kw("FROMFILE").and_then(|k| word_arg(k, 0)).unwrap_or_default(),
                tofile: d.kw("TOFILE").and_then(|k| word_arg(k, 0)).unwrap_or_default(),
                external: d
                    .kw("EXPORT")
                    .or_else(|| d.kw("IMPORT"))
                    .map(|k| str_arg(k, 0).unwrap_or_else(|| d.name.to_ascii_uppercase()))
                    .unwrap_or_default(),
                ..Sym::default()
            };
            let id = add(cx, s, d.at)?;
            if let Some(k) = d.kw("DTAARA") {
                let (area, auto) = dtaara_of(k);
                cx.t.dtaaras.insert(id, area.unwrap_or_else(|| d.name.to_ascii_uppercase()));
                if auto {
                    cx.t.dtaara_auto.insert(id);
                }
            }
            Some(id)
        }
        DefKind::Ds => declare_ds(cx, d),
        DefKind::Pr => {
            let p = proto(cx, d);
            let idx = cx.t.protos.len();
            cx.t.protos.push(p);
            let s = Sym {
                name: d.name.to_ascii_uppercase(),
                kind: Some(Kind::Proto(idx)),
                scope: cx.scope,
                def_line: d.at.line,
                ..Sym::default()
            };
            add(cx, s, d.at)
        }
        // The main procedure's interface: its parameters are fields the
        // program entry points at its caller's storage.
        DefKind::Pi if cx.scope == 0 => {
            for c in &d.children {
                let mut f = c.clone();
                f.kind = DefKind::Standalone;
                if declare(cx, &f).is_some() {
                    cx.t.entry_names.push(c.name.to_ascii_uppercase());
                }
            }
            None
        }
        _ => None,
    }
}

fn add(cx: &mut Cx<'_>, s: Sym, at: Pos) -> Option<SymId> {
    let name = s.name.clone();
    match cx.t.add(s) {
        Ok(id) => Some(id),
        Err(_) => {
            diag(cx, ids::DUPLICATE, 30, at, &name);
            None
        }
    }
}

/// `DIM(n)`: the element count.
fn dim_of(cx: &mut Cx<'_>, d: &Def) -> u32 {
    let Some(k) = d.kw("DIM") else { return 0 };
    if matches!(word_arg(k, 0).as_deref(), Some("*AUTO" | "*VAR")) {
        return int_arg(k, 1).and_then(|n| u32::try_from(n).ok()).unwrap_or(0);
    }
    match int_arg(k, 0).or_else(|| signed_arg(k, 0)) {
        Some(n) if (1..=16_773_104).contains(&n) => u32::try_from(n).unwrap_or(0),
        Some(_) => {
            diag(cx, ids::DIM_RANGE, 20, k.at, "DIM");
            1
        }
        None => {
            // `DIM(%ELEM(x))` and named constants are resolved here.
            if let Some(Some(Expr::Ref { name, .. })) = k.arg(0).map(|a| a.expr.clone())
                && let Some(id) = cx.t.find(cx.scope, &name)
                && let Some(Kind::Const(ConstVal::Expr(Expr::Lit(Lit::Num(n), _)))) = &cx.t.syms[id].kind
            {
                return n.parse().unwrap_or(0);
            }
            if let Some(Some(Expr::Bif { name, args, .. })) = k.arg(0).map(|a| a.expr.clone())
                && name == "%ELEM"
                && let Some(Expr::Ref { name: n, .. }) = args.first()
                && let Some(id) = cx.t.find(cx.scope, n)
            {
                return cx.t.syms[id].dim;
            }
            diag(cx, ids::BAD_KEYWORD_VALUE, 30, k.at, "DIM");
            0
        }
    }
}

fn storage_of(cx: &Cx<'_>, d: &Def) -> Storage {
    if let Some(k) = d.kw("BASED")
        && let Some(p) = word_arg(k, 0)
        && let Some(id) = cx.t.find(cx.scope, &p)
    {
        return Storage::Based(id);
    }
    if d.kw("EXPORT").is_some() {
        return Storage::Export;
    }
    if d.kw("IMPORT").is_some() {
        return Storage::Import;
    }
    if cx.scope != 0 && d.kw("STATIC").is_none() {
        return Storage::Auto;
    }
    Storage::Static
}

/// A definition without a type: IBM asks for LIKE (in fixed form, or
/// when the name was not valid either) and names it as not defined.
fn no_type(cx: &mut Cx<'_>, at: Pos, name: &str, fixed: bool) {
    if fixed || name.is_empty() {
        cx.diags.push(Diag::new(ids::NO_TYPE, 30, at).arg(name));
    }
    if !name.is_empty() {
        cx.diags.push(Diag::new(ids::NOT_DEFINED, 30, at).arg(name));
    }
}

/// Whether a fixed-form numeric field's decimals fit its digits.
fn positional_digits_ok(ty: &str, positional: bool, len: u32, dec: u8) -> bool {
    let digits = match (ty, positional) {
        ("P" | "", true) => 2 * len - 1,
        ("B", true) => if len == 2 { 4 } else { 9 },
        ("I" | "U" | "F" | "A" | "D" | "T" | "Z" | "N" | "*" | "G" | "C" | "O", _) => return true,
        _ => len,
    };
    u32::from(dec) <= digits
}

/// A data structure and its subfields.
fn declare_ds(cx: &mut Cx<'_>, d: &Def) -> Option<SymId> {
    let unnamed = d.name.is_empty() || d.name == "*N";
    let name = if unnamed {
        format!("_DS{}", cx.t.ds.len() + 1)
    } else {
        d.name.to_ascii_uppercase()
    };
    let mut qualified = d.kw("QUALIFIED").is_some() || d.kw("LIKEDS").is_some() || d.kw("LIKEREC").is_some();
    let ds_index = cx.t.ds.len();
    let dim = dim_of(cx, d);
    // An array of structures must be qualified; IBM keeps its
    // subfields to it.
    let unqualified_array = dim > 0 && !qualified;
    if unqualified_array {
        let at = d.kw("DIM").map_or(d.at, |k| k.at);
        cx.diags.push(Diag::new(ids::DS_DIM_UNQUALIFIED, 20, at));
        qualified = true;
    }
    let storage = storage_of(cx, d);
    let ir = ir_name(cx, &name);
    let fixed_len: Option<u32> = d.fixed.as_ref().and_then(|c| {
        if c.from.is_empty() { c.to.parse().ok() } else { None }
    });
    // LEN(0) is no length.
    let len_kw = d.kw("LEN").and_then(|k| int_arg(k, 0)).and_then(|v| u32::try_from(v).ok()).filter(|&v| v > 0);
    let occurs = match d.kw("OCCURS") {
        Some(k) => match int_arg(k, 0).or_else(|| k.arg(0).and_then(|a| a.expr.as_ref()).and_then(|e| const_int(cx, e))) {
            Some(n) if n >= 1 => u32::try_from(n).unwrap_or(0),
            _ => {
                let at = k.arg(0).and_then(|a| a.toks.first()).map_or(k.at, |t| t.at);
                cx.diags.push(Diag::new(ids::OCCURS_PARM, 20, at));
                0
            }
        },
        None => 0,
    };
    // One program status structure; a second is an ordinary one.
    let mut psds = d.fixed.as_ref().is_some_and(|c| c.dstype == "S") || d.kw("PSDS").is_some();
    if psds && cx.t.ds.iter().any(|x| x.psds) {
        let at = d.kw("PSDS").map_or(d.at, |k| k.at);
        cx.diags.push(Diag::new(ids::PSDS_TWICE, 40, at));
        psds = false;
    }
    // INZ(*LIKEDS) only with LIKEDS.
    if d.kw("LIKEDS").is_none()
        && let Some(k) = d.kw("INZ")
        && word_arg(k, 0).as_deref() == Some("*LIKEDS")
    {
        cx.diags.push(Diag::new(ids::INZ_LIKEDS, 20, d.at));
    }
    let mut ds = Ds {
        len: 0,
        qualified,
        unqualified_array,
        inz: d.kw("INZ").is_some(),
        occurs,
        psds,
        dtaara: d
            .kw("DTAARA")
            .map(|k| dtaara_of(k).0.unwrap_or_else(|| name.clone()))
            .or_else(|| (d.fixed.as_ref().is_some_and(|c| c.dstype == "U")).then(|| "*LDA".to_string())),
        ir_type: format!("{}DS_{}", cx.prefix.to_ascii_lowercase(), names::mangle(&name)),
        ..Ds::default()
    };
    // LIKEDS: copy the other structure's layout.
    let like = d.kw("LIKEDS").and_then(|k| word_arg(k, 0));
    let ds_sym = Sym {
        name: name.clone(),
        kind: Some(Kind::Ds(ds_index)),
        ty: Ty::Ds(ds_index),
        dim,
        ir: ir.clone(),
        scope: cx.scope,
        storage,
        inz: d.kw("INZ").map(|_| None),
        def_line: d.at.line,
        template: d.kw("TEMPLATE").is_some(),
        ..Sym::default()
    };
    let id = if unnamed {
        Some(cx.t.add_hidden(ds_sym))
    } else {
        add(cx, ds_sym, d.at)
    }?;
    ds.sym = id;
    if let Some(area) = &ds.dtaara {
        cx.t.dtaaras.insert(id, area.to_ascii_uppercase());
        let auto = d.kw("DTAARA").is_some_and(|k| dtaara_of(k).1) || d.fixed.as_ref().is_some_and(|c| c.dstype == "U");
        if auto {
            cx.t.dtaara_auto.insert(id);
        }
    }
    cx.t.ds.push(ds);
    if let Some(other) = like {
        let Some(oid) = cx.t.find(cx.scope, &other) else {
            diag(cx, ids::NOT_DEFINED, 30, d.at, &other);
            diag(cx, ids::NOT_DEFINED, 30, d.at, &d.name.to_ascii_uppercase());
            return Some(id);
        };
        let Some(Kind::Ds(oi)) = cx.t.syms[oid].kind.clone() else {
            diag(cx, ids::NOT_A_DS, 30, d.at, &other);
            return Some(id);
        };
        let src = cx.t.ds[oi].clone();
        // The other structure's initial values come along only with
        // INZ(*LIKEDS); otherwise the copy starts blank (or, with INZ,
        // each subfield at its type's default).
        let inz_likeds = d.kw("INZ").and_then(|k| word_arg(k, 0)).is_some_and(|w| w == "*LIKEDS");
        let mut subs = Vec::new();
        for &s in &src.subfields {
            let mut c = cx.t.syms[s].clone();
            c.parent = Some(id);
            c.scope = cx.scope;
            c.ir = String::new();
            if !inz_likeds {
                c.inz = None;
            }
            subs.push(cx.t.add_hidden(c));
        }
        let dsr = &mut cx.t.ds[ds_index];
        dsr.subfields = subs;
        dsr.len = src.len;
        dsr.has_ptr = src.has_ptr;
        dsr.ir_type = src.ir_type;
        if inz_likeds {
            dsr.inz |= src.inz;
        }
        return Some(id);
    }
    // EXTNAME and LIKEREC: the format's fields.
    if let Some((subs, len)) = crate::extfiles::ds_subfields(cx, d) {
        for (n, ty, off) in subs {
            let sub = Sym {
                name: n.clone(),
                kind: Some(Kind::Subfield),
                ty,
                scope: cx.scope,
                storage: Storage::InDs,
                parent: Some(id),
                offset: off,
                def_line: d.at.line,
                implicit: true,
                ..Sym::default()
            };
            let sid = if qualified {
                Some(cx.t.add_hidden(sub))
            } else {
                match cx.t.find_in(cx.scope, &n) {
                    // A file's field of the same name is this subfield.
                    Some(old) if cx.t.syms[old].implicit && cx.t.syms[old].kind == Some(Kind::Field) => {
                        cx.t.syms[old].kind = None;
                        let sid = cx.t.add_hidden(sub);
                        cx.t.rebind(cx.scope, &n, sid);
                        Some(sid)
                    }
                    _ => add(cx, sub, d.at),
                }
            };
            if let Some(s) = sid {
                cx.t.ds[ds_index].subfields.push(s);
            }
        }
        cx.t.ds[ds_index].len = len_kw.or(fixed_len).unwrap_or(0).max(len).max(1);
        return Some(id);
    }
    // Subfields.
    let mut next: u32 = 0;
    let mut end: u32 = 0;
    let mut overlay_next: std::collections::HashMap<SymId, u32> = std::collections::HashMap::new();
    let is_psds = cx.t.ds[ds_index].psds;
    for c in &d.children {
        // A to-position past the structure's given length is the
        // from-position.
        let clipped;
        let c = match (len_kw.or(fixed_len), &c.fixed) {
            (Some(given), Some(f))
                if f.from.parse::<u32>().is_ok() && f.to.parse::<u32>().is_ok_and(|t| t > given) =>
            {
                cx.diags.push(Diag::new(ids::TO_PAST_DS, 20, c.at));
                let mut n = c.clone();
                if let Some(nf) = n.fixed.as_mut() {
                    nf.to.clone_from(&nf.from);
                }
                clipped = n;
                &clipped
            }
            _ => c,
        };
        let cname = c.name.to_ascii_uppercase();
        // A subfield named twice in its structure is ignored, but for
        // the room it takes.
        let twice = !cname.is_empty() && cname != "*N" && cx.t.ds[ds_index].subfields.iter().any(|&s| cx.t.syms[s].name == cname);
        if twice {
            cx.diags.push(Diag::new(ids::SUBFIELD_TWICE, 20, c.at).arg(cname.clone()));
        }
        let special = crate::extfiles::special_subfield(c, is_psds);
        let ty = match &special {
            Some((t, _)) => Some(t.clone()),
            None => type_of(cx, c, true),
        };
        let sdim = dim_of(cx, c);
        // A nested LIKEDS gets its own copy of the subfields.
        let mut nested_copy = None;
        // PACKEVEN: a packed subfield of positions has an even number of
        // digits, one fewer.
        let ty = match (ty, c.kw("PACKEVEN")) {
            (Some(Ty::Packed(dg, sc)), Some(_)) if dg > 1 => Some(Ty::Packed(dg - 1, sc)),
            (t, Some(k)) => {
                cx.diags.push(Diag::new(ids::PACKEVEN_NOT_PACKED, 20, k.at));
                t
            }
            (t, None) => t,
        };
        let (ty, size) = match ty {
            Some(t) => {
                let one = cx.t.size(&t);
                (t, one * sdim.max(1))
            }
            None => {
                // A nested LIKEDS subfield.
                if let Some(k) = c.kw("LIKEDS")
                    && let Some(n) = word_arg(k, 0)
                    && let Some(oid) = cx.t.find(cx.scope, &n)
                    && let Some(Kind::Ds(oi)) = cx.t.syms[oid].kind.clone()
                {
                    let len = cx.t.ds[oi].len;
                    let mut copy = cx.t.ds[oi].clone();
                    let ni = cx.t.ds.len();
                    copy.subfields = Vec::new();
                    cx.t.ds.push(copy);
                    for s in cx.t.ds[oi].subfields.clone() {
                        let mut sub = cx.t.syms[s].clone();
                        sub.scope = cx.scope;
                        sub.ir = String::new();
                        let sid = cx.t.add_hidden(sub);
                        cx.t.ds[ni].subfields.push(sid);
                    }
                    nested_copy = Some(ni);
                    (Ty::Ds(ni), len * sdim.max(1))
                } else {
                    // A LIKE that names nothing was reported already.
                    if c.kw("LIKE").is_none() {
                        no_type(cx, c.at, &cname, c.fixed.is_some());
                    }
                    (Ty::Unknown, 0)
                }
            }
        };
        let is_ptr = matches!(ty, Ty::Ptr | Ty::ProcPtr | Ty::Object)
            || matches!(&ty, Ty::Ds(i) if cx.t.ds[*i].has_ptr);
        // Where it goes.
        let from = c
            .fixed
            .as_ref()
            .and_then(|f| f.from.parse::<u32>().ok())
            .or_else(|| special.as_ref().map(|(_, p)| *p));
        let pos_kw = c.kw("POS").and_then(|k| {
            match int_arg(k, 0).or_else(|| signed_arg(k, 0)) {
                Some(v) if (1..=16_773_104).contains(&v) => u32::try_from(v).ok(),
                _ => {
                    let at = k.arg(0).and_then(|a| a.toks.first()).map_or(k.at, |t| t.at);
                    cx.diags.push(Diag::new(ids::POS_RANGE, 20, at));
                    None
                }
            }
        });
        let overlay = c.kw("OVERLAY").cloned();
        let samepos = c.kw("SAMEPOS").and_then(|k| word_arg(k, 0)).and_then(|n| cx.t.subfield(ds_index, &n));
        let offset = if let Some((_, o)) = special {
            o
        } else if let Some(s) = samepos {
            cx.t.syms[s].offset
        } else if let Some(f) = from.or(pos_kw) {
            f.saturating_sub(1)
        } else if let Some(k) = overlay {
            let target = word_arg(&k, 0).unwrap_or_default();
            let base_sym = cx.t.subfield(ds_index, &target);
            let base_off = base_sym.map_or(0, |s| cx.t.syms[s].offset);
            let arg_at = k.arg(0).and_then(|a| a.toks.first()).map_or(k.at, |t| t.at);
            if target != name && base_sym.is_none() {
                diag(cx, ids::NOT_DEFINED, 30, k.at, &target);
                cx.diags.push(Diag::new(ids::OVERLAY_TARGET, 20, arg_at));
            }
            // Free form overlays its own structure with POS.
            if target == name && c.fixed.is_none() {
                cx.diags.push(Diag::new(ids::OVERLAY_SELF, 20, arg_at));
            }
            match k.arg(1).map(|a| a.toks.iter().map(rpgfront::lex::Tok::text).collect::<String>().to_ascii_uppercase()) {
                Some(p) if p == "*NEXT" => {
                    let key = base_sym.unwrap_or(id);
                    let at = *overlay_next.get(&key).unwrap_or(&base_off);
                    overlay_next.insert(key, at + size);
                    at
                }
                Some(p) => {
                    let n: u32 = p.parse().unwrap_or(1);
                    let at = base_off + n.saturating_sub(1);
                    let key = base_sym.unwrap_or(id);
                    overlay_next.insert(key, at + size);
                    at
                }
                // In free form IBM places it as if not overlaid.
                None if target == name && c.fixed.is_none() => {
                    let at = if is_ptr { next.div_ceil(16) * 16 } else { next };
                    next = at + size;
                    at
                }
                None => {
                    let key = base_sym.unwrap_or(id);
                    overlay_next.insert(key, base_off + size);
                    base_off
                }
            }
        } else {
            // ALIGN: integers and floats on a multiple of their size.
            let align = match &ty {
                Ty::Int(_) | Ty::Uns(_) | Ty::Float(_) if d.kw("ALIGN").is_some() => cx.t.size(&ty).max(1),
                _ => 1,
            };
            let at = if is_ptr { next.div_ceil(16) * 16 } else { next.div_ceil(align) * align };
            next = at + size;
            at
        };
        // An absolute position moves the next one past it too; an
        // overlay does not.
        if c.kw("OVERLAY").is_none() && samepos.is_none() {
            next = next.max(offset + size);
        }
        end = end.max(offset + size);
        if is_ptr {
            cx.t.ds[ds_index].has_ptr = true;
        }
        if twice {
            continue;
        }
        let sub = Sym {
            name: cname.clone(),
            kind: Some(Kind::Subfield),
            ty,
            dim: sdim,
            ir: String::new(),
            scope: cx.scope,
            storage: Storage::InDs,
            parent: Some(id),
            offset,
            seq: if c.kw("ASCEND").is_some() { 1 } else if c.kw("DESCEND").is_some() { 2 } else { 0 },
            inz: c.kw("INZ").map(|k| k.arg(0).and_then(|a| a.expr.clone())),
            def_line: c.at.line,
            ..Sym::default()
        };
        let sid = if qualified || cname.is_empty() || cname == "*N" {
            Some(cx.t.add_hidden(sub))
        } else {
            add(cx, sub, c.at)
        };
        if let Some(s) = sid {
            cx.t.ds[ds_index].subfields.push(s);
            if let Some(ni) = nested_copy {
                cx.t.ds[ni].sym = s;
                for f in cx.t.ds[ni].subfields.clone() {
                    cx.t.syms[f].parent = Some(s);
                }
            }
        }
    }
    // A subfield past a given length.
    if let Some(given) = len_kw.or(fixed_len) {
        for &s in &cx.t.ds[ds_index].subfields.clone() {
            let sym = &cx.t.syms[s];
            let size = cx.t.size(&sym.ty) * sym.dim.max(1);
            if sym.offset + size > given {
                let at = Pos { line: sym.def_line, col: 0 };
                cx.diags.push(Diag::new(ids::PAST_DS_LEN, 30, at).arg(sym.name.clone()));
            }
        }
    }
    // A given length stands even when the subfields pass it.
    let len = len_kw.or(fixed_len).unwrap_or(end).max(1);
    if d.children.is_empty() && d.kw("EXTNAME").is_none() && d.kw("LIKEREC").is_none() && d.fixed.as_ref().is_none_or(|c| c.ext != "E") {
        diag(cx, ids::DS_NO_SUBFIELDS, 0, d.at, &name);
    }
    cx.t.ds[ds_index].len = len;
    Some(id)
}

/// A prototype or interface's parameters and return type.
pub fn proto(cx: &mut Cx<'_>, d: &Def) -> Proto {
    let ret = type_of(cx, d, false).or_else(|| {
        d.kw("LIKEDS")
            .and_then(|k| word_arg(k, 0))
            .and_then(|n| cx.t.find(cx.scope, &n))
            .and_then(|i| match cx.t.syms[i].kind {
                Some(Kind::Ds(x)) => Some(Ty::Ds(x)),
                _ => None,
            })
    });
    let mut p = Proto {
        name: d.name.to_ascii_uppercase(),
        extpgm: d
            .kw("EXTPGM")
            .map(|k| str_arg(k, 0).or_else(|| word_arg(k, 0)).unwrap_or_else(|| d.name.to_ascii_uppercase())),
        extproc: d.kw("EXTPROC").map(|k| {
            let i = extproc_name_arg(k);
            match word_arg(k, i).as_deref() {
                // The name as the source writes it.
                Some("*DCLCASE") => d.name.clone(),
                _ => str_arg(k, i).or_else(|| word_arg(k, i)).unwrap_or_else(|| d.name.to_ascii_uppercase()),
            }
        }),
        extproc_word: d.kw("EXTPROC").is_some_and(|k| {
            let i = extproc_name_arg(k);
            str_arg(k, i).is_none() && word_arg(k, i).is_some_and(|w| !w.starts_with('*'))
        }),
        ret,
        ret_dim: dim_of(cx, d),
        opdesc: d.kw("OPDESC").is_some(),
        rtnparm: d.kw("RTNPARM").is_some(),
        def_line: d.at.line,
        ..Proto::default()
    };
    for c in &d.children {
        let ty = type_of(cx, c, false).or_else(|| {
            c.kw("LIKEDS")
                .and_then(|k| word_arg(k, 0))
                .and_then(|n| cx.t.find(cx.scope, &n))
                .and_then(|i| match cx.t.syms[i].kind {
                    Some(Kind::Ds(x)) => Some(Ty::Ds(x)),
                    _ => None,
                })
        });
        let opts: Vec<String> = c
            .kw("OPTIONS")
            .map(|k| (0..k.args.len()).filter_map(|i| word_arg(k, i)).collect())
            .unwrap_or_default();
        p.parms.push(Parm {
            name: c.name.to_ascii_uppercase(),
            ty: ty.unwrap_or(Ty::Unknown),
            dim: dim_of(cx, c),
            value: c.kw("VALUE").is_some(),
            konst: c.kw("CONST").is_some(),
            nopass: opts.iter().any(|o| o == "*NOPASS"),
            omit: opts.iter().any(|o| o == "*OMIT"),
            varsize: opts.iter().any(|o| o == "*VARSIZE"),
            string: opts.iter().any(|o| o == "*STRING"),
            trim: opts.iter().any(|o| o == "*TRIM"),
        });
    }
    p
}

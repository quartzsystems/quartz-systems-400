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
    let first = a.toks.first()?.text().to_ascii_uppercase();
    let f = DatFmt::parse(&first)?;
    let sep = a
        .toks
        .get(1)
        .map(rpgfront::lex::Tok::text)
        .and_then(|s| s.chars().next())
        .map_or(f.sep(), |c| if c == '0' { '\0' } else { c });
    Some((f, sep))
}

fn timfmt_arg(k: &Keyword) -> Option<(TimFmt, char)> {
    let a = k.arg(0)?;
    let first = a.toks.first()?.text().to_ascii_uppercase();
    let f = TimFmt::parse(&first)?;
    let sep = a
        .toks
        .get(1)
        .map(rpgfront::lex::Tok::text)
        .and_then(|s| s.chars().next())
        .map_or(f.sep(), |c| if c == '0' { '\0' } else { c });
    Some((f, sep))
}

/// The type a definition's keywords or columns give, and its `DIM`.
/// `None` when the type comes from elsewhere (`LIKEDS`, an external
/// description) or is in error.
pub fn type_of(cx: &mut Cx<'_>, d: &Def, subfield: bool) -> Option<Ty> {
    // Free-form type keywords.
    for k in &d.kws {
        let n = |i| int_arg(k, i);
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
                let scale = u8::try_from(n(1).unwrap_or(0)).ok()?;
                let max = if k.name == "BINDEC" { 18 } else { 63 };
                if digits == 0 || digits > max || scale > digits {
                    diag(cx, ids::BAD_LENGTH, 30, k.at, &k.name);
                    return None;
                }
                match k.name.as_str() {
                    "PACKED" => Ty::Packed(digits, scale),
                    "ZONED" => Ty::Zoned(digits, scale),
                    _ => Ty::Bin(digits, scale),
                }
            }
            "INT" | "UNS" => {
                let digits = u8::try_from(n(0)?).ok()?;
                if ![3, 5, 10, 20].contains(&digits) {
                    diag(cx, ids::BAD_LENGTH, 30, k.at, &k.name);
                    return None;
                }
                if k.name == "INT" { Ty::Int(digits) } else { Ty::Uns(digits) }
            }
            "FLOAT" => {
                let b = u8::try_from(n(0)?).ok()?;
                if b != 4 && b != 8 {
                    diag(cx, ids::BAD_LENGTH, 30, k.at, &k.name);
                    return None;
                }
                Ty::Float(b)
            }
            "DATE" => {
                let (f, s) = datfmt_arg(k).unwrap_or((cx.datfmt, cx.datfmt.sep()));
                Ty::Date(f, s)
            }
            "TIME" => {
                let (f, s) = timfmt_arg(k).unwrap_or((cx.timfmt, cx.timfmt.sep()));
                Ty::Time(f, s)
            }
            "TIMESTAMP" => Ty::Timestamp(u8::try_from(n(0).unwrap_or(6)).unwrap_or(6)),
            "POINTER" => {
                if word_arg(k, 0).as_deref() == Some("*PROC") {
                    Ty::ProcPtr
                } else {
                    Ty::Ptr
                }
            }
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
    let dec: Option<u8> = cols.decimals.parse().ok();
    let len = match (from, to) {
        (Some(f), Some(t)) if t >= f => Some(t - f + 1),
        (None, Some(t)) => Some(t),
        _ => None,
    };
    let positional = from.is_some();
    let varying = d.kw("VARYING").is_some();
    let code = if cols.ty.is_empty() {
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
        cols.ty.as_str()
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
            let (f, s) = d.kw("DATFMT").and_then(datfmt_arg).unwrap_or((cx.datfmt, cx.datfmt.sep()));
            Ty::Date(f, s)
        }
        "T" => {
            let (f, s) = d.kw("TIMFMT").and_then(timfmt_arg).unwrap_or((cx.timfmt, cx.timfmt.sep()));
            Ty::Time(f, s)
        }
        "Z" => Ty::Timestamp(6),
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

/// A length argument: 1 to 16773104.
fn len_ok(cx: &mut Cx<'_>, k: &Keyword, n: Option<i64>) -> Option<u32> {
    match n {
        Some(v) if (1..=16_773_104).contains(&v) => u32::try_from(v).ok(),
        _ => {
            diag(cx, ids::BAD_LENGTH, 30, k.at, &k.name);
            None
        }
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
    let name = word_arg(k, 0)?;
    let Some(id) = cx.t.find(cx.scope, &name) else {
        diag(cx, ids::NOT_DEFINED, 30, k.at, &name);
        return None;
    };
    let base = cx.t.syms[id].ty.clone();
    let adj = k.arg(1).and_then(|a| {
        let txt: String = a.toks.iter().map(rpgfront::lex::Tok::text).collect();
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

/// The IR name of a symbol in the current scope.
fn ir_name(cx: &Cx<'_>, name: &str) -> String {
    format!("${}{}", cx.prefix, names::mangle(name))
}

/// Declare one definition of a scope: a standalone field, constant,
/// data structure (with subfields), or prototype.
pub fn declare(cx: &mut Cx<'_>, d: &Def) -> Option<SymId> {
    match d.kind {
        DefKind::Const => {
            let v = d.value.clone().unwrap_or(Expr::Empty(d.at));
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
            let ty = type_of(cx, d, false).unwrap_or(Ty::Unknown);
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
                external: d
                    .kw("EXPORT")
                    .or_else(|| d.kw("IMPORT"))
                    .map(|k| str_arg(k, 0).unwrap_or_else(|| d.name.to_ascii_uppercase()))
                    .unwrap_or_default(),
                ..Sym::default()
            };
            add(cx, s, d.at)
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
    match int_arg(k, 0) {
        Some(n) if (1..=16_773_104).contains(&n) => u32::try_from(n).unwrap_or(0),
        _ => {
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

/// A data structure and its subfields.
fn declare_ds(cx: &mut Cx<'_>, d: &Def) -> Option<SymId> {
    let unnamed = d.name.is_empty() || d.name == "*N";
    let name = if unnamed {
        format!("_DS{}", cx.t.ds.len() + 1)
    } else {
        d.name.to_ascii_uppercase()
    };
    let qualified = d.kw("QUALIFIED").is_some() || d.kw("LIKEDS").is_some() || d.kw("LIKEREC").is_some();
    let ds_index = cx.t.ds.len();
    let dim = dim_of(cx, d);
    let storage = storage_of(cx, d);
    let ir = ir_name(cx, &name);
    let fixed_len: Option<u32> = d.fixed.as_ref().and_then(|c| {
        if c.from.is_empty() { c.to.parse().ok() } else { None }
    });
    let len_kw = d.kw("LEN").and_then(|k| int_arg(k, 0)).and_then(|v| u32::try_from(v).ok());
    let mut ds = Ds {
        len: 0,
        qualified,
        inz: d.kw("INZ").is_some(),
        occurs: d.kw("OCCURS").and_then(|k| int_arg(k, 0)).and_then(|v| u32::try_from(v).ok()).unwrap_or(0),
        psds: d.fixed.as_ref().is_some_and(|c| c.dstype == "S") || d.kw("PSDS").is_some(),
        dtaara: d
            .kw("DTAARA")
            .map(|k| str_arg(k, 0).unwrap_or_else(|| name.clone()))
            .or_else(|| (d.fixed.as_ref().is_some_and(|c| c.dstype == "U")).then(|| name.clone())),
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
    cx.t.ds.push(ds);
    if let Some(other) = like {
        let Some(oid) = cx.t.find(cx.scope, &other) else {
            diag(cx, ids::NOT_DEFINED, 30, d.at, &other);
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
        let cname = c.name.to_ascii_uppercase();
        let special = crate::extfiles::special_subfield(c, is_psds);
        let ty = match &special {
            Some((t, _)) => Some(t.clone()),
            None => type_of(cx, c, true),
        };
        let sdim = dim_of(cx, c);
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
                    (Ty::Ds(oi), len * sdim.max(1))
                } else {
                    // A LIKE that names nothing was reported already.
                    if c.kw("LIKE").is_none() {
                        diag(cx, ids::BAD_TYPE, 30, c.at, &cname);
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
        let pos_kw = c.kw("POS").and_then(|k| int_arg(k, 0)).and_then(|v| u32::try_from(v).ok());
        let overlay = c.kw("OVERLAY").cloned();
        let offset = if let Some((_, o)) = special {
            o
        } else if let Some(f) = from.or(pos_kw) {
            f.saturating_sub(1)
        } else if let Some(k) = overlay {
            let target = word_arg(&k, 0).unwrap_or_default();
            let base_sym = cx.t.subfield(ds_index, &target);
            let base_off = base_sym.map_or(0, |s| cx.t.syms[s].offset);
            if target != name && base_sym.is_none() {
                diag(cx, ids::NOT_DEFINED, 30, k.at, &target);
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
                None => {
                    let key = base_sym.unwrap_or(id);
                    overlay_next.insert(key, base_off + size);
                    base_off
                }
            }
        } else {
            let at = if is_ptr { next.div_ceil(16) * 16 } else { next };
            next = at + size;
            at
        };
        // An absolute position moves the next one past it too; an
        // overlay does not.
        if c.kw("OVERLAY").is_none() {
            next = next.max(offset + size);
        }
        end = end.max(offset + size);
        if is_ptr {
            cx.t.ds[ds_index].has_ptr = true;
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
        }
    }
    let len = len_kw.or(fixed_len).unwrap_or(0).max(end);
    let len = if len == 0 && d.children.is_empty() { 1 } else { len };
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
            str_arg(k, i).or_else(|| word_arg(k, i)).unwrap_or_else(|| d.name.to_ascii_uppercase())
        }),
        extproc_word: d.kw("EXTPROC").is_some_and(|k| {
            let i = extproc_name_arg(k);
            str_arg(k, i).is_none() && word_arg(k, i).is_some_and(|w| !w.starts_with('*'))
        }),
        ret,
        ret_dim: dim_of(cx, d),
        opdesc: d.kw("OPDESC").is_some(),
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

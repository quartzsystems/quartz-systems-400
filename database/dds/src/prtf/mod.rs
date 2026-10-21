// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The printer file compiler: record formats of fields and constants at
//! lines (positions 39-41) and print positions (42-44, `+n` after the
//! previous field), option indicators (8-16), and the keywords `SKIPB`,
//! `SKIPA`, `SPACEB`, `SPACEA`, `PAGNBR`, `DATE`, `TIME`, `EDTCDE`,
//! `EDTWRD`, `UNDERLINE`, `HIGHLIGHT`, `INDARA`, `TEXT`, `REF` and
//! `REFFLD`. `CPI`, `FONT`, `LPI` and their kin are accepted and change
//! nothing yet.

pub mod codec;
pub mod model;

use crate::compile::{Compilation, CompileOpts};
use crate::diag::{Diagnostics, ids};
use crate::lex::{self, Keyword, NameType, Spec, keyword::CONSTANT};
use crate::listing;
use crate::model::{DataType, FileRef};
use crate::refs::RefResolver;
use crate::source::Source;

pub use model::{Edit, PrtField, PrtFormat, PrtKind, PrtfDefinition, Spacing};

/// Keywords accepted with no effect yet.
const IGNORED: &[&str] = &[
    "CPI",
    "FONT",
    "LPI",
    "CHRSIZ",
    "CHRID",
    "COLOR",
    "TRNSPY",
    "DRAWER",
    "PAGRTT",
    "PRTQLTY",
    "DFNCHR",
    "TXTRTT",
    "BLKFOLD",
    "CDEFNT",
    "IGCALTTYP",
    "IGCANKCNV",
    "ALIAS",
    "COLHDG",
];

/// Compile printer file DDS.
pub fn compile_prtf(
    source: &Source,
    resolver: &dyn RefResolver,
    opts: &CompileOpts,
) -> Compilation<PrtfDefinition> {
    let mut diags = Diagnostics::default();
    let specs = lex::lex(source, &mut diags);
    let built = build(source, &specs, resolver, &mut diags);
    let listing = listing::render(source, None, diags.as_slice(), opts, "Printer file (PRTF)");
    let definition = built.filter(|_| !diags.blocks_create());
    Compilation {
        definition,
        diagnostics: diags.into_vec(),
        listing,
    }
}

fn number(k: &Keyword, i: usize) -> Option<u16> {
    k.parms.get(i)?.as_word()?.parse().ok()
}

/// `SKIPB`, `SKIPA`, `SPACEB`, `SPACEA`; whether `k` was one.
fn spacing(s: &mut Spacing, k: &Keyword, diags: &mut Diagnostics) -> bool {
    let slot = match k.name.as_str() {
        "SKIPB" => &mut s.skipb,
        "SKIPA" => &mut s.skipa,
        "SPACEB" => &mut s.spaceb,
        "SPACEA" => &mut s.spacea,
        _ => return false,
    };
    let max = if k.name.starts_with("SKIP") { 255 } else { 3 };
    match number(k, 0).filter(|n| (u16::from(max != 3)..=max).contains(n)) {
        Some(n) => *slot = n,
        None => diags.push(ids::prtf_parm(k.line, &k.name)),
    }
    true
}

fn text_of(k: &Keyword) -> Option<String> {
    k.parms
        .first()
        .and_then(|p| p.as_quoted())
        .map(str::to_string)
}

/// Positions 8-16: up to three indicators, each `N` or blank then two
/// digits.
fn conditions(raw: &str, line: usize, diags: &mut Diagnostics) -> Vec<(u8, bool)> {
    let chars: Vec<char> = raw.chars().collect();
    let mut out = Vec::new();
    for slot in chars.chunks(3) {
        let s: String = slot.iter().collect();
        if s.trim().is_empty() {
            continue;
        }
        let neg = slot.first() == Some(&'N');
        let digits: String = slot.iter().skip(1).collect();
        match digits.trim().parse::<u8>() {
            Ok(n) if (1..=99).contains(&n) && matches!(slot.first(), Some('N' | ' ')) => {
                out.push((n, !neg));
            }
            _ => {
                diags.push(ids::prtf_cond(line, raw));
                return Vec::new();
            }
        }
    }
    out
}

/// Positions 39-44: the line (0 for none) and the position, relative or
/// not; `None` when there is no position.
fn location(
    raw_line: &str,
    raw_pos: &str,
    line: usize,
    diags: &mut Diagnostics,
) -> Option<(u16, u16, bool)> {
    let l = raw_line.trim();
    let p = raw_pos.trim();
    if p.is_empty() {
        if !l.is_empty() {
            diags.push(ids::bad_location(line, &format!("{raw_line}{raw_pos}")));
        }
        return None;
    }
    let ln = if l.is_empty() {
        Some(0)
    } else {
        l.parse().ok().filter(|n| (1..=255).contains(n))
    };
    let (rel, digits) = p.strip_prefix('+').map_or((false, p), |d| (true, d.trim()));
    let pos: Option<u16> = digits.parse().ok().filter(|n| rel || (1..=378).contains(n));
    if let (Some(ln), Some(pos)) = (ln, pos) {
        Some((ln, pos, rel))
    } else {
        diags.push(ids::bad_location(line, &format!("{raw_line}{raw_pos}")));
        None
    }
}

/// The type and length of a referenced field.
fn referenced(
    resolver: &dyn RefResolver,
    file: Option<&FileRef>,
    field: &str,
) -> Option<(DataType, u32, u8)> {
    let f = file?;
    let formats = resolver.formats(&f.library, &f.file).ok()??;
    formats
        .iter()
        .flat_map(|fmt| fmt.fields.iter())
        .find(|d| d.name.to_string() == field)
        .map(|d| (d.data_type, d.length, d.decimals))
}

fn reffld(k: &Keyword, file_ref: Option<&FileRef>) -> Option<(String, Option<FileRef>)> {
    let first = k.parms.first()?.as_word()?;
    let field = first.rsplit('/').next().unwrap_or(first).to_string();
    let file = match k.parms.get(1).and_then(|p| p.as_word()) {
        Some(w) => FileRef::parse(w).ok(),
        None => file_ref.cloned(),
    };
    Some((field, file))
}

struct Ctx<'a> {
    source: &'a Source,
    resolver: &'a dyn RefResolver,
    file_ref: Option<FileRef>,
}

#[allow(clippy::too_many_lines)] // one arm per specification kind
fn build(
    source: &Source,
    specs: &[Spec],
    resolver: &dyn RefResolver,
    diags: &mut Diagnostics,
) -> Option<PrtfDefinition> {
    let mut def = PrtfDefinition::default();
    let mut cx = Ctx {
        source,
        resolver,
        file_ref: None,
    };
    for spec in specs {
        if spec.file_level {
            for k in &spec.keywords {
                match k.name.as_str() {
                    "INDARA" => def.indara = true,
                    "TEXT" => def.text = text_of(k),
                    "REF" => {
                        cx.file_ref = k
                            .parms
                            .first()
                            .and_then(|p| p.as_word())
                            .and_then(|w| FileRef::parse(w).ok());
                        if cx.file_ref.is_none() {
                            diags.push(ids::prtf_parm(k.line, "REF"));
                        }
                    }
                    n if IGNORED.contains(&n) => diags.push(ids::prtf_ignored(k.line, n)),
                    n => diags.push(ids::unknown_keyword(k.line, n)),
                }
            }
            continue;
        }
        match spec.name_type {
            Some(NameType::Record) => {
                let mut f = PrtFormat {
                    name: spec.name.clone().unwrap_or_default(),
                    text: None,
                    spacing: Spacing::default(),
                    fields: Vec::new(),
                };
                for k in &spec.keywords {
                    if spacing(&mut f.spacing, k, diags) {
                        continue;
                    }
                    match k.name.as_str() {
                        "TEXT" => f.text = text_of(k),
                        n if IGNORED.contains(&n) => diags.push(ids::prtf_ignored(k.line, n)),
                        n => diags.push(ids::unknown_keyword(k.line, n)),
                    }
                }
                def.formats.push(f);
            }
            Some(other) => diags.push(ids::spec_not_in_pf(spec.line, other.what())),
            None => {
                let Some(fmt) = def.formats.last_mut() else {
                    diags.push(ids::spec_before_format(spec.line, "field"));
                    continue;
                };
                if let Some(f) = field(&cx, spec, diags) {
                    fmt.fields.push(f);
                }
            }
        }
    }
    if def.formats.is_empty() {
        diags.push(ids::prtf_no_formats());
        return None;
    }
    Some(def)
}

/// A field or constant line.
#[allow(clippy::too_many_lines)] // one arm per field keyword
fn field(cx: &Ctx<'_>, spec: &Spec, diags: &mut Diagnostics) -> Option<PrtField> {
    let raw = &cx.source.lines()[spec.line];
    let cond = conditions(&raw.cols(8, 16), spec.line, diags);
    let Some((line, pos, relative)) =
        location(&raw.cols(39, 41), &raw.cols(42, 44), spec.line, diags)
    else {
        diags.push(ids::prtf_no_location(spec.line));
        return None;
    };
    let mut f = PrtField {
        name: spec.name.clone().unwrap_or_default(),
        kind: PrtKind::Char(0),
        line,
        pos,
        relative,
        cond,
        edit: Edit::None,
        underline: false,
        highlight: false,
        spacing: Spacing::default(),
    };
    let mut special = None;
    let mut refd = None;
    for k in &spec.keywords {
        if spacing(&mut f.spacing, k, diags) {
            continue;
        }
        match k.name.as_str() {
            CONSTANT => special = Some(PrtKind::Constant(text_of(k).unwrap_or_default())),
            "DATE" => special = Some(PrtKind::Date),
            "TIME" => special = Some(PrtKind::Time),
            "PAGNBR" => special = Some(PrtKind::PagNbr),
            "UNDERLINE" => f.underline = true,
            "HIGHLIGHT" => f.highlight = true,
            "EDTCDE" => {
                let code = k
                    .parms
                    .first()
                    .and_then(|p| p.as_word())
                    .and_then(|w| w.chars().next());
                let fill = k.parms.get(1).map(|p| {
                    let t = p.as_quoted().map_or_else(|| p.to_string(), str::to_string);
                    t.chars().next().unwrap_or('*')
                });
                match code {
                    Some(c) if "1234ABCDJKLMNOPQXYZ56789".contains(c) => {
                        f.edit = Edit::Code(c, fill);
                    }
                    _ => diags.push(ids::prtf_parm(k.line, "EDTCDE")),
                }
            }
            "EDTWRD" => match text_of(k) {
                Some(w) => f.edit = Edit::Word(w),
                None => diags.push(ids::prtf_parm(k.line, "EDTWRD")),
            },
            "REFFLD" => refd = reffld(k, cx.file_ref.as_ref()),
            "TEXT" => {}
            n if IGNORED.contains(&n) => diags.push(ids::prtf_ignored(k.line, n)),
            n => diags.push(ids::unknown_keyword(k.line, n)),
        }
    }
    if let Some(kind) = special {
        f.kind = kind;
        return Some(f);
    }
    if f.name.is_empty() {
        diags.push(ids::prtf_no_location(spec.line));
        return None;
    }
    if spec.reference && refd.is_none() {
        refd = Some((f.name.clone(), cx.file_ref.clone()));
    }
    let (mut ty, mut len, mut dec) = (spec.data_type, spec.length, spec.decimals);
    if let Some((rf, file)) = refd {
        let Some((t, l, d)) = referenced(cx.resolver, file.as_ref(), &rf) else {
            diags.push(ids::prtf_ref(spec.line, &rf));
            return None;
        };
        ty = ty.or(Some(if t == DataType::Char { 'A' } else { 'S' }));
        len = len.or(Some(l));
        dec = dec.or(Some(d)).filter(|_| t != DataType::Char);
    }
    if let Some(u) = spec.usage
        && !matches!(u, 'O' | 'P')
    {
        diags.push(ids::prtf_usage(spec.line, u));
        return None;
    }
    let Some(len) = len.filter(|n| *n > 0) else {
        diags.push(ids::bad_length(spec.line, ""));
        return None;
    };
    let ty = ty.unwrap_or(if dec.is_some() { 'S' } else { 'A' });
    f.kind = match ty {
        'A' => PrtKind::Char(u16::try_from(len.min(378)).unwrap_or(378)),
        'S' => {
            let d = u8::try_from(len.min(63)).unwrap_or(63);
            PrtKind::Zoned(d, dec.unwrap_or(0).min(d))
        }
        other => {
            diags.push(ids::prtf_type(spec.line, other));
            return None;
        }
    };
    Some(f)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::refs::NoRefs;

    /// A DDS line: conditioning (8-16), name type (17), name (19-28),
    /// length (30-34), type (35), decimals (36-37), line (39-41),
    /// position (42-44), keywords (45-).
    #[allow(clippy::too_many_arguments)]
    fn l(
        cond: &str,
        nt: &str,
        name: &str,
        len: &str,
        ty: &str,
        dec: &str,
        ln: &str,
        pos: &str,
        kw: &str,
    ) -> String {
        format!("     A {cond:<9}{nt:<1} {name:<10} {len:>5}{ty:<1}{dec:>2} {ln:>3}{pos:>3}{kw}\n")
    }

    #[test]
    fn report_formats() {
        let text = [
            l("", "", "", "", "", "", "", "", "INDARA"),
            l("", "R", "HEADER", "", "", "", "", "", "SKIPB(1) SPACEA(2)"),
            l("", "", "", "", "", "", "", "1", "'Quartz report'"),
            l("", "", "", "", "", "", "", "50", "DATE EDTCDE(Y)"),
            l("", "", "", "", "", "", "", "70", "'Page'"),
            l("", "", "", "", "", "", "", "+1", "PAGNBR"),
            l("", "R", "DETAIL", "", "", "", "", "", "SPACEA(1)"),
            l("", "", "CUST", "10", "A", "", "", "5", ""),
            l("", "", "AMOUNT", "9", "S", "2", "", "+2", "EDTCDE(J)"),
            l(" 50", "", "", "", "", "", "", "+1", "'*'"),
            l("", "R", "TOTAL", "", "", "", "", "", "SPACEB(1)"),
            l("", "", "", "", "", "", "", "5", "'Total' UNDERLINE"),
            l(
                "",
                "",
                "SUM",
                "11",
                "S",
                "2",
                "",
                "20",
                "EDTCDE(1 *) HIGHLIGHT",
            ),
        ]
        .concat();
        let (src, diags) = Source::from_text(&text);
        assert!(diags.is_empty());
        let c = compile_prtf(&src, &NoRefs, &CompileOpts::default());
        assert!(c.max_severity().is_none(), "{:?}", c.diagnostics);
        let d = c.definition.unwrap();
        assert!(d.indara);
        assert_eq!(d.formats.len(), 3);
        let h = d.format("HEADER").unwrap();
        assert_eq!((h.spacing.skipb, h.spacing.spacea), (1, 2));
        assert_eq!(h.fields[0].kind, PrtKind::Constant("Quartz report".into()));
        assert_eq!(h.fields[3].kind, PrtKind::PagNbr);
        assert!(h.fields[3].relative);
        let det = d.format("DETAIL").unwrap();
        assert_eq!(det.fields[1].kind, PrtKind::Zoned(9, 2));
        assert_eq!(det.fields[1].edit, Edit::Code('J', None));
        assert_eq!(det.fields[2].cond, vec![(50, true)]);
        assert_eq!(det.buffer_len(true), 19);
        assert_eq!(det.buffer_len(false), 20);
        let t = d.format("TOTAL").unwrap();
        assert!(t.fields[0].underline && t.fields[1].highlight);
        assert_eq!(t.fields[1].edit, Edit::Code('1', Some('*')));
        assert_eq!(codec::decode(&codec::encode(&d)), Some(d));
    }

    #[test]
    fn errors() {
        let text = [
            l("", "R", "ONE", "", "", "", "", "", ""),
            l("", "", "F1", "10", "P", "0", "", "5", ""),
            l("", "", "F2", "10", "A", "", "", "", ""),
            l("", "", "", "", "", "", "", "5", "BOGUS"),
        ]
        .concat();
        let (src, _) = Source::from_text(&text);
        let c = compile_prtf(&src, &NoRefs, &CompileOpts::default());
        assert!(c.definition.is_none());
        let ids = c.ids();
        assert!(ids.contains(&"DDS5002"), "{ids:?}");
        assert!(ids.contains(&"DDS5007"), "{ids:?}");
        assert!(ids.contains(&"DDS0209"), "{ids:?}");
    }
}

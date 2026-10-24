// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The lines the listing generates and numbers: the subfields of an
//! externally described data structure (`=D`), the input
//! specifications of externally described formats (`=I`) and their
//! output specifications (`=O`), each block with the box naming its
//! format; and the box after an externally described file.

use rpgfront::ast::{Def, DefKind};

use super::prologue::{framed, rule};
use super::{Out, leader};
use crate::compile::Compiled;
use crate::files::{ExtField, ExtFile};
use crate::syms::Kind;
use crate::types::Ty;

/// A row of a generated block.
pub(crate) enum Row {
    /// A box line, with the block's source id after it.
    Box(String),
    /// A numbered line: its text from column 6 and its sequence.
    Line(String, u32),
}

/// A generated block.
pub(crate) struct Block {
    /// The source line it comes before.
    pub at: u32,
    /// `D`, `I` or `O`.
    pub kind: char,
    /// Its rows.
    pub rows: Vec<Row>,
    /// Fields defined on its lines, by line offset in the block, with
    /// the flag the cross-reference shows.
    pub defs: Vec<(String, u32, char)>,
    /// Names referred to on its lines (fields of `=O`, formats).
    pub refs: Vec<(String, u32)>,
    /// The block is several sources, one per format: where each starts.
    pub starts: Vec<usize>,
    /// Output fields' buffer positions: line offset, start, end, name.
    pub positions: Vec<(u32, u32, u32, String)>,
}

fn ext_of<'a>(c: &'a Compiled, file: &str) -> Option<&'a ExtFile> {
    let (lib, name) = file.split_once('/').map_or(("*LIBL", file), |(l, n)| (l, n));
    c.table.extnames.get(&format!("{lib}/{name}"))
}

/// The box row `label . . . :  value` inside a 92-wide box.
fn boxrow(label: &str, value: &str) -> String {
    framed(8, &format!("{}  {value}", leader(1, label, 28)), 101)
}

fn data_format(t: &Ty) -> (String, char) {
    match t {
        Ty::Char(_) => (String::new(), 'A'),
        Ty::VarChar(..) => ("*VAR".into(), 'A'),
        Ty::Packed(..) => (String::new(), 'P'),
        Ty::Zoned(..) => (String::new(), 'S'),
        Ty::Bin(..) => (String::new(), 'B'),
        Ty::Int(_) => (String::new(), 'I'),
        Ty::Uns(_) => (String::new(), 'U'),
        Ty::Float(_) => (String::new(), 'F'),
        Ty::Date(f, s) => (format!("{}{s}", f.name()), 'D'),
        Ty::Time(f, s) => (format!("{}{s}", f.name()), 'T'),
        Ty::Timestamp(_) => (String::new(), 'Z'),
        Ty::Ucs2(_) | Ty::VarUcs2(..) => (String::new(), 'C'),
        Ty::Graph(_) | Ty::VarGraph(..) => (String::new(), 'G'),
        _ => (String::new(), 'A'),
    }
}

fn decimals(t: &Ty) -> String {
    match t {
        Ty::Packed(_, s) | Ty::Zoned(_, s) | Ty::Bin(_, s) => s.to_string(),
        Ty::Int(_) | Ty::Uns(_) => "0".into(),
        _ => String::new(),
    }
}

/// An `=I` field line, from column 6.
fn input_line(c: &Compiled, f: &ExtField, name: &str) -> String {
    let size = f.ty.size(&|i| c.table.ds_len(i));
    let (ext, fmt) = data_format(&f.ty);
    format!(
        "I{:24}{ext:<5}{fmt}{:>5}{:>5}{:>2}{name}",
        "",
        f.offset + 1,
        f.offset + size,
        decimals(&f.ty)
    )
}

/// An `=O` field line, from column 6.
fn output_line(c: &Compiled, f: &ExtField, name: &str) -> String {
    let size = f.ty.size(&|i| c.table.ds_len(i));
    let (_, fmt) = data_format(&f.ty);
    let (kind, len) = match &f.ty {
        Ty::Packed(d, s) => ("PACK".to_string(), format!("{d},{s}")),
        Ty::Zoned(d, s) => ("ZONE".to_string(), format!("{d},{s}")),
        Ty::Bin(d, s) => (format!("BIN{}", if *d <= 4 { 2 } else if *d <= 9 { 4 } else { 8 }), format!("{d},{s}")),
        Ty::Int(d) => (format!("INT{}", size), format!("{d},0")),
        Ty::Uns(d) => (format!("UNS{}", size), format!("{d},0")),
        Ty::Char(n) => ("CHAR".to_string(), n.to_string()),
        Ty::VarChar(n, _) => ("VCHR".to_string(), n.to_string()),
        Ty::Date(fm, s) => (format!("DATE {}{s}", fm.name()), size.to_string()),
        Ty::Time(fm, s) => (format!("TIME {}{s}", fm.name()), size.to_string()),
        Ty::Timestamp(_) => ("TMSP".to_string(), size.to_string()),
        Ty::Float(b) => ("FLTP".to_string(), b.to_string()),
        _ => ("CHAR".to_string(), size.to_string()),
    };
    format!("O{:23}{name:<14}{:>8}{fmt} {kind:<4}{len:>w$}", "", f.offset + size, w = 13usize.saturating_sub(kind.len()))
}

/// A `=D` subfield line, from column 6.
fn ds_line(t: &Ty, name: &str, extfld: Option<&str>) -> String {
    let (len, fmt) = match t {
        Ty::Char(n) | Ty::VarChar(n, _) => (n.to_string(), 'A'),
        Ty::Packed(d, _) => (d.to_string(), 'P'),
        Ty::Zoned(d, _) => (d.to_string(), 'S'),
        Ty::Bin(d, _) => (d.to_string(), 'B'),
        Ty::Int(d) => (d.to_string(), 'I'),
        Ty::Uns(d) => (d.to_string(), 'U'),
        Ty::Date(..) => (String::new(), 'D'),
        Ty::Time(..) => (String::new(), 'T'),
        Ty::Timestamp(_) => (String::new(), 'Z'),
        _ => (String::new(), 'A'),
    };
    let dec = decimals(t);
    let kw = extfld.map_or(String::new(), |e| format!("EXTFLD ({e})"));
    format!("D {name:<14}{:11}{len:>7}{fmt}{dec:>2} {kw}", "")
}

/// The blocks, in source order: the data structures' first, then the
/// input specifications, then the output (as `extfiles` counts them).
pub(crate) fn plan(c: &Compiled) -> Vec<Block> {
    let m = &c.module;
    let ext_ds: Vec<&Def> = m
        .defs
        .iter()
        .chain(m.procs.iter().flat_map(|p| p.defs.iter()))
        .filter(|d| d.kind == DefKind::Ds && (d.kw("EXTNAME").is_some() || d.fixed.as_ref().is_some_and(|x| x.ext == "E")))
        .collect();
    let has_input = c.table.recfmts.iter().any(|r| c.files.get(r.file).is_some_and(|f| f.scope == 0 && (f.input || f.update)));
    let mut out = Vec::new();
    let mut io = 0;
    for (k, &(at, n)) in c.generated.iter().enumerate() {
        let b = if let Some(d) = ext_ds.get(k) {
            ds_block(c, d, at)
        } else {
            io += 1;
            io_block(c, at, if io == 1 && has_input { 'I' } else { 'O' })
        };
        if n == 0 {
            continue;
        }
        let numbered = b.rows.iter().filter(|r| matches!(r, Row::Line(..))).count();
        if numbered == n as usize {
            out.push(b);
        } else {
            // Not what the numbering expects: plain numbered lines.
            out.push(Block {
                at,
                kind: b.kind,
                rows: (0..n).map(|k| Row::Line(b.kind.to_string(), k + 1)).collect(),
                defs: Vec::new(),
                refs: Vec::new(),
                starts: vec![0],
                positions: Vec::new(),
            });
        }
    }
    out
}

fn ds_block(c: &Compiled, d: &Def, at: u32) -> Block {
    let mut rows = Vec::new();
    let name = d.name.to_ascii_uppercase();
    let sym = c.table.syms.iter().find(|s| s.name == name && matches!(s.kind, Some(Kind::Ds(_))));
    let (file, fmt) = match d.kw("EXTNAME") {
        Some(k) => {
            let file = k
                .args
                .first()
                .map(|a| a.toks.iter().map(rpgfront::lex::Tok::text).collect::<String>())
                .unwrap_or_default()
                .trim_matches('\'')
                .to_ascii_uppercase();
            let fmt = k
                .args
                .iter()
                .skip(1)
                .map(|a| a.toks.iter().map(rpgfront::lex::Tok::text).collect::<String>().trim_matches('\'').to_ascii_uppercase())
                .find(|w| !w.starts_with('*'));
            (file, fmt)
        }
        None => (name.clone(), None),
    };
    let ext = ext_of(c, &file);
    let fm = ext.and_then(|e| match &fmt {
        Some(f) => e.formats.iter().find(|x| x.name == *f),
        None => e.formats.first(),
    });
    let fname = file.rsplit('/').next().unwrap_or(&file).to_string();
    rows.push(Row::Box(rule(8, 101)));
    rows.push(Row::Box(boxrow("Data structure", &name)));
    let prefix = d.kw("PREFIX").map(|k| {
        let p = k.args.first().map(|a| a.toks.iter().map(rpgfront::lex::Tok::text).collect::<String>()).unwrap_or_default();
        let n = k.args.get(1).map(|a| a.toks.iter().map(rpgfront::lex::Tok::text).collect::<String>()).unwrap_or_default();
        (p.trim_matches('\'').to_ascii_uppercase(), n.trim().parse::<u32>().unwrap_or(0))
    });
    if let Some((p, n)) = &prefix {
        rows.push(Row::Box(boxrow("Prefix", &format!("{p} : {n:>4}"))));
    }
    if let (Some(e), Some(f)) = (ext, fm) {
        rows.push(Row::Box(boxrow("External format", &format!("{} : {}/{fname}", f.name, e.lib))));
    }
    rows.push(Row::Box(rule(8, 101)));
    let mut defs = Vec::new();
    if let Some(Kind::Ds(di)) = sym.and_then(|s| s.kind.clone()) {
        for (k, &sf) in c.table.ds[di].subfields.iter().enumerate() {
            let s = &c.table.syms[sf];
            let orig = fm.and_then(|f| f.fields.iter().find(|x| x.offset == s.offset)).map(|x| x.name.clone());
            let extfld = orig.as_deref().filter(|o| *o != s.name);
            rows.push(Row::Line(ds_line(&s.ty, &s.name, extfld), u32::try_from(k + 1).unwrap_or(0)));
            defs.push((format!("{name}.{}", s.name), u32::try_from(k).unwrap_or(0), 'D'));
        }
    }
    Block {
        at,
        kind: 'D',
        rows,
        defs,
        refs: Vec::new(),
        starts: vec![0],
        positions: Vec::new(),
    }
}

fn io_block(c: &Compiled, at: u32, kind: char) -> Block {
    let mut rows = Vec::new();
    let mut defs = Vec::new();
    let mut refs = Vec::new();
    let mut starts = Vec::new();
    let mut positions = Vec::new();
    let mut no = 0u32;
    for r in &c.table.recfmts {
        let Some(f) = c.files.get(r.file) else { continue };
        if f.scope != 0 || r.qualified {
            continue;
        }
        let wanted = if kind == 'I' {
            f.input || f.update
        } else {
            (f.output || f.update) && crate::extfiles::written(&c.module, &r.name)
        };
        if !wanted {
            continue;
        }
        starts.push(rows.len());
        let mut seq = 0;
        let named = kind == 'I' && c.module.inputs.iter().any(|i| i.external && (i.name.eq_ignore_ascii_case(&r.name) || i.name.eq_ignore_ascii_case(&r.fmt.name)));
        if !named {
            seq += 1;
            rows.push(Row::Line(format!("{kind}{}", r.name), seq));
            refs.push((r.name.clone(), no));
            no += 1;
        }
        let lib = f.ext.as_ref().map(|e| e.lib.clone()).unwrap_or_default();
        rows.push(Row::Box(rule(8, 101)));
        rows.push(Row::Box(boxrow("RPG record format", &r.name)));
        if let Some((p, n)) = &f.prefix {
            rows.push(Row::Box(boxrow("Prefix", &format!("{} : {n}", p.trim_matches('\'')))));
        }
        rows.push(Row::Box(boxrow("External format", &format!("{} : {lib}/{}", r.fmt.name, f.obj.1))));
        rows.push(Row::Box(rule(8, 101)));
        for (k, x) in r.fmt.fields.iter().enumerate() {
            let used = if kind == 'I' { matches!(x.usage, 'I' | 'B') } else { matches!(x.usage, 'O' | 'B') };
            if !used {
                continue;
            }
            let name = r.names.get(k).cloned().unwrap_or_else(|| x.name.clone());
            seq += 1;
            let text = if kind == 'I' {
                let mut t = input_line(c, x, &name);
                if let Some(f) = c
                    .module
                    .inputs
                    .iter()
                    .filter(|i| i.external)
                    .flat_map(|i| i.fields.iter())
                    .find(|f| f.extname.eq_ignore_ascii_case(&x.name) || f.name.eq_ignore_ascii_case(&name))
                {
                    let extra = format!("{:<2}{:<2}{:<2}{:<2}{:<2}{:<2}", f.level, f.matching, f.relation, f.inds[0], f.inds[1], f.inds[2]);
                    if !extra.trim().is_empty() {
                        t = format!("{t:<57}{}", extra.trim_end());
                    }
                }
                t
            } else {
                output_line(c, x, &name)
            };
            rows.push(Row::Line(text, seq));
            if kind == 'I' {
                let written = (f.output || f.update) && crate::extfiles::written(&c.module, &r.name);
                defs.push((name, no, if written { 'M' } else { 'D' }));
            } else {
                let size = x.ty.size(&|i| c.table.ds_len(i));
                positions.push((no, x.offset + 1, x.offset + size, name.clone()));
                refs.push((name, no));
            }
            no += 1;
        }
    }
    Block {
        at,
        kind,
        rows,
        defs,
        refs,
        starts,
        positions,
    }
}

/// Print a block: its numbered lines from `first`, each source its
/// own id from `next_id`; `rulers`, the fixed-form rulers its lines
/// print under and those to go back to.
pub(crate) fn print(
    out: &mut Out,
    b: &Block,
    first: u32,
    next_id: &mut u32,
    cc_first: char,
    rulers: Option<((String, String), (String, String))>,
) {
    let mut no = first;
    let mut id = *next_id;
    let mut cc = cc_first;
    let mut switched = false;
    for (k, r) in b.rows.iter().enumerate() {
        if b.starts.contains(&k) {
            id = *next_id;
            *next_id += 1;
        }
        match r {
            Row::Box(t) => out.line(' ', format!("{t}{id:>22}")),
            Row::Line(t, seq) => {
                if let Some((fixed, _)) = &rulers
                    && !switched
                {
                    out.change_rulers(fixed.clone());
                    switched = true;
                }
                out.line(cc, format!("{no:>6}={t:<100}{id:>17}{seq:06}"));
                cc = ' ';
                no += 1;
            }
        }
    }
    if let Some((_, back)) = rulers
        && switched
    {
        out.change_rulers(back);
    }
}

/// The box after an externally described file's declaration.
pub(crate) fn file_box(out: &mut Out, c: &Compiled, file: &crate::files::File) {
    let Some(e) = &file.ext else { return };
    out.line(' ', rule(8, 101));
    out.line(' ', framed(8, &format!("{:31}{:<17}External name", "", "RPG name"), 101));
    out.line(' ', framed(8, &format!(" File name. . . . . . . . . :  {:<17}{}/{}", file.name, e.lib, file.obj.1), 101));
    let fmts: Vec<_> = c.table.recfmts.iter().filter(|r| r.file_name == file.name).collect();
    for (k, r) in fmts.iter().enumerate() {
        let label = if k == 0 { " Record format(s) . . . . . :" } else { "                            " };
        out.line(' ', framed(8, &format!("{label}  {:<17}{}", r.name, r.fmt.name), 101));
    }
    out.line(' ', rule(8, 101));
}

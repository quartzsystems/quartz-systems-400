// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The cross-reference: files and record formats, fields by scope
//! (sorted in EBCDIC order, the attributes in IBM's notation, the
//! references four to a line with `D` defined and `M` modified) and
//! indicators; a name nothing refers to is marked `RNF7031`, one that
//! is not defined `RNF7030`. And the key field information before it.

use super::refs::{At, Refs, has_definition};
use super::{Out, spaced};
use crate::compile::Compiled;
use crate::syms::{Kind, Storage, Sym};
use crate::types::Ty;

/// Where the references start, and how many fit on a line.
const REFS_AT: usize = 45;
const PER_LINE: usize = 4;

use super::refs::GEN;

/// A reference's listing number.
fn lno(c: &Compiled, line: u32) -> u32 {
    let l = line & !(GEN | super::refs::LATE);
    if line & GEN != 0 { l } else { c.listing_line(l) }
}

/// Sort references by listing number, those noted late last.
fn sort_refs(c: &Compiled, v: &mut [At]) {
    v.sort_by_key(|(l, _)| (*l & super::refs::LATE != 0, lno(c, *l)));
}

/// What generated lines define and refer to, by name.
#[derive(Default)]
struct GenRefs {
    defs: std::collections::HashMap<String, At>,
    refs: std::collections::HashMap<String, Vec<At>>,
}

fn gen_refs(c: &Compiled) -> GenRefs {
    let plan = super::genlines::plan(c);
    let nums = super::source::numbers(c, &plan);
    let mut g = GenRefs::default();
    for (b, first) in plan.iter().zip(&nums.blocks) {
        for (name, k, flag) in &b.defs {
            match g.defs.get(name) {
                // A response indicator defined again is changed again.
                Some(_) if name.starts_with("*IN") => g.refs.entry(name.clone()).or_default().push(((first + k) | GEN, 'M')),
                // A field another format's input defines again: each
                // input changes it.
                Some(_) => {
                    if let Some(d) = g.defs.get_mut(name) {
                        d.1 = 'M';
                    }
                    g.refs.entry(name.clone()).or_default().push(((first + k) | GEN, 'M'));
                }
                None => {
                    g.defs.insert(name.clone(), ((first + k) | GEN, *flag));
                }
            }
        }
        for (name, k) in &b.refs {
            g.refs.entry(name.clone()).or_default().push(((first + k) | GEN, ' '));
        }
    }
    g
}

/// The identifier of an unreferenced name.
pub(crate) const UNREF: &str = "RNF7031";

/// EBCDIC collation of a name: specials, then letters, then digits.
pub(crate) fn ebcdic_key(s: &str) -> Vec<u8> {
    s.bytes()
        .map(|b| match b {
            b' ' => 0x40,
            b'.' => 0x4B,
            b'(' => 0x4D,
            b'+' => 0x4E,
            b'&' => 0x50,
            b'$' => 0x5B,
            b'*' => 0x5C,
            b')' => 0x5D,
            b';' => 0x5E,
            b'-' => 0x60,
            b'/' => 0x61,
            b',' => 0x6B,
            b'%' => 0x6C,
            b'_' => 0x6D,
            b':' => 0x7A,
            b'#' => 0x7B,
            b'@' => 0x7C,
            b'a'..=b'i' => 0x81 + (b - b'a'),
            b'j'..=b'r' => 0x91 + (b - b'j'),
            b's'..=b'z' => 0xA2 + (b - b's'),
            b'A'..=b'I' => 0xC1 + (b - b'A'),
            b'J'..=b'R' => 0xD1 + (b - b'J'),
            b'S'..=b'Z' => 0xE2 + (b - b'S'),
            b'0'..=b'9' => 0xF0 + (b - b'0'),
            _ => b,
        })
        .collect()
}

/// One entry: its marker, name, attributes and references.
struct Item {
    msg: &'static str,
    name: String,
    attrs: Vec<String>,
    refs: Vec<At>,
    subs: Vec<Item>,
}

impl Item {
    fn unreferenced(&self) -> bool {
        self.refs.iter().all(|(_, f)| matches!(f, 'D' | 'd' | 'n' | 'm'))
    }
}

/// Special subroutines are never unreferenced.
fn always_used(name: &str) -> bool {
    matches!(name, "*PSSR" | "*INZSR" | "*ENTRY")
}

/// The cross-reference's content.
pub(crate) struct Model {
    files: Vec<Item>,
    proc_files: Vec<Vec<Item>>,
    globals: Vec<Item>,
    procs: Vec<(String, Vec<Item>)>,
    inds: Vec<Item>,
}

impl Model {
    /// How many names are marked unreferenced.
    pub fn unreferenced(&self) -> usize {
        // Of the indicators, only those set and never used are counted.
        let all = self.files.iter().chain(self.proc_files.iter().flatten()).chain(&self.globals).chain(self.procs.iter().flat_map(|p| &p.1)).chain(
            self.inds.iter().filter(|i| i.refs.iter().all(|(_, f)| *f == 'D')),
        );
        all.map(count_unref).sum()
    }
}

/// An item's unreferenced names, its nested subfields' among them.
fn count_unref(i: &Item) -> usize {
    usize::from(i.msg == UNREF) + i.subs.iter().map(count_unref).sum::<usize>()
}

fn attrs_of(c: &Compiled, s: &Sym) -> Vec<String> {
    let mut v = match &s.kind {
        Some(Kind::Subr) => return vec!["BEGSR".into()],
        Some(Kind::Label) => return vec!["TAG".into()],
        Some(Kind::Klist) => return vec!["KLIST".into()],
        Some(Kind::Plist) => return vec!["PLIST".into()],
        Some(Kind::Const(_)) => return vec!["CONST".into()],
        Some(Kind::Proto(pi)) => {
            let mut v = Vec::new();
            if let Some(r) = c.table.protos.get(*pi).and_then(|p| p.ret.clone()) {
                v.extend(type_attrs(c, &r));
            }
            v.push("PROTOTYPE".into());
            return v;
        }
        _ => {
            let input_field = s.implicit && s.kind == Some(Kind::Field);
            match &s.ty {
                Ty::Zoned(d, sc) if input_field => vec![Ty::Packed(*d, *sc).xref(&|_| 0)],
                t => vec![t.xref(&|i| c.table.ds_len(i))],
            }
        }
    };
    if s.ty == Ty::ProcPtr {
        v[0] = "*(16) PROCPTR".into();
    }
    match &s.storage {
        Storage::Based(p) => {
            let p = &c.table.syms[*p];
            let based = if p.name.starts_with("_EP") {
                "_QRNL_PRM+".to_string()
            } else if p.name.starts_with('_') {
                "_QRNL_PST+".to_string()
            } else {
                p.name.clone()
            };
            v.push(format!("BASED({based})"));
        }
        // A parameter passed by reference, when it is used.
        // (when the compile got as far as laying out storage).
        Storage::Param if !is_value_param(c, s) && c.max_severity <= c.ctl.genlvl => v.push("BASED(_QRNL_PST+)".into()),
        _ => {}
    }
    if let Ty::VarChar(_, p) | Ty::VarUcs2(_, p) | Ty::VarGraph(_, p) = &s.ty {
        v.push(format!("VARYING({p})"));
    }
    if matches!(s.ty, Ty::Ucs2(_) | Ty::VarUcs2(..)) {
        v.push("CCSID(13488)".into());
    }
    if s.template {
        v.push("TEMPLATE".into());
    }
    v
}

fn type_attrs(c: &Compiled, t: &Ty) -> Vec<String> {
    let mut v = vec![t.xref(&|i| c.table.ds_len(i))];
    if let Ty::VarChar(_, p) | Ty::VarUcs2(_, p) | Ty::VarGraph(_, p) = t {
        v.push(format!("VARYING({p})"));
    }
    v
}

/// Whether a parameter is passed by value.
fn is_value_param(c: &Compiled, s: &Sym) -> bool {
    let Some(k) = s.scope.checked_sub(1) else { return false };
    let Some(p) = c.module.procs.get(k) else { return false };
    let Some(id) = c.table.find(0, &p.name.to_ascii_uppercase()) else { return false };
    let Some(Kind::Proto(pi)) = c.table.syms[id].kind else { return false };
    c.table.protos[pi].parms.iter().any(|q| q.name.eq_ignore_ascii_case(&s.name) && q.value)
}

fn shown(s: &Sym) -> bool {
    !s.name.starts_with('_')
        && !s.name.is_empty()
        && matches!(
            s.kind,
            Some(
                Kind::Field
                    | Kind::Ds(_)
                    | Kind::Subfield
                    | Kind::Const(_)
                    | Kind::Proto(_)
                    | Kind::Param
                    | Kind::Subr
                    | Kind::Label
                    | Kind::Klist
                    | Kind::Plist
            )
        )
}

fn display_name(c: &Compiled, s: &Sym) -> String {
    let dim = match s.kind {
        Some(Kind::Ds(di)) if s.dim == 0 => c.table.ds.get(di).map_or(0, |d| d.occurs),
        _ => s.dim,
    };
    match s.vardim {
        1 => format!("{}(*AUTO:{dim})", s.name),
        2 => format!("{}(*VAR:{dim})", s.name),
        _ if dim > 0 => format!("{}({dim})", s.name),
        _ => s.name.clone(),
    }
}

fn item(c: &Compiled, w: &Refs, g: &GenRefs, id: usize) -> Item {
    item_at(c, w, g, id, 0)
}

fn item_at(c: &Compiled, w: &Refs, g: &GenRefs, id: usize, depth: usize) -> Item {
    let s = &c.table.syms[id];
    let mut refs = Vec::new();
    let key = match (&s.kind, s.parent) {
        (Some(Kind::Subfield), Some(p)) => format!("{}.{}", c.table.syms[p].name, s.name),
        _ => s.name.clone(),
    };
    let gen_def = g.defs.get(&key).filter(|_| s.implicit && matches!(s.kind, Some(Kind::Field | Kind::Subfield)));
    let generated = gen_def.is_some() || (s.implicit && s.kind == Some(Kind::Field) && g.refs.contains_key(&s.name));
    let file_field = s.kind == Some(Kind::Field);
    match gen_def {
        Some(d) => refs.push(*d),
        // A file's output-only field is defined on no line.
        None if generated => {}
        None if has_definition(c, id) => refs.push((name_line(c, s.def_line), 'D')),
        None => {}
    }
    if generated && file_field {
        refs.extend(g.refs.get(&s.name).cloned().unwrap_or_default());
    }
    refs.extend(w.syms.get(&id).cloned().unwrap_or_default());
    sort_refs(c, &mut refs);
    let attrs = attrs_of(c, s);
    let mut it = Item {
        msg: "",
        name: format!("{:w$}{}", "", display_name(c, s), w = depth * 2),
        attrs,
        refs,
        subs: Vec::new(),
    };
    // A qualified structure's subfields, and a nested one's.
    let nested = match (&s.kind, &s.ty) {
        (Some(Kind::Ds(di)), _) if c.table.ds.get(*di).is_some_and(|d| d.qualified) => Some(*di),
        (Some(Kind::Subfield), Ty::Ds(di)) => Some(*di),
        _ => None,
    };
    if let Some(di) = nested {
        for &sf in &c.table.ds[di].subfields {
            // Copied subfields nothing refers to are not listed, but
            // counted as unreferenced.
            if !has_definition(c, sf) && w.syms.get(&sf).is_none_or(Vec::is_empty) {
                it.subs.push(Item { msg: UNREF, name: String::new(), attrs: Vec::new(), refs: Vec::new(), subs: Vec::new() });
                continue;
            }
            let mut sub = item_at(c, w, g, sf, depth + 1);
            if sub.unreferenced() {
                sub.msg = UNREF;
            }
            it.subs.push(sub);
        }
        for ((ds, name), refs) in &w.undefined_subs {
            if *ds == id {
                it.subs.push(Item {
                    msg: "RNF7030",
                    name: format!("{:w$}{name}", "", w = (depth + 1) * 2),
                    attrs: vec!["**UNDEF**".into()],
                    refs: refs.clone(),
                    subs: Vec::new(),
                });
            }
        }
        it.subs.sort_by_key(|s| ebcdic_key(s.name.trim_start()));
    }
    // A structure whose LIKEDS failed is itself not defined.
    let failed_ds = matches!(s.kind, Some(Kind::Ds(_)))
        && c.diags.iter().any(|d| d.msgid == "RNF7030" && d.at.line == s.def_line && d.args.first().is_some_and(|a| a.eq_ignore_ascii_case(&s.name)));
    if failed_ds || (s.ty == Ty::Unknown && matches!(s.kind, Some(Kind::Field | Kind::Subfield | Kind::Param))) {
        it.msg = "RNF7030";
        if failed_ds {
            it.attrs = vec!["**UNDEF**".into()];
        }
        return it;
    }
    // A field a calculation defines by its length is in use.
    let calc_defined = s.implicit
        && !generated
        && c.read.lines.iter().find(|l| l.no == s.def_line).is_some_and(|l| {
            matches!(l.class, rpgfront::reader::Class::Spec('C' | 'c'))
        });
    if it.unreferenced() && !calc_defined {
        it.msg = UNREF;
    }
    it
}

/// The line a definition's name starts on: a name continued with
/// `...` starts on an earlier line.
fn name_line(c: &Compiled, line: u32) -> u32 {
    let mut l = line;
    while l > 1
        && c.read.lines.iter().find(|x| x.no == l - 1).is_some_and(|x| {
            matches!(x.class, rpgfront::reader::Class::Spec('D' | 'd')) && x.text.trim_end().ends_with("...")
        })
    {
        l -= 1;
    }
    l
}

fn qualified_sub(c: &Compiled, s: &Sym) -> bool {
    s.kind == Some(Kind::Subfield)
        && s.parent.is_some_and(|p| match c.table.syms.get(p).and_then(|p| p.kind.clone()) {
            Some(Kind::Ds(di)) => c.table.ds.get(di).is_some_and(|d| d.qualified),
            // A nested structure's subfields.
            Some(Kind::Subfield) => matches!(c.table.syms[p].ty, Ty::Ds(_)),
            _ => false,
        })
}

fn scope_items(c: &Compiled, w: &Refs, g: &GenRefs, scope: usize) -> Vec<Item> {
    let mut v: Vec<(Vec<u8>, Item)> = c
        .table
        .syms
        .iter()
        .enumerate()
        .filter(|(_, s)| s.scope == scope && shown(s) && !qualified_sub(c, s))
        .map(|(i, s)| (ebcdic_key(&s.name), item(c, w, g, i)))
        .collect();
    if scope == 0 {
        let mut specials: std::collections::BTreeMap<String, Vec<At>> = w.fields.clone();
        for (k, d) in g.defs.iter().filter(|(k, _)| k.starts_with("*IN")) {
            specials.entry(k.clone()).or_default().push(*d);
        }
        for (k, r) in g.refs.iter().filter(|(k, _)| k.starts_with("*IN")) {
            specials.entry(k.clone()).or_default().extend(r.iter().copied());
        }
        for (k, refs) in &specials {
            let mut refs = refs.clone();
            sort_refs(c, &mut refs);
            let name = if k == "*IN" { "*IN(99)".to_string() } else { k.clone() };
            let attr = match k.as_str() {
                "UDATE" => "S(6,0)",
                "*DATE" => "S(8,0)",
                p if p.starts_with("PAGE") => "P(4,0)",
                _ => "N(1)",
            };
            let ind = k.strip_prefix("*IN").unwrap_or(k);
            let declared = refs.iter().all(|(_, f)| matches!(f, 'd' | 'D')) && w.inds.get(ind).is_none_or(|v| v.iter().all(|(_, f)| *f == 'd'));
            v.push((
                ebcdic_key(k),
                Item {
                    msg: if declared { UNREF } else { "" },
                    name,
                    attrs: vec![attr.into()],
                    refs,
                    subs: Vec::new(),
                },
            ));
        }
    }
    for ((sc, name), (what, refs)) in &w.labels {
        if *sc != scope {
            continue;
        }
        let mut refs = refs.clone();
        sort_refs(c, &mut refs);
        let mut it = Item {
            msg: "",
            name: name.clone(),
            attrs: vec![(*what).to_string()],
            refs,
            subs: Vec::new(),
        };
        if it.unreferenced() && !always_used(name) {
            it.msg = UNREF;
        }
        v.push((ebcdic_key(name), it));
    }
    for ((sc, name), refs) in &w.undefined {
        if *sc != scope || name.starts_with('*') {
            continue;
        }
        let mut refs = refs.clone();
        sort_refs(c, &mut refs);
        v.push((
            ebcdic_key(name),
            Item {
                msg: "RNF7030",
                name: name.clone(),
                attrs: vec!["**UNDEF**".into()],
                refs,
                subs: Vec::new(),
            },
        ));
    }
    // Names only a declaration's diagnostic tells of.
    if scope == 0 {
        for d in c.diags.iter().filter(|d| d.msgid == "RNF7030") {
            let Some(name) = d.args.first().map(|a| a.to_ascii_uppercase()) else { continue };
            let known = c.table.find(0, &name).is_some()
                || w.undefined.keys().any(|(_, n)| *n == name)
                || w.undefined_subs.keys().any(|(_, n)| n.eq_ignore_ascii_case(&name))
                || c.table.syms.iter().any(|s| s.kind == Some(Kind::Subfield) && s.name == name && s.def_line == d.at.line)
                || name.contains('.');
            if known || name.is_empty() || v.iter().any(|(k, _)| *k == ebcdic_key(&name)) {
                continue;
            }
            // Defined (and dropped) by its own declaration, or named there.
            let defines = c.read.lines.iter().find(|l| l.no == d.at.line).is_some_and(|l| {
                let words: Vec<String> = l.text.split_whitespace().take(2).map(|w| w.trim_end_matches(';').to_ascii_uppercase()).collect();
                words.len() == 2 && words[0].starts_with("DCL-") && words[1] == name
            });
            v.push((
                ebcdic_key(&name),
                Item {
                    msg: "RNF7030",
                    name: name.clone(),
                    attrs: vec!["**UNDEF**".into()],
                    refs: vec![(d.at.line, if defines { 'D' } else { ' ' })],
                    subs: Vec::new(),
                },
            ));
        }
    }
    v.sort_by(|a, b| a.0.cmp(&b.0));
    v.dedup_by(|a, b| a.0 == b.0);
    v.into_iter().map(|(_, i)| i).collect()
}

/// IBM's order of indicators: `*IN`, the numbered ones, control
/// levels, last record, overflow, matching, first page, halt, external,
/// function keys.
fn ind_rank(k: &str) -> u8 {
    let b = k.as_bytes();
    match b {
        [b'*', ..] => 0,
        [d, e] if d.is_ascii_digit() && e.is_ascii_digit() => 1,
        [b'L', d] if d.is_ascii_digit() => 2,
        b"LR" => 3,
        [b'O', _] => 4,
        b"MR" => 5,
        b"1P" => 6,
        [b'H', _] => 7,
        [b'U', _] => 8,
        [b'K', _] => 9,
        _ => 10,
    }
}

/// Build the cross-reference.
pub(crate) fn model(c: &Compiled) -> Model {
    let w = super::refs::collect(c);
    let g = gen_refs(c);
    let files = file_items(c, &w, &g, 0);
    let proc_files = (1..=c.module.procs.len()).map(|k| file_items(c, &w, &g, k)).collect();
    model_rest(c, &w, &g, files, proc_files)
}

/// A scope's files and their record formats.
fn file_items(c: &Compiled, w: &Refs, g: &GenRefs, scope: usize) -> Vec<Item> {
    let mut files = Vec::new();
    let mut sorted: Vec<&crate::files::File> = c.files.iter().filter(|f| f.scope == scope).collect();
    sorted.sort_by_key(|f| ebcdic_key(&f.name));
    sorted.dedup_by(|a, b| a.name == b.name);
    for f in sorted {
        let id = c.table.find(scope, &f.name);
        let mut refs = vec![(f.def_line, 'D')];
        // A second declaration of the name refers to the first.
        refs.extend(c.files.iter().filter(|o| o.name == f.name && o.def_line != f.def_line).map(|o| (o.def_line, 'n')));
        // So does one the compiler refused.
        if scope == 0 {
            for d in c.module.files.iter().filter(|d| d.name.eq_ignore_ascii_case(&f.name) && d.at.line != f.def_line) {
                if !refs.iter().any(|(l, _)| *l == d.at.line) {
                    refs.push((d.at.line, 'n'));
                }
            }
        }
        if let Some(id) = id {
            refs.extend(w.syms.get(&id).cloned().unwrap_or_default());
        }
        sort_refs(c, &mut refs);
        let mut it = Item {
            msg: "",
            name: f.name.clone(),
            attrs: vec![f.dev.name().to_string()],
            refs,
            subs: Vec::new(),
        };
        // The cycle reads primary and secondary files.
        let cycle = matches!(f.designation.as_str(), "P" | "S");
        let by_format = c.table.recfmts.iter().filter(|r| r.file_name == f.name).any(|r| {
            c.table.find(0, &r.name).is_some_and(|id| w.syms.get(&id).is_some_and(|v| !v.is_empty())) || g.refs.contains_key(&r.name)
        });
        let not_found = f.external && f.ext.is_none();
        // An operation the device does not allow is no use of the file.
        let severe = |line: u32| c.diags.iter().any(|d| d.at.line == line && matches!(d.msgid, "RNF5276" | "RNF5202"));
        let unused = it.refs.iter().all(|(l, fl)| matches!(fl, 'D' | 'd' | 'n' | 'm') || severe(*l));
        if (unused || !f.external && !io_used(c, &f.name)) && !cycle && !by_format && !not_found {
            it.msg = UNREF;
        }
        for r in c.table.recfmts.iter().filter(|r| r.file_name == f.name) {
            // A renamed format is named where RENAME is, not defined.
            let rename_line = c
                .module
                .files
                .iter()
                .find(|d| d.name.eq_ignore_ascii_case(&f.name))
                .and_then(|d| d.kws.iter().find(|k| k.name.eq_ignore_ascii_case("RENAME")))
                .map_or(f.def_line, |k| k.at.line);
            let mut refs = vec![if r.name == r.fmt.name { (f.def_line, 'D') } else { (rename_line, ' ') }];
            if let Some(id) = c.table.find(0, &r.name) {
                refs.extend(w.syms.get(&id).cloned().unwrap_or_default());
            }
            refs.extend(g.refs.get(&r.name).cloned().unwrap_or_default());
            refs.extend(w.formats.get(&r.name).cloned().unwrap_or_default());
            sort_refs(c, &mut refs);
            it.subs.push(Item {
                msg: "",
                name: format!("  {}", r.name),
                attrs: Vec::new(),
                refs,
                subs: Vec::new(),
            });
        }
        files.push(it);
    }
    files
}

fn model_rest(c: &Compiled, w: &Refs, g: &GenRefs, files: Vec<Item>, proc_files: Vec<Vec<Item>>) -> Model {
    let globals = scope_items(c, w, g, 0);
    let procs = c
        .module
        .procs
        .iter()
        .enumerate()
        .map(|(k, p)| (p.name.to_ascii_uppercase(), scope_items(c, w, g, k + 1)))
        .collect();
    // Numbered indicators first, then the others in EBCDIC order.
    // Response indicators on generated lines are changed there.
    let mut all_inds = w.inds.clone();
    // A display file's separate indicator area defines the indicators.
    // (without INDDS), on the first generated input line.
    let indara = c.files.iter().any(|f| f.dev == crate::files::Dev::Workstn && f.indds.is_none() && f.ext.as_ref().is_some_and(|e| e.indara));
    if indara {
        let plan = super::genlines::plan(c);
        let nums = super::source::numbers(c, &plan);
        if let Some((_, first)) = plan.iter().zip(&nums.blocks).find(|(b, _)| b.kind == 'I') {
            for (k, v) in &mut all_inds {
                if k.bytes().all(|b| b.is_ascii_digit()) {
                    v.push((*first | GEN, 'D'));
                }
            }
        }
    }
    for (k, v, flag) in g.defs.iter().map(|(k, d)| (k, vec![*d], 'm')).chain(g.refs.iter().map(|(k, v)| (k, v.clone(), 'n'))) {
        if let Some(n) = k.strip_prefix("*IN") {
            all_inds.entry(n.to_string()).or_default().extend(v.iter().map(|(l, f)| (*l, if *f == 'M' { 'm' } else { flag })));
        }
    }
    // One only generated output lines name is referred to as `*IN`.
    let only_gen: Vec<String> = all_inds.iter().filter(|(_, v)| v.iter().all(|(_, f)| *f == 'n')).map(|(k, _)| k.clone()).collect();
    for k in only_gen {
        if let Some(v) = all_inds.remove(&k) {
            all_inds.entry("*IN".into()).or_default().extend(v.into_iter().map(|(l, _)| (l, ' ')));
        }
    }
    let mut ind_names: Vec<(&String, &Vec<At>)> = all_inds.iter().collect();
    ind_names.sort_by_key(|(k, _)| (ind_rank(k), (*k).clone()));
    let inds = ind_names
        .into_iter()
        .map(|(k, v)| {
            let mut refs = v.clone();
            sort_refs(c, &mut refs);
            // Set only (LR aside) is not used.
            let only_declared = refs.iter().all(|(_, f)| matches!(f, 'd' | 'm' | 'n') || (*f == 'D' && k.as_str() != "LR"));
            Item {
                msg: if only_declared { UNREF } else { "" },
                name: k.clone(),
                attrs: Vec::new(),
                refs,
                subs: Vec::new(),
            }
        })
        .collect();
    Model {
        files,
        proc_files,
        globals,
        procs,
        inds,
    }
}

/// A reference in its slot: the line right-aligned, then its flag.
pub(crate) fn slot(c: &Compiled, (line, flag): At) -> String {
    let flag = if flag == 'n' { ' ' } else { flag.to_ascii_uppercase() };
    if c.srcstmt() && line & GEN == 0 {
        let seq = c.stmt_seq(line & !super::refs::LATE);
        return format!("{seq:>11}{flag}");
    }
    format!("{:>7}{flag}", lno(c, line))
}

fn print(out: &mut Out, c: &Compiled, it: &Item) {
    // A copied subfield only counted.
    if it.name.is_empty() && it.attrs.is_empty() && it.refs.is_empty() {
        return;
    }
    let refs: Vec<String> = it.refs.iter().map(|r| slot(c, *r)).collect();
    let prefix = if it.msg.is_empty() { String::new() } else { format!("*{}", it.msg) };
    let first_attr = it.attrs.first().cloned().unwrap_or_default();
    if it.name.len() > 17 {
        out.line(' ', format!("{prefix:<9}{}...", it.name));
        let rest = Item {
            msg: "",
            name: String::new(),
            attrs: it.attrs.clone(),
            refs: it.refs.clone(),
            subs: Vec::new(),
        };
        print(out, c, &rest);
        for s in &it.subs {
            print(out, c, s);
        }
        return;
    }
    let head = format!("{prefix:<9}{:<18}{first_attr}", it.name);
    let mut chunks = refs.chunks(PER_LINE);
    let first = chunks.next().map(<[String]>::concat).unwrap_or_default();
    out.line(' ', if first.is_empty() { head } else { format!("{head:<REFS_AT$}{first}") });
    let mut more = it.attrs.iter().skip(1);
    for chunk in chunks {
        let a = more.next().cloned().unwrap_or_default();
        out.line(' ', format!("{:27}{a:<18}{}", "", chunk.concat()));
    }
    for a in more {
        out.line(' ', format!("{:27}{a}", ""));
    }
    for s in &it.subs {
        print(out, c, s);
    }
}

pub(crate) fn xref(out: &mut Out, c: &Compiled, m: &Model) {
    out.new_page(None);
    out.line('0', format!("{:26}{}", "", spaced("Cross Reference")));
    out.line('0', "      File and Record References:");
    out.line('0', "         File              Device             References (D=Defined)");
    out.line(' ', "           Record");
    if m.files.is_empty() {
        out.line('0', "         No references in the source.");
    }
    for it in &m.files {
        print(out, c, it);
    }
    out.line('0', "      Global Field References:");
    out.line('0', "         Field             Attributes         References (D=Defined M=Modified)");
    if m.globals.is_empty() {
        out.line('0', "         No references in the source.");
    }
    for it in &m.globals {
        print(out, c, it);
    }
    for (k, (name, items)) in m.procs.iter().enumerate() {
        // A procedure declared twice has one heading.
        if k == 0 || m.procs[k - 1].0 != *name {
            out.line(' ', format!("      Field References for subprocedure {name}"));
        }
        if let Some(fs) = m.proc_files.get(k).filter(|f| !f.is_empty()) {
            out.line('0', "         File              Device             References (D=Defined)");
            out.line(' ', "           Record");
            for it in fs {
                print(out, c, it);
            }
        }
        // A procedure whose calculations refer to no global name, or
        // one never ended, has no column headings.
        let unended = c.diags.iter().any(|d| d.msgid == "RNF1502");
        let calcs = !unended && c.module.procs.iter().find(|p| p.name.eq_ignore_ascii_case(name)).is_some_and(|p| {
            let inside = |l: u32| p.calcs.iter().any(|s| s.at.line == l);
            let refers = |it: &Item| it.refs.iter().any(|(l, _)| inside(*l));
            m.globals.iter().chain(&m.inds).any(|it| refers(it) || it.subs.iter().any(refers))
        });
        let files = m.proc_files.get(k).is_some_and(|f| !f.is_empty());
        if !items.is_empty() || calcs || files {
            out.line('0', "         Field             Attributes         References (D=Defined M=Modified)");
        }
        if items.is_empty() {
            out.line('0', "         No references in the source.");
        }
        for it in items {
            print(out, c, it);
        }
    }
    out.line('0', "      Indicator References:");
    out.line('0', "         Indicator                            References (D=Defined M=Modified)");
    for it in &m.inds {
        print(out, c, it);
    }
    out.end_line('0', format!("{:7}{}", "", "* * * * *   E N D   O F   C R O S S   R E F E R E N C E   * * * * *"));
}

/// The external references: procedures bound by name (prototypes no
/// procedure of the module defines) with their references, and the
/// imported and exported fields with where they are defined.
/// A field of the external references: name, attributes, where defined.
pub(crate) type ExtField = (String, String, String);
/// The external references: bound procedures with their references,
/// imported fields, exported fields.
pub(crate) type Externals = (Vec<(String, Vec<String>)>, Vec<ExtField>, Vec<ExtField>);

pub(crate) fn externals(c: &Compiled) -> Externals {
    let w = super::refs::collect(c);
    let mut procs = Vec::new();
    for (id, s) in c.table.syms.iter().enumerate() {
        let Some(Kind::Proto(pi)) = s.kind else { continue };
        let p = &c.table.protos[pi];
        let local = c.module.procs.iter().any(|q| q.name.eq_ignore_ascii_case(&s.name));
        // A procedure pointer named by EXTPROC is no bound procedure.
        // EXTPROC as written, even beside EXTPGM.
        let written = c
            .module
            .defs
            .iter()
            .chain(c.module.procs.iter().flat_map(|q| q.defs.iter()))
            .find(|d| d.kind == rpgfront::ast::DefKind::Pr && d.name.eq_ignore_ascii_case(&s.name))
            .and_then(|d| d.kw("EXTPROC"))
            .and_then(|k| k.args.first())
            .map(|a| a.toks.iter().map(rpgfront::lex::Tok::text).collect::<String>())
            .filter(|t| t.starts_with('\''))
            .map(|t| t.trim_matches('\'').to_string());
        let extproc = p.extproc.clone().or(written);
        if local || (p.extpgm.is_some() && extproc.is_none()) || p.extproc_word {
            continue;
        }
        let name = extproc.unwrap_or_else(|| s.name.clone());
        let mut refs = vec![(p.def_line, ' ')];
        refs.extend(w.syms.get(&id).into_iter().flatten().map(|(l, _)| (*l, ' ')));
        sort_refs(c, &mut refs);
        let slots = refs.iter().map(|r| slot(c, *r)).collect();
        procs.push((name, slots));
    }
    let fields = |want: Storage| {
        let mut v: Vec<(Vec<u8>, (String, String, String))> = c
            .table
            .syms
            .iter()
            .filter(|s| s.storage == want && !s.name.starts_with('_'))
            .map(|s| {
                let attr = attrs_of(c, s).first().cloned().unwrap_or_default();
                (ebcdic_key(&s.name), (s.name.clone(), attr, format!("{:>6}", lno(c, s.def_line))))
            })
            .collect();
        v.sort_by(|a, b| a.0.cmp(&b.0));
        v.into_iter().map(|(_, x)| x).collect::<Vec<_>>()
    };
    (procs, fields(Storage::Import), fields(Storage::Export))
}

/// Whether an operation reads or writes a program-described file, or
/// output or input specifications name it: one that is only opened,
/// closed or posted counts as unreferenced.
fn io_used(c: &Compiled, file: &str) -> bool {
    let calcs = c.module.calcs.iter().chain(c.module.procs.iter().flat_map(|p| p.calcs.iter()));
    let by_op = calcs
        .filter(|s| !matches!(rpgfront::parse::ops::split_suffix(&s.op.code).0, "OPEN" | "CLOSE" | "POST" | "ACQ" | "REL" | "NEXT"))
        .any(|s| s.op.f2.iter().chain(s.op.result.iter()).any(|e| e.name().is_some_and(|n| n.eq_ignore_ascii_case(file))));
    by_op
        || c.table.syms.iter().any(|s| s.fromfile.eq_ignore_ascii_case(file) || s.tofile.eq_ignore_ascii_case(file))
        || c.module.outputs.iter().any(|o| o.name.eq_ignore_ascii_case(file))
        || c.module.inputs.iter().any(|i| i.name.eq_ignore_ascii_case(file))
}

/// The key fields of keyed externally described files.
pub(crate) fn keys(out: &mut Out, c: &Compiled) {
    let files: Vec<(usize, &crate::files::File)> =
        c.files.iter().enumerate().filter(|(_, f)| f.keyed && f.ext.is_some() && f.scope == 0).collect();
    let any = files.iter().any(|(_, f)| c.table.recfmts.iter().any(|r| r.file_name == f.name && !r.fmt.keys.is_empty()));
    if !any {
        return;
    }
    out.new_page(None);
    out.line('0', format!("{:20}{}", "", spaced("Key Field Information")));
    out.line('0', "         File           Internal    External");
    out.line(' ', "           Record       field name  field name  Attributes");
    for (k, f) in files {
        out.line('0', format!("{:>6}  {}", k + 1, f.name).replacen(' ', "  ", 1));
        for r in c.table.recfmts.iter().filter(|r| r.file_name == f.name) {
            if r.name == r.fmt.name {
                out.line(' ', format!("           {}", r.name));
            } else {
                out.line(' ', format!("           {} is the RPG name of the external format {}.", r.name, r.fmt.name));
            }
            for key in &r.fmt.keys {
                let Some(x) = r.fmt.fields.iter().find(|x| x.name == *key) else { continue };
                let (kind, len, signed) = match &x.ty {
                    Ty::Packed(d, s) => ("PACK", format!("{d},{s}"), " SIGNED"),
                    Ty::Zoned(d, s) => ("ZONE", format!("{d},{s}"), " SIGNED"),
                    Ty::Bin(d, s) => ("BINARY", format!("{d},{s}"), " SIGNED"),
                    Ty::Int(d) => ("INTEGER", format!("{d},0"), " SIGNED"),
                    Ty::Char(n) | Ty::VarChar(n, _) => ("CHAR", n.to_string(), ""),
                    t => ("CHAR", t.size(&|i| c.table.ds_len(i)).to_string(), ""),
                };
                out.line(' ', format!("{:24}{:<24}{kind:<4}{len:>9}{signed}", "", key));
            }
        }
    }
    out.end_line('0', " * * * * *   E N D   O F   K E Y   F I E L D   I N F O R M A T I O N   * * * * *");
}

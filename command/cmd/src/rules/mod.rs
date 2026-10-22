// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The rules a definition must satisfy (`rules.toml`), and the checks
//! that apply them to a [`CmdDef`].

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use serde::Deserialize;

use crate::def::{CmdDef, DepCtl, DepItem, NbrTrue, Parm, ParmType, PmtCtlRef};
use crate::error::Diag;

/// The source of the rule table.
pub const RULES: &str = include_str!("../../rules.toml");

/// One rule.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Rule {
    /// The rule's name, as the checks use it.
    pub id: String,
    /// The diagnostic it sends.
    pub msgid: String,
    /// Its text, with `{n}` for the details.
    pub text: String,
}

#[derive(Deserialize)]
struct Table {
    rule: Vec<Rule>,
}

/// Every rule, by name.
pub fn table() -> &'static HashMap<String, Rule> {
    static T: OnceLock<HashMap<String, Rule>> = OnceLock::new();
    T.get_or_init(|| {
        let t: Table = toml::from_str(RULES).expect("rules.toml parses");
        t.rule.into_iter().map(|r| (r.id.clone(), r)).collect()
    })
}

/// The diagnostic of a rule with its details filled in.
#[must_use]
pub fn diag(rule: &str, details: &[&str]) -> Diag {
    let r = table()
        .get(rule)
        .unwrap_or_else(|| panic!("rule {rule} is in rules.toml"));
    let mut text = r.text.clone();
    for (i, d) in details.iter().enumerate() {
        text = text.replace(&format!("{{{i}}}"), d);
    }
    let kwd = details.first().copied().unwrap_or("");
    Diag::new(&r.msgid).data(&[(kwd, 10)]).detail(text)
}

/// Whether a keyword is well formed: 1 to 10 characters, a letter
/// first, then letters, digits, `$`, `#` or `@`.
#[must_use]
pub fn kwd_ok(k: &str) -> bool {
    let b = k.as_bytes();
    !b.is_empty()
        && b.len() <= 10
        && b[0].is_ascii_alphabetic()
        && b.iter()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'$' | b'#' | b'@'))
}

/// Check a whole definition; every rule broken.
#[must_use]
pub fn check(def: &CmdDef) -> Vec<Diag> {
    let mut out = Vec::new();
    if def.parms.len() > 99 {
        out.push(diag("too-many-parms", &[]));
    }
    if let Some(m) = def.maxpos
        && usize::from(m) > def.parms.len()
    {
        out.push(diag("maxpos-over", &[&m.to_string()]));
    }
    let mut seen = HashSet::new();
    let mut after_key = false;
    for p in &def.parms {
        if p.kwd.is_empty() {
            out.push(diag("kwd-required", &[]));
        } else if !kwd_ok(&p.kwd) {
            out.push(diag("kwd-bad", &[&p.kwd]));
        } else if !seen.insert(p.kwd.clone()) {
            out.push(diag("kwd-dup", &[&p.kwd]));
        }
        if p.keyparm {
            if after_key {
                out.push(diag("keyparm-order", &[&p.kwd]));
            }
        } else {
            after_key = true;
        }
        check_parm(def, p, &p.kwd, 1, &mut out);
    }
    for d in &def.deps {
        let mut names: Vec<&str> = Vec::new();
        if let DepCtl::Item { item } = &d.ctl {
            names.push(item_kwd(item));
        }
        names.extend(d.parms.iter().map(item_kwd));
        for n in names {
            if def.parm(n).is_none() {
                out.push(diag("dep-kwd", &[n]));
            }
        }
        if let NbrTrue::Count { n, .. } = d.nbrtrue
            && usize::from(n) > d.parms.len()
        {
            out.push(diag("dep-nbrtrue", &[&n.to_string()]));
        }
    }
    let mut labels: HashSet<&str> = HashSet::new();
    for c in &def.pmtctls {
        if def.parm(&c.ctl).is_none() {
            out.push(diag("pmtctl-ctl", &[&c.ctl]));
        }
        let first = labels.insert(&c.label);
        if first && c.lglrel.is_some() {
            out.push(diag("lglrel-first", &[&c.label]));
        }
    }
    out
}

fn item_kwd(i: &DepItem) -> &str {
    match i {
        DepItem::Specified { kwd } | DepItem::Cond { kwd, .. } => kwd,
    }
}

/// Check one parameter, element or qualifier; `depth` counts the list
/// levels above it.
fn check_parm(def: &CmdDef, p: &Parm, kwd: &str, depth: u32, out: &mut Vec<Diag>) {
    let ty = p.ty.text();
    if let Some(l) = p.len {
        match p.ty {
            ParmType::Dec => {
                if l.len == 0 || l.len > 24 || u32::from(l.dec) > l.len {
                    out.push(diag("dec-len", &[&l.len.to_string(), &l.dec.to_string()]));
                }
            }
            ParmType::Char | ParmType::PName | ParmType::Cmd | ParmType::CmdStr | ParmType::X => {
                if l.len == 0 || l.len > p.ty.max_len() {
                    out.push(diag("len-bad", &[&ty, &l.len.to_string()]));
                }
            }
            ParmType::Name
            | ParmType::Generic
            | ParmType::SName
            | ParmType::CName
            | ParmType::VarName => {
                if l.len == 0 || l.len > p.ty.max_len() {
                    out.push(diag("len-bad", &[&ty, &l.len.to_string()]));
                }
            }
            _ => out.push(diag("len-bad", &[&ty, &l.len.to_string()])),
        }
    }
    if p.rtnval {
        if !matches!(p.ty, ParmType::Char | ParmType::Dec | ParmType::Lgl) {
            out.push(diag("rtnval-type", &[kwd, &ty]));
        }
        if p.dft.is_some()
            || p.min > 0
            || !p.values.is_empty()
            || p.rel.is_some()
            || p.range.is_some()
            || !p.spcval.is_empty()
            || !p.sngval.is_empty()
        {
            out.push(diag("rtnval-attrs", &[kwd]));
        }
    }
    if p.ty == ParmType::CmdStr
        && (p.max > 1
            || p.rstd
            || !p.values.is_empty()
            || p.rel.is_some()
            || p.range.is_some()
            || !p.spcval.is_empty()
            || !p.sngval.is_empty())
    {
        out.push(diag("cmdstr-attrs", &[kwd]));
    }
    if p.ty == ParmType::Cmd && p.max > 1 {
        out.push(diag("cmd-list", &[kwd]));
    }
    if p.min > p.max || p.max > 300 || p.max == 0 {
        out.push(diag(
            "min-max",
            &[&p.min.to_string(), &p.max.to_string(), kwd],
        ));
    }
    if p.dft.is_some() && p.min > 0 {
        out.push(diag("dft-with-min", &[kwd]));
    }
    if p.dft.is_some()
        && matches!(
            p.ty,
            ParmType::VarName | ParmType::Null | ParmType::ZeroElem
        )
    {
        out.push(diag("dft-type", &[kwd]));
    }
    if p.rstd && p.values.is_empty() && p.spcval.is_empty() && p.sngval.is_empty() {
        out.push(diag("rstd-values", &[kwd]));
    }
    if p.rel.is_some() && p.range.is_some() {
        out.push(diag("rel-range", &[kwd]));
    }
    if let Some((lo, hi)) = &p.range
        && let (Ok(a), Ok(b)) = (qdecimal::Decimal::parse(lo), qdecimal::Decimal::parse(hi))
        && a > b
    {
        out.push(diag("range-order", &[kwd]));
    }
    if !p.sngval.is_empty() && p.max <= 1 && !p.ty.is_group() {
        out.push(diag("sngval-scalar", &[kwd]));
    }
    if p.constant.is_some()
        && (p.dft.is_some() || p.min > 0 || p.rstd || !p.values.is_empty() || p.rtnval)
    {
        out.push(diag("constant-attrs", &[kwd]));
    }
    if p.vary.is_some() && !matches!(p.ty, ParmType::Char | ParmType::PName) {
        out.push(diag("vary-type", &[kwd]));
    }
    if let Some((t, _)) = &p.prompt
        && t.chars().count() > 30
    {
        out.push(diag("prompt-len", &[kwd]));
    }
    match &p.pmtctl {
        Some(PmtCtlRef::Label { label }) if def.pmtctl(label).is_empty() => {
            out.push(diag("pmtctl-missing", &[label, kwd]));
        }
        _ => {}
    }
    match &p.ty {
        ParmType::Elem { label } => {
            let d = depth + u32::from(p.max > 1);
            if d > 3 {
                out.push(diag("elem-depth", &[kwd]));
            }
            if p.items.len() > 300 {
                out.push(diag("elem-count", &[label]));
            }
            for (i, e) in p.items.iter().enumerate() {
                let name = format!("{kwd} element {}", i + 1);
                check_parm(def, e, &name, d, out);
            }
        }
        ParmType::Qual { .. } => {
            if p.items.len() > 5 {
                out.push(diag("qual-count", &[kwd]));
            }
            for q in &p.items {
                if !matches!(
                    q.ty,
                    ParmType::Name
                        | ParmType::Generic
                        | ParmType::Char
                        | ParmType::PName
                        | ParmType::SName
                        | ParmType::CName
                ) {
                    out.push(diag("qual-first", &[kwd]));
                    break;
                }
                let name = format!("{kwd} qualifier");
                check_parm(def, q, &name, depth, out);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_loads() {
        let t = table();
        assert!(t.len() > 30);
        let d = diag("kwd-dup", &["LIB"]);
        assert_eq!(d.msgid, "CPD0214");
        assert!(d.detail.contains("LIB"));
        assert!(kwd_ok("LIB") && kwd_ok("A$#@1") && !kwd_ok("1A") && !kwd_ok("ABCDEFGHIJK"));
    }
}

// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The command-definition source compiler: `CMD`, `PARM`, `ELEM`,
//! `QUAL`, `DEP` and `PMTCTL` statements into a [`CmdDef`], then the
//! rules. Every diagnostic is collected; the definition is returned
//! only when there is none.

use std::collections::HashMap;

use crate::def::{
    Allow, Case, Choice, CmdDef, Dep, DepCtl, DepItem, DspInput, FileUse, InlPmtLen, Len, LglRel,
    ListDspl, Mode, NbrTrue, Parm, ParmType, PassVal, PmtCtl, PmtCtlRef, Qual, RelOp, Special,
    ThdSafe, Vary,
};
use crate::error::Diag;
use crate::rules::{self, diag};
use crate::token::{self, Options, Tok};

/// One statement of the source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Statement {
    /// Its label.
    pub label: Option<String>,
    /// `CMD`, `PARM`, `ELEM`, `QUAL`, `DEP` or `PMTCTL`.
    pub kind: String,
    /// Its keywords with their arguments.
    pub kws: Vec<(String, Vec<Tok>)>,
    /// The first source line.
    pub line: usize,
}

/// The statements of a source.
pub fn statements(text: &str) -> Result<Vec<Statement>, Vec<Diag>> {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = Vec::new();
    let mut errs = Vec::new();
    for j in token::join(&lines) {
        match token::tokenize(&j.text, Options::default()) {
            Ok(t) => {
                let mut kws = Vec::new();
                let mut bad = false;
                for tok in t.toks {
                    match tok {
                        Tok::Kw { name, args, .. } => kws.push((name, args)),
                        other => {
                            errs.push(at(diag("value-bad", &[&t.name, &other.text()]), j.first));
                            bad = true;
                        }
                    }
                }
                if !bad {
                    out.push(Statement {
                        label: t.label,
                        kind: t.name,
                        kws,
                        line: j.first,
                    });
                }
            }
            Err(e) => {
                for d in e.diags {
                    errs.push(at(d, j.first));
                }
            }
        }
    }
    if errs.is_empty() { Ok(out) } else { Err(errs) }
}

/// A diagnostic noted with its source line.
fn at(mut d: Diag, line: usize) -> Diag {
    d.detail = format!("Line {line}: {}", d.detail);
    d
}

/// Compile a definition source: the statements, the structures, the
/// rules. The command name comes from the caller (`CRTCMD`).
pub fn compile(name: &str, text: &str) -> Result<CmdDef, Vec<Diag>> {
    let stmts = statements(text)?;
    let mut b = Builder {
        def: CmdDef {
            name: name.to_ascii_uppercase(),
            ..CmdDef::default()
        },
        errs: Vec::new(),
        groups: Vec::new(),
        cmd_seen: false,
        last_pmtctl: None,
    };
    for s in &stmts {
        b.statement(s);
    }
    if !b.cmd_seen {
        b.errs.push(diag("cmd-missing", &[]));
    }
    b.resolve_groups();
    if !b.errs.is_empty() {
        return Err(b.errs);
    }
    let errs = rules::check(&b.def);
    if errs.is_empty() {
        Ok(b.def)
    } else {
        Err(errs)
    }
}

/// A group of `ELEM` or `QUAL` statements under a label.
struct Group {
    label: String,
    qual: bool,
    items: Vec<Parm>,
    line: usize,
}

struct Builder {
    def: CmdDef,
    errs: Vec<Diag>,
    groups: Vec<Group>,
    cmd_seen: bool,
    last_pmtctl: Option<String>,
}

impl Builder {
    fn err(&mut self, d: Diag, line: usize) {
        self.errs.push(at(d, line));
    }

    fn statement(&mut self, s: &Statement) {
        match s.kind.as_str() {
            "CMD" => {
                if self.cmd_seen || !self.def.parms.is_empty() || !self.groups.is_empty() {
                    self.err(diag("cmd-first", &[]), s.line);
                }
                self.cmd_seen = true;
                self.cmd(s);
            }
            "PARM" => {
                if !self.cmd_seen {
                    self.err(diag("cmd-first", &[]), s.line);
                    self.cmd_seen = true;
                }
                let p = self.parm(s, true);
                self.def.parms.push(p);
            }
            "ELEM" | "QUAL" => {
                let p = self.parm(s, false);
                let qual = s.kind == "QUAL";
                match (&s.label, self.groups.last_mut()) {
                    (Some(l), _) => {
                        if self.groups.iter().any(|g| g.label == *l)
                            || self.def.pmtctls.iter().any(|c| c.label == *l)
                        {
                            self.err(diag("label-dup", &[l]), s.line);
                        }
                        self.groups.push(Group {
                            label: l.clone(),
                            qual,
                            items: vec![p],
                            line: s.line,
                        });
                    }
                    (None, Some(g)) if g.qual == qual => g.items.push(p),
                    (None, _) => self.err(diag("group-orphan", &[&s.kind]), s.line),
                }
            }
            "DEP" => self.dep(s),
            "PMTCTL" => self.pmtctl(s),
            other => self.err(diag("statement-unknown", &[other]), s.line),
        }
    }

    fn cmd(&mut self, s: &Statement) {
        for (k, args) in &s.kws {
            let v = word(args);
            match k.as_str() {
                "PROMPT" => self.def.prompt = text_of(args),
                "TEXT" => self.def.text = text_of(args),
                "ALLOW" => {
                    let mut bits = 0;
                    for a in args {
                        match Allow::flag(&a.text()) {
                            Some(f) => bits |= f,
                            None => self.err(diag("value-bad", &[k, &a.text()]), s.line),
                        }
                    }
                    self.def.allow = Allow(bits);
                }
                "MODE" => {
                    let mut bits = 0;
                    for a in args {
                        match Mode::flag(&a.text()) {
                            Some(f) => bits |= f,
                            None => self.err(diag("value-bad", &[k, &a.text()]), s.line),
                        }
                    }
                    self.def.mode = Mode(bits);
                }
                "MAXPOS" => match v.as_deref() {
                    Some("*NONE") => self.def.maxpos = None,
                    Some(n) => match n.parse::<u8>() {
                        Ok(n) if n <= 99 => self.def.maxpos = Some(n),
                        _ => self.err(diag("maxpos-value", &[n]), s.line),
                    },
                    None => self.err(diag("value-bad", &[k, ""]), s.line),
                },
                "PMTOVRPGM" => self.def.pmtovrpgm = v.map(|q| Qual::parse(&q)),
                "PMTFILE" => self.def.pmtfile = v.map(|q| Qual::parse(&q)),
                "MSGF" => {
                    if let Some(q) = v {
                        self.def.msgf = Qual::parse(&q);
                    }
                }
                "PRDLIB" => self.def.prdlib = v.filter(|l| l != "*NONE"),
                "CURLIB" => self.def.curlib = v.filter(|l| l != "*NOCHG"),
                "VLDCKR" => self.def.vldckr = v.filter(|p| p != "*NONE").map(|q| Qual::parse(&q)),
                "HLPPNLGRP" => {
                    self.def.hlppnlgrp = v.filter(|p| p != "*NONE").map(|q| Qual::parse(&q));
                }
                "HLPID" => self.def.hlpid = v.filter(|p| p != "*NONE"),
                "THDSAFE" => {
                    self.def.thdsafe = match v.as_deref() {
                        Some("*NO") | None => ThdSafe::No,
                        Some("*COND") => ThdSafe::Cond,
                        Some("*YES") => ThdSafe::Yes,
                        Some(o) => {
                            self.err(diag("value-bad", &[k, o]), s.line);
                            ThdSafe::No
                        }
                    };
                }
                "HLPSHELF" | "HLPSCHIDX" | "EXECBATCH" | "PRDLIB2" => {}
                other => self.err(diag("keyword-unknown", &[other, "CMD"]), s.line),
            }
        }
    }

    /// A `PARM`, `ELEM` or `QUAL` statement.
    #[allow(clippy::too_many_lines)] // one arm per IBM keyword
    fn parm(&mut self, s: &Statement, is_parm: bool) -> Parm {
        let mut p = Parm::default();
        let kind = s.kind.as_str();
        for (k, args) in &s.kws {
            let v = word(args);
            let bad = |b: &mut Self, val: &str| b.err(diag("value-bad", &[k, val]), s.line);
            match k.as_str() {
                "KWD" => {
                    if is_parm {
                        p.kwd = v.unwrap_or_default();
                    } else {
                        self.err(diag("elem-kwd", &[]), s.line);
                    }
                }
                "TYPE" => match v.as_deref().and_then(ParmType::parse) {
                    Some(t) => p.ty = t,
                    None => bad(self, v.as_deref().unwrap_or("")),
                },
                "LEN" => {
                    let nums: Vec<Option<u32>> =
                        args.iter().map(|a| a.text().parse::<u32>().ok()).collect();
                    match nums.as_slice() {
                        [Some(n)] => p.len = Some(Len::chars(*n)),
                        [Some(n), Some(d)] => {
                            p.len = Some(Len {
                                len: *n,
                                dec: u8::try_from(*d).unwrap_or(255),
                            });
                        }
                        _ => bad(self, &Tok::join(args)),
                    }
                }
                "RTNVAL" => p.rtnval = yes(args),
                "RSTD" => p.rstd = yes(args),
                "DFT" => p.dft = Some(Tok::join(args)),
                "VALUES" => p.values = args.iter().map(Tok::text).collect(),
                "REL" => match args.as_slice() {
                    [op, val] => match RelOp::parse(&op.text()) {
                        Some(op) => p.rel = Some((op, val.text())),
                        None => bad(self, &Tok::join(args)),
                    },
                    _ => bad(self, &Tok::join(args)),
                },
                "RANGE" => match args.as_slice() {
                    [lo, hi] => p.range = Some((lo.text(), hi.text())),
                    _ => bad(self, &Tok::join(args)),
                },
                "SPCVAL" | "SNGVAL" => {
                    let mut list = Vec::new();
                    for a in args {
                        let items = match a {
                            Tok::Paren(items) => items.clone(),
                            other => vec![other.clone()],
                        };
                        match items.as_slice() {
                            [from] => list.push(Special {
                                from: from.text(),
                                to: None,
                            }),
                            [from, to] => list.push(Special {
                                from: from.text(),
                                to: Some(to.text()),
                            }),
                            _ => bad(self, &a.text()),
                        }
                    }
                    if k == "SPCVAL" {
                        p.spcval = list;
                    } else {
                        p.sngval = list;
                    }
                }
                "MIN" => match v.as_deref().and_then(|n| n.parse::<u16>().ok()) {
                    Some(n) => p.min = n,
                    None => bad(self, v.as_deref().unwrap_or("")),
                },
                "MAX" => match v.as_deref().and_then(|n| n.parse::<u16>().ok()) {
                    Some(n) => p.max = n,
                    None => bad(self, v.as_deref().unwrap_or("")),
                },
                "ALWUNPRT" => p.alwunprt = yes(args),
                "ALWVAR" => p.alwvar = yes(args),
                "PGM" => p.pgm = yes(args),
                "DTAARA" => p.dtaara = yes(args),
                "FILE" => {
                    p.file = match v.as_deref() {
                        Some("*NO") | None => FileUse::No,
                        Some("*IN") => FileUse::In,
                        Some("*OUT") => FileUse::Out,
                        Some("*UPD") => FileUse::Upd,
                        Some("*INOUT") => FileUse::InOut,
                        Some("*UNSPFD") => FileUse::Unspfd,
                        Some(o) => {
                            bad(self, o);
                            FileUse::No
                        }
                    };
                }
                "FULL" => p.full = yes(args),
                "EXPR" => p.expr = yes(args),
                "VARY" => {
                    let words: Vec<String> = args.iter().map(Tok::text).collect();
                    p.vary = match words
                        .iter()
                        .map(String::as_str)
                        .collect::<Vec<_>>()
                        .as_slice()
                    {
                        ["*NO"] => None,
                        ["*YES"] | ["*YES", "*INT2"] => Some(Vary::Int2),
                        ["*YES", "*INT4"] => Some(Vary::Int4),
                        _ => {
                            bad(self, &words.join(" "));
                            None
                        }
                    };
                }
                "PASSATR" => p.passatr = yes(args),
                "PASSVAL" => {
                    p.passval = match v.as_deref() {
                        Some("*NULL") => PassVal::Null,
                        _ => PassVal::Dft,
                    };
                }
                "CASE" => {
                    p.case = match v.as_deref() {
                        Some("*MIXED") => Case::Mixed,
                        _ => Case::Mono,
                    };
                }
                "LISTDSPL" => {
                    p.listdspl = match v.as_deref() {
                        Some("*LEN") | None => ListDspl::Len,
                        Some(n) => match n.parse::<u16>() {
                            Ok(n) => ListDspl::N { n },
                            Err(_) => {
                                bad(self, n);
                                ListDspl::Len
                            }
                        },
                    };
                }
                "CHOICE" => {
                    p.choice = match args.first() {
                        None => Choice::Values,
                        Some(Tok::Word(w)) if w == "*VALUES" => Choice::Values,
                        Some(Tok::Word(w)) if w == "*NONE" => Choice::None,
                        Some(Tok::Word(w)) if w == "*PGM" => Choice::Pgm,
                        Some(Tok::Str(t)) => Choice::Text { text: t.clone() },
                        Some(Tok::Word(w)) => Choice::Text { text: w.clone() },
                        Some(o) => {
                            bad(self, &o.text());
                            Choice::Values
                        }
                    };
                }
                "CHOICEPGM" => p.choicepgm = v.filter(|x| x != "*NONE").map(|q| Qual::parse(&q)),
                "PMTCTL" => {
                    p.pmtctl = match v.as_deref() {
                        Some("*NONE") | None => None,
                        Some("*PMTRQS") => Some(PmtCtlRef::PmtRqs),
                        Some(l) => Some(PmtCtlRef::Label {
                            label: l.to_string(),
                        }),
                    };
                }
                "PMTCTLPGM" => p.pmtctlpgm = v.filter(|x| x != "*NONE").map(|q| Qual::parse(&q)),
                "KEYPARM" => p.keyparm = yes(args),
                "PROMPT" => match args.as_slice() {
                    [Tok::Str(t)] => p.prompt = Some((t.clone(), None)),
                    [Tok::Str(t), Tok::Num(n)] => {
                        p.prompt = Some((t.clone(), n.parse().ok()));
                    }
                    [Tok::Word(w)] | [Tok::Word(w), _] if w == "*NONE" => p.prompt = None,
                    _ => bad(self, &Tok::join(args)),
                },
                "CONSTANT" => p.constant = Some(Tok::join(args)),
                "DSPINPUT" => {
                    p.dspinput = match v.as_deref() {
                        Some("*YES") | None => DspInput::Yes,
                        Some("*PROMPT") => DspInput::Prompt,
                        Some("*NO") => DspInput::No,
                        Some(o) => {
                            bad(self, o);
                            DspInput::Yes
                        }
                    };
                }
                "INLPMTLEN" => {
                    p.inlpmtlen = match v.as_deref() {
                        Some("*CALC") | None => Some(InlPmtLen::Calc),
                        Some("*PWD") => Some(InlPmtLen::Pwd),
                        Some(n) => match n.parse::<u16>() {
                            Ok(n) => Some(InlPmtLen::N { n }),
                            Err(_) => {
                                bad(self, n);
                                None
                            }
                        },
                    };
                }
                other => self.err(diag("keyword-unknown", &[other, kind]), s.line),
            }
        }
        if is_parm && p.kwd.is_empty() {
            self.err(diag("kwd-required", &[]), s.line);
        }
        p
    }

    fn dep(&mut self, s: &Statement) {
        let mut d = Dep {
            ctl: DepCtl::Always,
            parms: Vec::new(),
            nbrtrue: NbrTrue::All,
            msgid: None,
        };
        for (k, args) in &s.kws {
            match k.as_str() {
                "CTL" => match args.as_slice() {
                    [Tok::Word(w)] if w == "*ALWAYS" => d.ctl = DepCtl::Always,
                    [one] => match dep_item(one) {
                        Some(item) => d.ctl = DepCtl::Item { item },
                        None => self.err(diag("value-bad", &[k, &one.text()]), s.line),
                    },
                    // `CTL(&KWD *EQ value)` without the inner parentheses.
                    [Tok::Var(_), _, _] => match dep_item(&Tok::Paren(args.clone())) {
                        Some(item) => d.ctl = DepCtl::Item { item },
                        None => self.err(diag("value-bad", &[k, &Tok::join(args)]), s.line),
                    },
                    _ => self.err(diag("value-bad", &[k, &Tok::join(args)]), s.line),
                },
                "PARM" => {
                    for a in args {
                        match dep_item(a) {
                            Some(item) => d.parms.push(item),
                            None => self.err(diag("value-bad", &[k, &a.text()]), s.line),
                        }
                    }
                }
                "NBRTRUE" => match nbrtrue(args) {
                    Some(n) => d.nbrtrue = n,
                    None => self.err(diag("value-bad", &[k, &Tok::join(args)]), s.line),
                },
                "MSGID" => d.msgid = word(args),
                other => self.err(diag("keyword-unknown", &[other, "DEP"]), s.line),
            }
        }
        self.def.deps.push(d);
    }

    fn pmtctl(&mut self, s: &Statement) {
        let label = match (&s.label, &self.last_pmtctl) {
            (Some(l), _) => {
                if self.groups.iter().any(|g| g.label == *l)
                    || self.def.pmtctls.iter().any(|c| c.label == *l)
                {
                    self.err(diag("label-dup", &[l]), s.line);
                }
                l.clone()
            }
            (None, Some(l)) => l.clone(),
            (None, None) => {
                self.err(diag("group-orphan", &[&s.kind]), s.line);
                String::new()
            }
        };
        let mut c = PmtCtl {
            label: label.clone(),
            ctl: String::new(),
            conds: Vec::new(),
            nbrtrue: NbrTrue::All,
            lglrel: None,
        };
        for (k, args) in &s.kws {
            match k.as_str() {
                "CTL" => c.ctl = word(args).unwrap_or_default(),
                "COND" => {
                    for a in args {
                        let items = match a {
                            Tok::Paren(items) => items.clone(),
                            other => vec![other.clone()],
                        };
                        match items.as_slice() {
                            [op, val] => match RelOp::parse(&op.text()) {
                                Some(op) => c.conds.push((op, val.text())),
                                None => self.err(diag("value-bad", &[k, &a.text()]), s.line),
                            },
                            [Tok::Word(w)] if w == "*SPCFD" || w == "*UNSPCFD" => {
                                c.conds.push((RelOp::Eq, w.clone()));
                            }
                            _ => self.err(diag("value-bad", &[k, &a.text()]), s.line),
                        }
                    }
                }
                "NBRTRUE" => match nbrtrue(args) {
                    Some(n) => c.nbrtrue = n,
                    None => self.err(diag("value-bad", &[k, &Tok::join(args)]), s.line),
                },
                "LGLREL" => {
                    c.lglrel = match word(args).as_deref() {
                        Some("*AND") => Some(LglRel::And),
                        Some("*OR") => Some(LglRel::Or),
                        _ => {
                            self.err(diag("value-bad", &[k, &Tok::join(args)]), s.line);
                            None
                        }
                    };
                }
                other => self.err(diag("keyword-unknown", &[other, "PMTCTL"]), s.line),
            }
        }
        self.last_pmtctl = Some(label);
        self.def.pmtctls.push(c);
    }

    /// Attach the `ELEM` and `QUAL` groups to the parameters that name
    /// them, nested groups included.
    fn resolve_groups(&mut self) {
        let groups: HashMap<String, (bool, Vec<Parm>, usize)> = self
            .groups
            .iter()
            .map(|g| (g.label.clone(), (g.qual, g.items.clone(), g.line)))
            .collect();
        let pmt_labels: Vec<String> = self.def.pmtctls.iter().map(|c| c.label.clone()).collect();
        let mut parms = std::mem::take(&mut self.def.parms);
        let mut errs = Vec::new();
        for p in &mut parms {
            resolve(p, &groups, &pmt_labels, 0, &mut errs);
        }
        self.def.parms = parms;
        self.errs.extend(errs);
    }
}

/// Resolve `TYPE(label)` on one parameter and, recursively, its items.
fn resolve(
    p: &mut Parm,
    groups: &HashMap<String, (bool, Vec<Parm>, usize)>,
    pmt_labels: &[String],
    depth: u32,
    errs: &mut Vec<Diag>,
) {
    let label = match &p.ty {
        ParmType::Elem { label } | ParmType::Qual { label } => label.clone(),
        _ => return,
    };
    if depth > 4 {
        errs.push(diag("elem-depth", &[&label]));
        return;
    }
    match groups.get(&label) {
        Some((qual, items, _)) => {
            p.ty = if *qual {
                ParmType::Qual {
                    label: label.clone(),
                }
            } else {
                ParmType::Elem {
                    label: label.clone(),
                }
            };
            p.items.clone_from(items);
            for i in &mut p.items {
                resolve(i, groups, pmt_labels, depth + 1, errs);
            }
        }
        None if pmt_labels.contains(&label) => errs.push(diag("label-kind", &[&label])),
        None => errs.push(diag("label-missing", &[&label])),
    }
}

/// A `&KWD` or `(&KWD *EQ value)` of a `DEP`.
fn dep_item(t: &Tok) -> Option<DepItem> {
    match t {
        Tok::Var(k) => Some(DepItem::Specified { kwd: k.clone() }),
        Tok::Paren(items) => match items.as_slice() {
            [Tok::Var(k), op, val] => Some(DepItem::Cond {
                kwd: k.clone(),
                op: RelOp::parse(&op.text())?,
                value: val.text(),
            }),
            [Tok::Var(k)] => Some(DepItem::Specified { kwd: k.clone() }),
            _ => None,
        },
        _ => None,
    }
}

/// `NBRTRUE(*ALL)`, `(*NONE)`, `((*EQ 2))`.
fn nbrtrue(args: &[Tok]) -> Option<NbrTrue> {
    match args {
        [Tok::Word(w)] if w == "*ALL" => Some(NbrTrue::All),
        [Tok::Word(w)] if w == "*NONE" => Some(NbrTrue::None),
        [Tok::Paren(items)] => match items.as_slice() {
            [op, Tok::Num(n)] => Some(NbrTrue::Count {
                op: RelOp::parse(&op.text())?,
                n: n.parse().ok()?,
            }),
            _ => None,
        },
        [op, Tok::Num(n)] => Some(NbrTrue::Count {
            op: RelOp::parse(&op.text())?,
            n: n.parse().ok()?,
        }),
        _ => None,
    }
}

/// The one word of a keyword's arguments.
fn word(args: &[Tok]) -> Option<String> {
    match args {
        [one] => Some(one.text()),
        _ => None,
    }
}

/// The text of a `TEXT('...')`-style argument, without quotes.
fn text_of(args: &[Tok]) -> String {
    match args {
        [Tok::Str(s)] => s.clone(),
        [Tok::Word(w)] if w == "*NONE" || w == "*BLANK" => String::new(),
        other => Tok::join(other),
    }
}

/// `(*YES)`.
fn yes(args: &[Tok]) -> bool {
    matches!(args, [Tok::Word(w)] if w == "*YES")
}

#[cfg(test)]
mod tests {
    use super::*;

    const CRTLIB: &str = "\
             CMD        PROMPT('Create Library') ALLOW(*ALL) MAXPOS(2) +
                          HLPPNLGRP(QHLCRTLIB) HLPID(CRTLIB)
             PARM       KWD(LIB) TYPE(*SNAME) LEN(10) MIN(1) +
                          PROMPT('Library')
             PARM       KWD(TYPE) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*PROD) VALUES(*PROD *TEST) PROMPT('Library type')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*BLANK) +
                          SPCVAL((*BLANK ' ')) PROMPT('Text ''description''')
             PARM       KWD(AUT) TYPE(*NAME) LEN(10) DFT(*LIBCRTAUT) +
                          SPCVAL((*LIBCRTAUT) (*CHANGE) (*ALL) (*USE) +
                          (*EXCLUDE)) PMTCTL(*PMTRQS) PROMPT('Authority')
             PARM       KWD(CRTOBJAUD) TYPE(*CHAR) LEN(7) DFT(*SYSVAL) +
                          VALUES(*SYSVAL *NONE *USRPRF *CHANGE *ALL) +
                          PMTCTL(*PMTRQS) PROMPT('Create object auditing')
";

    #[test]
    fn crtlib_compiles() {
        let d = compile("crtlib", CRTLIB).unwrap();
        assert_eq!(d.name, "CRTLIB");
        assert_eq!(d.prompt, "Create Library");
        assert_eq!(d.maxpos, Some(2));
        assert_eq!(d.parms.len(), 5);
        assert_eq!(d.parms[0].ty, ParmType::SName);
        assert_eq!(d.parms[0].min, 1);
        assert_eq!(d.parms[1].values, vec!["*PROD", "*TEST"]);
        assert_eq!(d.parms[2].prompt.as_ref().unwrap().0, "Text 'description'");
        assert_eq!(d.parms[2].spcval[0].to.as_deref(), Some("' '"));
        assert_eq!(d.parms[3].pmtctl, Some(PmtCtlRef::PmtRqs));
        assert_eq!(d.hlppnlgrp.as_ref().unwrap().name, "QHLCRTLIB");
    }

    const GROUPS: &str = "\
 CMD PROMPT('Submit Job')
 PARM KWD(CMD) TYPE(*CMDSTR) LEN(3000) PROMPT('Command to run')
 PARM KWD(JOBD) TYPE(Q1) DFT(*USRPRF) SNGVAL((*USRPRF)) PROMPT('Job description')
 Q1: QUAL TYPE(*NAME) LEN(10)
 QUAL TYPE(*NAME) LEN(10) DFT(*LIBL) SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
 PARM KWD(LOG) TYPE(E1) DFT(*JOBD) SNGVAL((*JOBD)) PROMPT('Message logging')
 E1: ELEM TYPE(*INT2) DFT(4) RANGE(0 4) PROMPT('Level')
 ELEM TYPE(*INT2) DFT(0) RANGE(0 99) PROMPT('Severity')
 ELEM TYPE(*CHAR) LEN(7) RSTD(*YES) DFT(*NOLIST) VALUES(*MSG *SECLVL *NOLIST) PROMPT('Text')
 PARM KWD(INLLIBL) TYPE(*NAME) LEN(10) MAX(250) DFT(*JOBD) SNGVAL((*JOBD) (*SYSVAL) (*NONE)) PMTCTL(P1)
 P1: PMTCTL CTL(LOG) COND((*SPCFD)) NBRTRUE(*ALL)
 DEP CTL(&LOG) PARM(&INLLIBL) NBRTRUE(*ALL) MSGID(CPD1234)
";

    #[test]
    fn groups_resolve() {
        let d = compile("SBMJOB", GROUPS).unwrap();
        let jobd = d.parm("JOBD").unwrap();
        assert!(matches!(jobd.ty, ParmType::Qual { .. }));
        assert_eq!(jobd.items.len(), 2);
        assert_eq!(jobd.items[1].dft.as_deref(), Some("*LIBL"));
        let log = d.parm("LOG").unwrap();
        assert!(matches!(log.ty, ParmType::Elem { .. }));
        assert_eq!(log.items.len(), 3);
        assert_eq!(log.items[2].values.len(), 3);
        assert_eq!(d.pmtctls.len(), 1);
        assert_eq!(d.deps.len(), 1);
        assert_eq!(d.deps[0].msgid.as_deref(), Some("CPD1234"));
    }

    #[test]
    fn errors_are_collected() {
        let src = "\
 CMD PROMPT('X')
 PARM KWD(A) TYPE(*CHAR) LEN(10)
 PARM KWD(A) TYPE(*NOPE)
 PARM KWD(B) TYPE(E9)
 PARM KWD(C) TYPE(*DEC) RTNVAL(*YES) DFT(1)
 PARM KWD(D) TYPE(*CHAR) RSTD(*YES)
 FOO BAR(1)
";
        let errs = compile("X", src).unwrap_err();
        let ids: Vec<&str> = errs.iter().map(|d| d.msgid.as_str()).collect();
        assert!(ids.contains(&"CPD0244"), "{ids:?}");
        assert!(ids.contains(&"CPD0243"), "{ids:?}");
        assert!(ids.contains(&"CPD0217"), "{ids:?}");
        let errs = compile("X", " CMD PROMPT('X')\n PARM KWD(A) TYPE(*CHAR)\n PARM KWD(A) TYPE(*DEC) RTNVAL(*YES) DFT(1)\n PARM KWD(D) TYPE(*CHAR) RSTD(*YES)\n").unwrap_err();
        let ids: Vec<&str> = errs.iter().map(|d| d.msgid.as_str()).collect();
        assert!(ids.contains(&"CPD0214"), "{ids:?}");
        assert!(ids.contains(&"CPD0225"), "{ids:?}");
        assert!(ids.contains(&"CPD0230"), "{ids:?}");
    }

    #[test]
    fn cmd_must_come_first() {
        let errs = compile("X", " PARM KWD(A) TYPE(*CHAR)\n CMD PROMPT('x')\n").unwrap_err();
        assert!(errs.iter().any(|d| d.msgid == "CPD0250"));
    }
}

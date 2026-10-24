// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Calculations into IR: assignment, the structured operations, the
//! fixed-form compare-and-branch families, subroutines (a return-point
//! slot per subroutine and one `switch` at `ENDSR`), `GOTO`/`TAG`,
//! indicators, printer output, `DSPLY`, `RETURN`, calls, and the
//! subprocedures themselves.

use rpgfront::ast::{AssignOp, BinOp, CondInd, Conditioning, Expr, Fig, Lit, Proc, Stmt};

use crate::decl;
use crate::diag::ids;
use crate::files::Dev;
use crate::codegen::emit::{Body, quote};
use crate::codegen::expr::{V, Vk, Want};
use crate::codegen::{Gen, LoopEnd, Open, Subr, place_of};
use crate::names::mangle;
use crate::syms::{Kind, Storage, SymId};
use crate::types::Ty;

/// The decimal positions of an assignment target, for precision.
fn target_mode(g: &Gen<'_>, t: &V, half: bool) -> qdecimal::Mode {
    qdecimal::Mode {
        result_scale: t.ty.shape().map_or(0, |(_, s)| s),
        half_adjust: half,
        resdecpos: g.ctl.resdecpos,
    }
}

impl Gen<'_> {
    /// Assign `src` to the place `dst`.
    pub fn assign(&mut self, dst: &V, src: &V, half: bool) {
        if dst.kind != Vk::Place {
            self.err(ids::NOT_ASSIGNABLE, 30, "");
            return;
        }
        // A whole array assigned to one field.
        if src.dim > 0 && dst.dim == 0 && src.kind == Vk::Place && !matches!(dst.ty, Ty::Ds(_)) {
            self.err(ids::ARRAY_NOT_INDEXED, 30, "");
            return;
        }
        // A field whose definition was in error is not defined.
        if dst.ty == Ty::Unknown || src.ty == Ty::Unknown && src.kind == Vk::Place {
            self.err(ids::UNDEFINED_OPERAND, 30, "");
            return;
        }
        let dst = self.as_chars(dst);
        let src = if matches!(dst.ty, Ty::Ds(_)) { src.clone() } else { self.as_chars(src) };
        // Text of another class converts to the target's.
        let src = match (super::ucs2::class(&dst.ty), super::ucs2::class(&src.ty)) {
            (Some(to), Some(from)) if to != from && src.fig.is_none() => {
                let Some(v) = self.to_class(&src, to) else { return };
                v
            }
            _ => src,
        };
        let h = if half { " halfadj" } else { "" };
        match (&dst.ty, &src.ty) {
            (d, _) if d.is_numeric() => {
                if let Some(f) = &src.fig {
                    match f {
                        Fig::Zeros | Fig::Hival | Fig::Loval => self.b.ins(format!("mov {}, {}", dst.op, src.op)),
                        Fig::All(s) if s.chars().all(|c| c.is_ascii_digit()) => {
                            self.b.ins(format!("mov {}, *ZEROS", dst.op));
                        }
                        _ => self.err(ids::ASSIGN_TYPE, 30, ""),
                    }
                    return;
                }
                if !src.ty.is_numeric() {
                    self.err(ids::ASSIGN_TYPE, 30, "");
                    return;
                }
                let s = self.dec_operand(&src);
                self.b.ins(format!("dec.mov {}, {s}{h}", dst.op));
            }
            (d, _) if d.is_string() => {
                if let Some(Fig::All(pat)) = &src.fig {
                    let n = usize::try_from(dst.ty.str_len().unwrap_or(0)).unwrap_or(0);
                    let lit: String = pat.chars().cycle().take(n).collect();
                    self.b.ins(format!("mov {}, {}", dst.op, quote(&lit)));
                    return;
                }
                if let Some(Fig::AllX(h)) = &src.fig {
                    let n = usize::try_from(dst.ty.str_len().unwrap_or(0)).unwrap_or(0);
                    let bytes: String = h.chars().collect::<Vec<_>>().chunks(2).map(|c| c.iter().collect::<String>()).cycle().take(n).collect();
                    self.b.ins(format!("mov {}, x\"{bytes}\"", dst.op));
                    return;
                }
                if src.fig.is_some() {
                    self.b.ins(format!("mov {}, {}", dst.op, src.op));
                    return;
                }
                if !src.ty.is_string() {
                    self.err(ids::ASSIGN_TYPE, 30, "");
                    return;
                }
                if let (Vk::Lit, Some(n), Some(m)) = (src.kind, dst.ty.str_len(), src.ty.str_len())
                    && m > n
                {
                    // The literal cut to the field (or its maximum).
                    let text: String = src.op.trim_matches('"').replace("\"\"", "\"");
                    let cut: String = text.chars().take(usize::try_from(n).unwrap_or(0)).collect();
                    self.b.ins(format!("mov {}, {}", dst.op, quote(&cut)));
                    return;
                }
                self.b.ins(format!("mov {}, {}", dst.op, src.op));
            }
            (Ty::Ind, _) => {
                if src.ty == Ty::Ind || matches!(src.fig, Some(Fig::On | Fig::Off)) {
                    match src.kind {
                        Vk::Place => self.b.ins(format!("mov {}, {}", dst.op, src.op)),
                        _ => self.b.ins(format!("store {}, {}", dst.op, src.op)),
                    }
                } else if src.ty.is_char() && src.kind == Vk::Lit {
                    let v = if src.op.contains('1') { "*ON" } else { "*OFF" };
                    self.b.ins(format!("store {}, {v}", dst.op));
                } else if matches!(src.fig, Some(Fig::Blanks | Fig::Zeros)) {
                    self.b.ins(format!("store {}, *OFF", dst.op));
                } else {
                    self.err(ids::ASSIGN_TYPE, 30, "");
                }
            }
            (Ty::Ds(_), _) => {
                let whole = format!("{}.all_", dst.op);
                if src.ty == dst.ty && src.kind == Vk::Place {
                    self.b.ins(format!("mov {}, {}", dst.op, src.op));
                } else {
                    let s = self.as_chars(&src);
                    if s.ty.is_string() || s.fig.is_some() {
                        let s = if let Some(Fig::All(pat)) = &s.fig {
                            let n = usize::try_from(self.t.size(&dst.ty)).unwrap_or(0);
                            quote(&pat.chars().cycle().take(n).collect::<String>())
                        } else {
                            s.op
                        };
                        self.b.ins(format!("mov {whole}, {s}"));
                    } else {
                        self.err(ids::ASSIGN_TYPE, 30, "");
                    }
                }
            }
            (Ty::Date(..) | Ty::Time(..) | Ty::Timestamp(_), _) => self.dt_assign(&dst, &src),
            (Ty::Ptr | Ty::ProcPtr, _) => {
                if src.fig == Some(Fig::Null) {
                    let r = self.b.reg();
                    let k = if dst.ty == Ty::ProcPtr { "procptr" } else { "ptr" };
                    self.b.ins(format!("{r} = const.{k} *NULL"));
                    self.b.ins(format!("store {}, {r}", dst.op));
                } else if matches!(src.ty, Ty::Ptr | Ty::ProcPtr) {
                    let r = self.ptr_reg(&src);
                    self.b.ins(format!("store {}, {r}", dst.op));
                } else {
                    self.err(ids::ASSIGN_TYPE, 30, "");
                }
            }
            _ => self.err(ids::ASSIGN_TYPE, 30, ""),
        }
    }

    /// An `ind` register for a conditioning set (OR of ANDs).
    pub fn conditioning(&mut self, c: &Conditioning) -> Option<String> {
        let t = self.b.temp("ind");
        self.b.ins(format!("store {t}, *OFF"));
        let done = self.b.label();
        for group in c {
            let next = self.b.label();
            for ind in group {
                let r = self.cond_ind(ind)?;
                let ok = self.b.label();
                self.b.term(format!("brif {r}, {ok}, {next}"));
                self.b.place(&ok);
            }
            self.b.ins(format!("store {t}, *ON"));
            self.b.term(format!("br {done}"));
            self.b.place(&next);
        }
        self.b.place(&done);
        let r = self.b.reg();
        self.b.ins(format!("{r} = load {t}"));
        Some(r)
    }

    /// One conditioning indicator, negated when `N`.
    fn cond_ind(&mut self, c: &CondInd) -> Option<String> {
        let v = self.indicator(&c.ind)?;
        let r = self.ind_reg(&v)?;
        if c.not {
            let n = self.b.reg();
            self.b.ins(format!("{n} = cmp.eq {r}, *OFF"));
            return Some(n);
        }
        Some(r)
    }

    /// An indicator by its fixed-form name (`01`, `LR`, `KA`, `H1`).
    pub fn indicator(&mut self, name: &str) -> Option<V> {
        let word = format!("*IN{name}");
        let e = Expr::Special {
            word,
            index: None,
            at: self.at,
        };
        self.expr(&e, Want::default())
    }

    /// Whether a print just reached the overflow line: the printer says
    /// so for every line at or past it, the indicator goes on only for
    /// the first.
    pub fn overflow_crossed(&mut self, file: &str, st: &str) -> String {
        let prev = format!("$ovp_{}", mangle(file));
        self.externs.entry(prev.clone()).or_insert_with(|| format!("static {prev}: ind inz(*OFF)"));
        let ov = self.b.reg();
        self.b.ins(format!("{ov} = cmp.eq {st}, 1"));
        let was = self.b.reg();
        self.b.ins(format!("{was} = load {prev}"));
        let off = self.b.reg();
        self.b.ins(format!("{off} = const.ind *OFF"));
        let crossed = self.b.reg();
        self.b.ins(format!("{crossed} = select {was}, {off}, {ov}"));
        self.b.ins(format!("store {prev}, {ov}"));
        crossed
    }

    /// Set a file's overflow indicator on: `*INxx`, or a named
    /// indicator field (`OFLIND(name)`).
    pub fn oflind_on(&mut self, ind: &str) {
        let x = match ind.strip_prefix("*IN") {
            Some(n) => self.indicator(n),
            None => {
                let e = Expr::Ref { name: ind.to_ascii_uppercase(), steps: Vec::new(), at: self.at };
                self.expr(&e, Want::default())
            }
        };
        if let Some(x) = x {
            self.b.ins(format!("store {}, *ON", x.op));
        }
    }

    /// Set a resulting indicator on or off from a condition register.
    pub fn set_ind(&mut self, name: &str, r: &str) {
        if name.is_empty() {
            return;
        }
        if let Some(v) = self.indicator(name) {
            self.b.ins(format!("store {}, {r}", v.op));
        }
    }

    /// A condition expression as an `ind` register.
    pub fn cond(&mut self, e: &Expr) -> Option<String> {
        let v = self.expr(e, Want::default())?;
        if v.ty != Ty::Ind {
            self.err(ids::NOT_LOGICAL, 30, "");
            return None;
        }
        self.ind_reg(&v)
    }

    /// `factor1 op factor2` of a fixed-form `IFxx`, `DOWxx`, `CABxx`...
    pub fn cmp_suffix(&mut self, sfx: &str, f1: &Expr, f2: &Expr) -> Option<String> {
        let a = self.expr(f1, Want::default())?;
        let b = self.expr(f2, Want::default())?;
        let op = match sfx {
            "EQ" => BinOp::Eq,
            "NE" => BinOp::Ne,
            "GT" => BinOp::Gt,
            "LT" => BinOp::Lt,
            "GE" => BinOp::Ge,
            _ => BinOp::Le,
        };
        let (a, b) = self.altseq_pair(a, b);
        let v = self.compare(op, &a, &b)?;
        Some(v.op)
    }

    /// `ALTSEQ(*SRC)`: two character operands compared through the
    /// alternate collating sequence, unless one is `ALTSEQ(*NONE)`.
    fn altseq_pair(&mut self, a: V, b: V) -> (V, V) {
        let Some(table) = self.altseq_table() else { return (a, b) };
        let exempt = |g: &Self, v: &V| v.sym.is_some_and(|s| g.t.syms[s].altseq_none);
        if !a.ty.is_char() || !b.ty.is_char() || exempt(self, &a) || exempt(self, &b) {
            return (a, b);
        }
        let mut out = Vec::new();
        for v in [a, b] {
            let v = self.as_chars(&v);
            let r = self.rt_of(&v, None);
            let n = v.ty.str_len().unwrap_or(1).max(1);
            let t = self.tmp(&Ty::Char(n));
            self.rt_proc("_QRNX_ALTSEQ", "($dst: bytes(1) *VARSIZE, $src: bytes(1) *VARSIZE, byval $sd: i64, const $tab: char(256))");
            self.b.ins(format!("callb _QRNX_ALTSEQ({}, {}, {}, {table})", t.op, r.op, r.desc));
            out.push(t);
        }
        let b = out.pop().expect("two");
        let a = out.pop().expect("two");
        (a, b)
    }

    /// The alternate collating sequence of `ALTSEQ(*SRC)` and the
    /// `**ALTSEQ` records, as a static table; none otherwise.
    fn altseq_table(&mut self) -> Option<String> {
        if !self.ctl.altseq_src {
            return None;
        }
        let mut tab: Vec<u8> = (0..=255u8).collect();
        for c in self.ctdata.iter().filter(|c| c.kind == "ALTSEQ") {
            for r in &c.records {
                // Columns 11-80: pairs of from and to, two hex digits each.
                let body: String = r.chars().skip(10).take(70).collect();
                let hex: Vec<u8> = body.as_bytes().chunks(2).filter_map(|p| std::str::from_utf8(p).ok().and_then(|h| u8::from_str_radix(h, 16).ok())).collect();
                for pair in hex.chunks(2) {
                    if let [from, to] = pair {
                        tab[usize::from(*from)] = *to;
                    }
                }
            }
        }
        let lit = tab.iter().fold(String::new(), |mut acc, b| {
            let _ = std::fmt::Write::write_fmt(&mut acc, format_args!("{b:02X}"));
            acc
        });
        let name = "$altseq_tab".to_string();
        self.externs.entry(name.clone()).or_insert_with(|| format!("static {name}: char(256) inz(x\"{lit}\")"));
        Some(name)
    }

    /// A fixed-form condition with its `ANDxx`/`ORxx` lines from
    /// `stmts[k]`: the register and how many statements it took.
    fn cmp_chain(&mut self, stmts: &[&Stmt], k: usize, sfx: &str) -> Option<(String, usize)> {
        let s = stmts[k];
        let f1 = s.op.f1.first()?.clone();
        let f2 = s.op.f2.first()?.clone();
        let mut acc = self.cmp_suffix(sfx, &f1, &f2)?;
        let mut used = 1;
        // Precedence: AND binds tighter than OR; fold left to right in
        // groups.
        let mut or_parts: Vec<String> = Vec::new();
        while let Some(n) = stmts.get(k + used) {
            let (base, sfx2) = rpgfront::parse::ops::split_suffix(&n.op.code);
            if base != "AND" && base != "OR" || sfx2.is_empty() {
                break;
            }
            let a = n.op.f1.first()?.clone();
            let b = n.op.f2.first()?.clone();
            let r = self.cmp_suffix(sfx2, &a, &b)?;
            if base == "AND" {
                let t = self.b.reg();
                self.b.ins(format!("{t} = select {acc}, {r}, {acc}"));
                acc = t;
            } else {
                or_parts.push(acc);
                acc = r;
            }
            used += 1;
        }
        for p in or_parts {
            let t = self.b.reg();
            self.b.ins(format!("{t} = select {p}, {p}, {acc}"));
            acc = t;
        }
        Some((acc, used))
    }

    /// The innermost loop.
    fn innermost_loop(&self) -> Option<(String, String)> {
        self.open.iter().rev().find_map(|o| match o {
            Open::Loop { iter, leave, .. } => Some((iter.clone(), leave.clone())),
            _ => None,
        })
    }

    /// A subroutine's record, made on first mention.
    fn subr(&mut self, name: &str) -> &mut Subr {
        if !self.subrs.contains_key(name) {
            let label = self.b.label();
            let end = self.b.label();
            let slot = self.b.temp("i32");
            self.subrs.insert(
                name.to_string(),
                Subr {
                    label,
                    slot,
                    end,
                    ..Subr::default()
                },
            );
        }
        self.subrs.get_mut(name).expect("just made")
    }

    /// `EXSR name` of a main-procedure subroutine: a call; a `RETURN`
    /// inside it ends the main procedure.
    pub fn exsr_call(&mut self, name: &str) {
        if self.bad_subrs.contains(&name.to_ascii_uppercase()) {
            self.err(ids::BAD_FACTOR2, 30, name.to_string());
            return;
        }
        let rc = self.b.reg();
        self.b.ins(format!("{rc} = callb {}()", Gen::subr_proc(name)));
        let cont = self.b.label();
        let back = self.b.label();
        let one = self.b.reg();
        self.b.ins(format!("{one} = cmp.eq {rc}, {}", crate::codegen::exc::rp::RETURN));
        self.b.term(format!("brif {one}, {back}, {cont}"));
        self.b.place(&back);
        if self.in_subr_proc {
            self.b.term(format!("ret {rc}"));
        } else {
            let exit = self.main_exit.clone().unwrap_or_default();
            self.b.term(format!("br {exit}"));
        }
        self.b.place(&cont);
        if !self.defined_subrs.contains(name) {
            self.called_subrs.insert(name.to_string());
        }
    }

    /// `EXSR name`.
    pub fn exsr(&mut self, name: &str) {
        if self.bad_subrs.contains(&name.to_ascii_uppercase()) {
            self.err(ids::BAD_FACTOR2, 30, name.to_string());
            return;
        }
        let back = self.b.label();
        let s = self.subr(name);
        s.returns.push(back.clone());
        let n = s.returns.len();
        let (slot, label) = (s.slot.clone(), s.label.clone());
        self.b.ins(format!("store {slot}, {n}"));
        self.b.term(format!("br {label}"));
        self.b.place(&back);
    }

    /// One statement.
    #[allow(clippy::too_many_lines)]
    fn statement(&mut self, stmts: &[&Stmt], k: usize) -> usize {
        let before = self.diags.len();
        // A factor 1 the operation ignores still names something.
        let s = stmts[k];
        if s.fixed && rpgfront::parse::ops::no_factor1(&s.op.code) {
            self.at = s.at;
            for e in &s.op.f1 {
                if let Some(n) = e.name()
                    && self.t.find(self.scope, n).is_none()
                {
                    self.undefined_name(n, e.pos());
                }
            }
        }
        let n = self.statement_conditioned(stmts, k);
        // Factors are names, not expressions: an undefined one is only
        // RNF7030.
        if stmts[k].fixed {
            let tail = self.diags.split_off(before);
            self.diags.extend(tail.into_iter().filter(|d| d.msgid != ids::UNDEFINED_OPERAND));
        }
        n
    }

    /// One statement under its conditioning indicators.
    fn statement_conditioned(&mut self, stmts: &[&Stmt], k: usize) -> usize {
        let s = stmts[k];
        self.mark(s.at);
        // Conditioning indicators skip the operation when off.
        let skip = if s.cond.is_empty() {
            None
        } else {
            let r = self.conditioning(&s.cond);
            let run = self.b.label();
            let skip = self.b.label();
            if let Some(r) = r {
                self.b.term(format!("brif {r}, {run}, {skip}"));
            }
            self.b.place(&run);
            Some(skip)
        };
        let op = &s.op;
        // The (E) extender: the operation in a monitor region of its own.
        if op.ext.contains('E') && !self.in_error_extender {
            self.in_error_extender = true;
            let mut used = 1;
            self.with_error_extender(|g| used = g.statement_body(stmts, k));
            self.in_error_extender = false;
            if let Some(skip) = skip {
                self.b.place(&skip);
            }
            return k + used;
        }
        let used = self.statement_body(stmts, k);
        if let Some(skip) = skip {
            self.b.place(&skip);
        }
        k + used
    }

    /// One statement's operation, after its conditioning; how many
    /// statements it took.
    #[allow(clippy::too_many_lines)]
    fn statement_body(&mut self, stmts: &[&Stmt], k: usize) -> usize {
        let s = stmts[k];
        let op = &s.op;
        if op.code == "EXEC SQL" {
            self.exec_sql(s);
            return 1;
        }
        let (base, sfx) = rpgfront::parse::ops::split_suffix(&op.code);
        let half = op.ext.contains('H');
        let mut used = 1;
        match (base, sfx) {
            ("EVAL", _) => {
                if let Some(Expr::Assign(aop, t, v, _)) = &op.xf2 {
                    self.eval(*aop, t, v, half);
                }
            }
            ("IF", "") => {
                let r = op.xf2.as_ref().and_then(|e| self.cond(e));
                self.open_if(r);
            }
            ("IF", sfx) => {
                let r = self.cmp_chain(stmts, k, sfx).map(|(r, n)| {
                    used = n;
                    r
                });
                self.open_if(r);
            }
            ("ELSEIF", _) => {
                let r_label = self.else_part();
                if let Some(l) = r_label {
                    let r = op.xf2.as_ref().and_then(|e| self.cond(e));
                    let then = self.b.label();
                    let next = self.b.label();
                    if let Some(r) = r {
                        self.b.term(format!("brif {r}, {then}, {next}"));
                    }
                    self.b.place(&then);
                    if let Some(Open::If { next: n, .. }) = self.open.last_mut() {
                        *n = next;
                    }
                    let _ = l;
                }
            }
            ("ELSE", _) => {
                self.else_part();
                if let Some(Open::If { next, .. }) = self.open.last_mut() {
                    *next = String::new();
                }
            }
            ("ENDIF", _) => self.end_if(),
            ("DOW", "") => {
                let top = self.b.label();
                let body = self.b.label();
                let end = self.b.label();
                self.b.place(&top);
                if let Some(r) = op.xf2.as_ref().and_then(|e| self.cond(e)) {
                    self.b.term(format!("brif {r}, {body}, {end}"));
                }
                self.b.place(&body);
                self.open.push(Open::Loop {
                    iter: top.clone(),
                    leave: end,
                    end: LoopEnd::Top(top),
                });
            }
            ("DOW", sfx) => {
                let top = self.b.label();
                let body = self.b.label();
                let end = self.b.label();
                self.b.place(&top);
                if let Some((r, n)) = self.cmp_chain(stmts, k, sfx) {
                    used = n;
                    self.b.term(format!("brif {r}, {body}, {end}"));
                }
                self.b.place(&body);
                self.open.push(Open::Loop {
                    iter: top.clone(),
                    leave: end,
                    end: LoopEnd::Top(top),
                });
            }
            ("DOU", "") => {
                let body = self.b.label();
                let test = self.b.label();
                let end = self.b.label();
                self.b.place(&body);
                let e = op.xf2.clone().unwrap_or(Expr::Empty(s.at));
                self.open.push(Open::Loop {
                    iter: test.clone(),
                    leave: end,
                    end: LoopEnd::Bottom(Box::new(e), body),
                });
            }
            ("DOU", sfx) => {
                let body = self.b.label();
                let test = self.b.label();
                let end = self.b.label();
                self.b.place(&body);
                let f1 = op.f1.first().cloned().unwrap_or(Expr::Empty(s.at));
                let f2 = op.f2.first().cloned().unwrap_or(Expr::Empty(s.at));
                self.open.push(Open::Loop {
                    iter: test.clone(),
                    leave: end,
                    end: LoopEnd::BottomCmp(sfx.to_string(), Box::new(f1), Box::new(f2), body),
                });
            }
            ("FOR", _) => self.for_loop(s),
            ("DO", _) => self.counted_do(s),
            ("ENDDO" | "ENDFOR", _) => self.end_loop(None),
            ("END", _) => {
                if matches!(self.open.last(), Some(Open::If { .. })) {
                    self.end_if();
                } else if matches!(self.open.last(), Some(Open::Select { .. })) {
                    self.end_select();
                } else {
                    let inc = op.f2.first().cloned();
                    self.end_loop(inc.as_ref());
                }
            }
            ("LEAVE", _) => match self.innermost_loop() {
                Some((_, l)) => self.b.term(format!("br {l}")),
                None => self.err(ids::UNBALANCED, 30, "LEAVE"),
            },
            ("ITER", _) => match self.innermost_loop() {
                Some((i, _)) => self.b.term(format!("br {i}")),
                None => self.err(ids::UNBALANCED, 30, "ITER"),
            },
            ("SELECT", _) => {
                let end = self.b.label();
                let subject = op.xf2.clone().map(Box::new);
                self.open.push(Open::Select { next: None, end, cas: false, subject });
            }
            ("WHEN" | "WHEN-IS" | "WHEN-IN", sfx) => {
                let Some(Open::Select { next, end, subject, .. }) = self.open.last().cloned() else {
                    self.err(ids::UNBALANCED, 30, "WHEN");
                    return k + 1;
                };
                if let Some(n) = next {
                    self.b.term(format!("br {end}"));
                    self.b.place(&n);
                }
                let r = if base != "WHEN" {
                    // The operand compared, or looked for in the list.
                    let at = s.at;
                    match (subject, op.xf2.clone()) {
                        (Some(subj), Some(e)) => {
                            let e = match e {
                                Expr::List(mut v, _) if v.len() == 1 => v.remove(0),
                                e => e,
                            };
                            let bop = if base == "WHEN-IS" { rpgfront::ast::BinOp::Eq } else { rpgfront::ast::BinOp::In };
                            let before = self.diags.len();
                            let r = self.cond(&Expr::Binary(bop, subj, Box::new(e), at));
                            // Operands of types that do not compare.
                            for d in &mut self.diags[before..] {
                                if d.msgid == ids::TYPE_MISMATCH {
                                    d.msgid = ids::WHEN_IS_TYPE;
                                }
                            }
                            r
                        }
                        _ => {
                            self.err(ids::UNBALANCED, 30, base.to_string());
                            None
                        }
                    }
                } else if sfx.is_empty() {
                    op.xf2.as_ref().and_then(|e| self.cond(e))
                } else {
                    self.cmp_chain(stmts, k, sfx).map(|(r, n)| {
                        used = n;
                        r
                    })
                };
                let body = self.b.label();
                let miss = self.b.label();
                if let Some(r) = r {
                    self.b.term(format!("brif {r}, {body}, {miss}"));
                }
                self.b.place(&body);
                if let Some(Open::Select { next, .. }) = self.open.last_mut() {
                    *next = Some(miss);
                }
            }
            ("OTHER", _) => {
                let Some(Open::Select { next, end, .. }) = self.open.last().cloned() else {
                    self.err(ids::UNBALANCED, 30, "OTHER");
                    return k + 1;
                };
                self.b.term(format!("br {end}"));
                if let Some(n) = next {
                    self.b.place(&n);
                }
                if let Some(Open::Select { next, .. }) = self.open.last_mut() {
                    *next = None;
                }
            }
            ("ENDSL" | "ENDCS", _) => self.end_select(),
            ("CAS", sfx) => self.cas_op(op, sfx),
            ("EXSR", _) => {
                if let Some(n) = op.f2.first().and_then(|e| e.name().map(str::to_string).or_else(|| match e {
                    Expr::Special { word, .. } => Some(word.clone()),
                    _ => None,
                })) {
                    if self.subr_procs {
                        self.exsr_call(&n);
                    } else {
                        self.exsr(&n);
                    }
                }
            }
            ("BEGSR", _) => {
                let name = op.f1.first().and_then(|e| match e {
                    Expr::Ref { name, .. } => Some(name.clone()),
                    Expr::Special { word, .. } => Some(word.clone()),
                    _ => None,
                });
                if let Some(n) = name {
                    // The calculations before the first subroutine end here.
                    if self.b.is_open() {
                        let exit = self.calcs_end.clone().or_else(|| self.main_exit.clone()).unwrap_or_default();
                        if exit.is_empty() {
                            self.b.term("ret");
                        } else {
                            self.b.term(format!("br {exit}"));
                        }
                    }
                    let label = self.subr(&n).label.clone();
                    self.subr(&n).defined = true;
                    self.b.place(&label);
                    self.open.push(Open::Subr(n));
                }
            }
            ("ENDSR", _) => {
                if let Some(Open::Subr(n)) = self.open.last().cloned() {
                    self.open.pop();
                    let end = self.subr(&n).end.clone();
                    self.b.place(&end);
                    let idx = self.b.lines.len();
                    self.b.term("unreachable");
                    self.subr(&n).switch_line = Some(idx);
                } else {
                    self.err(ids::UNBALANCED, 30, "ENDSR");
                }
            }
            ("LEAVESR", _) if self.in_subr_proc => self.b.term(format!("ret {}", crate::codegen::exc::rp::NONE)),
            ("LEAVESR", _) => {
                let n = self.open.iter().rev().find_map(|o| match o {
                    Open::Subr(n) => Some(n.clone()),
                    _ => None,
                });
                match n {
                    Some(n) => {
                        let end = self.subr(&n).end.clone();
                        self.b.term(format!("br {end}"));
                    }
                    None => self.err(ids::NOT_IN_SUBR, 30, "LEAVESR"),
                }
            }
            ("TAG", _) => {
                if let Some(n) = op.f1.first().and_then(|e| e.name().map(str::to_string)) {
                    // A label defined twice: the second is ignored.
                    if !self.placed_tags.insert(n.to_ascii_uppercase()) {
                        self.err(ids::TAG_TWICE, 30, n);
                        return k + 1;
                    }
                    let l = self.tag_label(&n);
                    self.b.place(&l);
                }
            }
            ("GOTO", _) => {
                if let Some(n) = op.f2.first().and_then(|e| e.name().map(str::to_string)) {
                    if !self.is_label(&n, op.f2[0].pos()) {
                        return k + 1;
                    }
                    let l = self.tag_label(&n);
                    self.b.term(format!("br {l}"));
                }
            }
            ("CAB", sfx) => {
                let target = op.result.first().and_then(|e| e.name().map(|n| (n.to_string(), e.pos())));
                if let Some((n, at)) = &target
                    && !self.is_label(n, *at)
                {
                    return k + 1;
                }
                if let (Some(a), Some(b)) = (op.f1.first().cloned(), op.f2.first().cloned())
                    && let Some(r) = self.cmp_suffix(sfx, &a, &b)
                {
                    self.cmp_resind(op, &a, &b);
                    if let Some(n) = op.result.first().and_then(|e| e.name().map(str::to_string)) {
                        let l = self.tag_label(&n);
                        let cont = self.b.label();
                        self.b.term(format!("brif {r}, {l}, {cont}"));
                        self.b.place(&cont);
                    }
                }
            }
            ("SETON" | "SETOFF", _) => {
                let v = if base == "SETON" { "*ON" } else { "*OFF" };
                for i in op.resind.clone() {
                    if !i.is_empty() && let Some(x) = self.indicator(&i) {
                        self.b.ins(format!("store {}, {v}", x.op));
                    }
                }
            }
            ("RETURN", _) if self.in_subr_proc => self.b.term(format!("ret {}", crate::codegen::exc::rp::RETURN)),
            ("RETURN", _) => self.return_op(op.xf2.as_ref()),
            ("CALLP", _) => {
                if let Some(e) = &op.xf2 {
                    let _ = self.call_statement(e);
                }
            }
            ("DSPLY", _) => self.dsply(op),
            ("WRITE", _) => self.write_op(op),
            ("Z-ADD" | "Z-SUB", _) => {
                if let (Some(f2), Some(r)) = (op.f2.first(), op.result.first()) {
                    let t = self.target(r);
                    let v = self.expr(f2, Want::default());
                    // Factor 2 is a number.
                    if let Some(x) = &v
                        && !x.ty.is_numeric()
                        && x.fig.is_none()
                    {
                        self.err(ids::BAD_FACTOR2, 30, base.to_string());
                        return k + 1;
                    }
                    if let (Some(t), Some(v)) = (t, v) {
                        let v = if base == "Z-SUB" {
                            let z = V::lit(Ty::Packed(1, 0), "0");
                            self.arith(BinOp::Sub, &z, &v, Want::default()).unwrap_or(v)
                        } else {
                            v
                        };
                        self.assign_fixed(&t, &v, half);
                        self.num_resind(op, &t);
                    }
                }
            }
            ("ADD" | "SUB" | "MULT" | "DIV", _) => self.fixed_arith(base, op, half),
            ("EXCEPT", _) => {
                let name = op.f2.first().and_then(|e| e.name().map(str::to_string)).unwrap_or_default();
                let outputs = self.outputs.clone();
                if !name.is_empty() && !outputs.iter().any(|o| o.kind == "E" && o.except == name) {
                    self.err(ids::NOT_DEFINED, 30, name.clone());
                }
                self.output(&outputs, crate::codegen::output::Step::Except(&name));
            }
            ("UNLOCK", _) if self.names_dtaara(op) => self.dtaara_op(op, base),
            (
                "READ" | "READP" | "READE" | "READPE" | "CHAIN" | "SETLL" | "SETGT" | "UPDATE" | "DELETE" | "UNLOCK"
                | "FEOD" | "OPEN" | "CLOSE" | "KLIST" | "KFLD" | "EXFMT" | "READC",
                _,
            ) => self.db_op(op, base),
            ("POST" | "ACQ" | "REL" | "NEXT", _) => self.device_op(op, base),
            ("FORCE", _) => self.force_op(op),
            ("PLIST" | "PARM", _) => {}
            // Declared before generation (`extfiles::define`); the names
            // are referenced here.
            ("DEFINE", _) => {
                let like = matches!(op.f1.first(), Some(Expr::Special { word, .. }) if word == "*LIKE");
                if let Some(b) = op.f2.first().and_then(|e| e.name().map(str::to_string)) {
                    match self.t.find(self.scope, &b).or_else(|| self.t.find(0, &b)) {
                        Some(id) => self.t.refer(id, self.at.line, false),
                        None if like => self.err(ids::NOT_DEFINED, 30, b),
                        None => {}
                    }
                }
                if let Some(n) = op.result.first().and_then(|e| e.name().map(str::to_string))
                    && let Some(id) = self.t.find(self.scope, &n).or_else(|| self.t.find(0, &n))
                {
                    self.t.refer(id, self.at.line, true);
                }
            }
            ("IN" | "OUT", _) => self.dtaara_op(op, base),
            ("ALLOC", _) => self.alloc_op(op),
            ("REALLOC", _) => self.realloc_op(op),
            ("DEALLOC", _) => self.dealloc_op(op),
            ("CALL" | "CALLB", _) => self.call_op(s),
            ("COMMIT" | "ROLBK", _) => {
                self.rt_proc("_QRNX_COMMIT", "(byval $k: i32)");
                self.b.ins(format!("callb _QRNX_COMMIT({})", i32::from(base == "ROLBK") + 1));
            }
            // A dump is taken only with DEBUG(*YES).
            ("DUMP", _) => {
                if self.ctl.debug {
                    self.rt_proc("_QRNX_DUMP", "()");
                    self.b.ins("callb _QRNX_DUMP()");
                }
            }
            // No shutdown is requested: the indicator goes off.
            ("SHTDN", _) => {
                let ind = op.resind[0].clone();
                if ind.is_empty() {
                    // At the high indicator's columns (71-72).
                    self.diags.push(rpgfront::Diag::new(ids::NO_HI_IND, 30, rpgfront::ast::Pos { line: self.at.line, col: 71 }));
                } else if let Some(x) = self.indicator(&ind) {
                    self.b.ins(format!("store {}, *OFF", x.op));
                }
            }
            ("MONITOR", _) => {
                self.monitors += 1;
                let name = format!("M{}", self.monitors);
                let end = self.b.label();
                self.b.ins(format!("monitor {name}"));
                let line = self.b.lines.len() - 1;
                self.open.push(Open::Monitor {
                    name,
                    end,
                    closed: false,
                    line,
                    handlers: Vec::new(),
                });
            }
            ("ON-ERROR" | "ON-EXCP", _) => self.on_error(op, base == "ON-EXCP"),
            ("ENDMON", _) => self.end_monitor(),
            ("SND-MSG", _) => self.snd_msg(op),
            ("XML-INTO", _) => self.xml_into(op),
            ("XML-SAX", _) => self.xml_sax(op),
            ("DATA-INTO" | "DATA-GEN", _) => self.data_op(op, base == "DATA-GEN"),
            ("AND" | "OR", _) => self.err(ids::UNBALANCED, 30, op.code.clone()),
            _ if self.strop(base, s) => {}
            (b, _) if self.dt_op(b, op) => {}
            _ => self.err(ids::UNSUPPORTED, 30, op.code.clone()),
        }
        used
    }

    /// `ON-ERROR` (status codes, `*PROGRAM`, `*FILE`, `*ALL`) or
    /// `ON-EXCP` (message identifiers): close the monitored body at the
    /// first one, then start this handler.
    fn on_error(&mut self, op: &rpgfront::ast::Op, excp: bool) {
        let Some(Open::Monitor { name, end, closed, .. }) = self.open.last().cloned() else {
            self.err(ids::UNBALANCED, 30, op.code.clone());
            return;
        };
        if !closed {
            self.b.ins(format!("endmon {name}"));
        }
        self.b.term(format!("br {end}"));
        let label = self.b.label();
        self.b.place(&label);
        self.psds_error();
        let items: Vec<Expr> = match &op.xf2 {
            Some(Expr::List(v, _)) => v.clone(),
            Some(e) => vec![e.clone()],
            None => Vec::new(),
        };
        let mut sels = Vec::new();
        for it in &items {
            match it {
                Expr::Lit(Lit::Num(n), _) if !excp => {
                    let v: u32 = n.parse().unwrap_or(0);
                    sels.push(format!("{v}"));
                }
                Expr::Special { word, .. } if matches!(word.as_str(), "*PROGRAM" | "*FILE" | "*ALL") => {
                    sels.push(word.clone());
                }
                Expr::Lit(Lit::Char(id), _) if excp => sels.push(id.to_ascii_uppercase()),
                Expr::Empty(_) => {}
                _ => self.err(ids::BAD_KEYWORD_VALUE, 30, op.code.clone()),
            }
        }
        if sels.is_empty() {
            sels.push("*ALL".into());
        }
        if let Some(Open::Monitor { closed, handlers, .. }) = self.open.last_mut() {
            *closed = true;
            for s in sels {
                handlers.push((s, label.clone()));
            }
        }
    }

    /// `ENDMON`: the region's `monitor` line written with every
    /// handler, and the group's end.
    fn end_monitor(&mut self) {
        let Some(Open::Monitor { name, end, closed, line, handlers }) = self.open.pop() else {
            self.err(ids::UNBALANCED, 30, "ENDMON");
            return;
        };
        if !closed {
            self.b.ins(format!("endmon {name}"));
        }
        self.b.place(&end);
        // The status is cleared once the group is done.
        self.b.ins("dec.mov $status, 0");
        let list: Vec<String> = if handlers.is_empty() {
            vec![format!("*ALL -> {end}")]
        } else {
            handlers.iter().map(|(s, l)| format!("{s} -> {l}")).collect()
        };
        self.b.lines[line] = format!("  monitor {name}, {}", list.join(", "));
    }

    fn snd_msg(&mut self, op: &rpgfront::ast::Op) {
        let items: Vec<Expr> = match &op.xf2 {
            Some(Expr::List(v, _)) => v.clone(),
            Some(e) => vec![e.clone()],
            None => Vec::new(),
        };
        let mut kind = "*INFO";
        let mut msg = None;
        let mut target = None;
        for it in items {
            match it {
                Expr::Special { ref word, .. } if word == "*INFO" || word == "*ESCAPE" => {
                    kind = if word == "*ESCAPE" { "*ESCAPE" } else { "*INFO" };
                }
                Expr::Bif { ref name, ref args, .. } if name == "%TARGET" => target = args.first().cloned(),
                e => msg = Some(e),
            }
        }
        let to = match (&target, kind) {
            (Some(Expr::Special { word, .. }), _) if word == "*SELF" => "*SAME",
            (Some(Expr::Special { word, .. }), _) if word == "*EXT" => "*EXT",
            (_, "*ESCAPE") => "*PRV",
            _ => "*SAME",
        };
        let (msgid, msgf, data) = match msg {
            Some(Expr::Bif { name, args, .. }) if name == "%MSG" => {
                let id = args.first().and_then(|a| self.expr(a, Want::default()));
                let f = args.get(1).and_then(|a| self.expr(a, Want::default()));
                let d = args.get(2).and_then(|a| self.expr(a, Want::default()));
                (id, f, d)
            }
            Some(e) => {
                // Text: an immediate message, as CPF9897's data.
                let d = self.expr(&e, Want::default());
                (Some(V::lit(Ty::Char(7), quote("CPF9897"))), Some(V::lit(Ty::Char(10), quote("QCPFMSG"))), d)
            }
            None => return,
        };
        let (Some(id), Some(f)) = (msgid, msgf) else { return };
        // The identifier as char(7), the file as char(10).
        let idt = self.b.temp("char(7)");
        let idv = self.as_chars(&id);
        self.b.ins(format!("mov {idt}, {}", idv.op));
        let ft = self.b.temp("char(10)");
        let fv = self.as_chars(&f);
        self.b.ins(format!("mov {ft}, {}", fv.op));
        let (id, f) = (idt, ft);
        let data = data.map(|d| {
            let d = self.as_chars(&d);
            self.lit_storage(&d)
        });
        let d = data.map(|d| format!(" data({d})")).unwrap_or_default();
        self.b.ins(format!("sndpgmmsg msgid({id}) msgf({f}){d} type({kind}) to({to})"));
    }

    /// `CASxx f1 f2 subr`: the first true comparison of the group runs
    /// its subroutine; a `CAS` without a suffix always does.
    fn cas_op(&mut self, op: &rpgfront::ast::Op, sfx: &str) {
        if !matches!(self.open.last(), Some(Open::Select { cas: true, .. })) {
            let end = self.b.label();
            self.open.push(Open::Select { next: None, end, cas: true, subject: None });
        }
        let Some(Open::Select { next, end, .. }) = self.open.last().cloned() else { return };
        if let Some(n) = next {
            self.b.term(format!("br {end}"));
            self.b.place(&n);
        }
        let body = self.b.label();
        let miss = self.b.label();
        let r = match (op.f1.first().cloned(), op.f2.first().cloned()) {
            (Some(a), Some(b)) if !sfx.is_empty() => {
                let r = self.cmp_suffix(sfx, &a, &b);
                self.cmp_resind(op, &a, &b);
                r
            }
            _ => None,
        };
        match r {
            Some(r) => self.b.term(format!("brif {r}, {body}, {miss}")),
            None => self.b.term(format!("br {body}")),
        }
        self.b.place(&body);
        if let Some(n) = op.result.first().and_then(|e| e.name().map(str::to_string)) {
            if self.subr_procs {
                self.exsr_call(&n);
            } else {
                self.exsr(&n);
            }
        }
        if let Some(Open::Select { next, .. }) = self.open.last_mut() {
            *next = Some(miss);
        }
    }

    /// Whether a `GOTO` or `CABxx` target is a label of the procedure;
    /// one that is not is RNF7043 (and RNF7030 when nothing has the
    /// name).
    fn is_label(&mut self, name: &str, at: rpgfront::ast::Pos) -> bool {
        if self.labels.contains(&name.to_ascii_uppercase()) {
            return true;
        }
        if self.t.find(self.scope, name).is_none() {
            self.undefined_name(name, at);
        }
        self.err(ids::NOT_A_LABEL, 30, name.to_string());
        false
    }

    pub(crate) fn tag_label(&mut self, name: &str) -> String {
        if let Some(l) = self.tags.get(name) {
            return l.clone();
        }
        let l = self.b.label();
        self.tags.insert(name.to_string(), l.clone());
        l
    }

    /// The target of an assignment.
    pub fn target(&mut self, e: &Expr) -> Option<V> {
        match e {
            Expr::Ref { name, steps, .. } => {
                let v = self.reference(name, steps, true)?;
                if let Some(id) = v.sym
                    && self.t.syms[id].readonly
                {
                    self.err(ids::NOT_ASSIGNABLE, 30, name.clone());
                    return None;
                }
                if !matches!(
                    v.sym.and_then(|i| self.t.syms[i].kind.clone()),
                    Some(Kind::Field | Kind::Subfield | Kind::Param | Kind::Ds(_))
                ) {
                    self.err(ids::NOT_ASSIGNABLE, 30, name.clone());
                    return None;
                }
                Some(v)
            }
            Expr::Special { .. } => self.expr(e, Want::default()),
            Expr::Bif { name, args, .. } => self.bif_target(name, args),
            _ => {
                self.err(ids::NOT_ASSIGNABLE, 30, "");
                None
            }
        }
    }

    fn eval(&mut self, aop: AssignOp, t: &Expr, v: &Expr, half: bool) {
        if self.eval_hook(aop, t, v, half) {
            return;
        }
        let Some(tv) = self.target(t) else {
            return;
        };
        let mode = target_mode(self, &tv, half);
        let w = Want { mode, compared: false };
        let value = match aop {
            AssignOp::Set => self.expr(v, w),
            _ => {
                let cur = self.expr(t, w);
                let rhs = self.expr(v, w);
                match (cur, rhs) {
                    (Some(c), Some(r)) => {
                        let op = match aop {
                            AssignOp::Add => BinOp::Add,
                            AssignOp::Sub => BinOp::Sub,
                            AssignOp::Mul => BinOp::Mul,
                            AssignOp::Div => BinOp::Div,
                            _ => BinOp::Pow,
                        };
                        if op == BinOp::Add && (c.ty.is_string() || matches!(c.ty, Ty::Ds(_))) {
                            self.expr(&Expr::Binary(BinOp::Add, Box::new(t.clone()), Box::new(v.clone()), self.at), w)
                        } else {
                            self.arith(op, &c, &r, w)
                        }
                    }
                    _ => None,
                }
            }
        };
        if let Some(value) = value {
            // EVAL(H) of a value with no more decimals than the target
            // rounds nothing.
            if half
                && tv.ty.is_numeric()
                && !matches!(value.ty, Ty::Float(_))
                && let (Some((_, vs)), Some((_, ts))) = (value.ty.shape(), tv.ty.shape())
                && vs <= ts
            {
                self.err(ids::HALF_ADJUST_IDLE, 0, String::new());
            }
            self.assign(&tv, &value, half);
        }
    }

    fn open_if(&mut self, r: Option<String>) {
        let then = self.b.label();
        let next = self.b.label();
        let end = self.b.label();
        if let Some(r) = r {
            self.b.term(format!("brif {r}, {then}, {next}"));
        } else {
            self.b.term(format!("br {then}"));
        }
        self.b.place(&then);
        self.open.push(Open::If { next, end });
    }

    /// Leave the true branch of the open `IF`: the next label placed.
    fn else_part(&mut self) -> Option<String> {
        let Some(Open::If { next, end }) = self.open.last().cloned() else {
            self.err(ids::UNBALANCED, 30, "ELSE");
            return None;
        };
        self.b.term(format!("br {end}"));
        if !next.is_empty() {
            self.b.place(&next);
        }
        Some(next)
    }

    fn end_if(&mut self) {
        match self.open.pop() {
            Some(Open::If { next, end }) => {
                if !next.is_empty() {
                    self.b.place(&next);
                }
                self.b.place(&end);
            }
            Some(other) => {
                self.open.push(other);
                self.err(ids::UNBALANCED, 30, "ENDIF");
            }
            None => self.err(ids::UNBALANCED, 30, "ENDIF"),
        }
    }

    fn end_select(&mut self) {
        match self.open.pop() {
            Some(Open::Select { next, end, .. }) => {
                if let Some(n) = next {
                    self.b.place(&n);
                }
                self.b.place(&end);
            }
            Some(other) => {
                self.open.push(other);
                self.err(ids::UNBALANCED, 30, "ENDSL");
            }
            None => self.err(ids::UNBALANCED, 30, "ENDSL"),
        }
    }

    fn end_loop(&mut self, inc: Option<&Expr>) {
        let Some(Open::Loop { iter, leave, end }) = self.open.pop() else {
            self.err(ids::UNBALANCED, 30, "ENDDO");
            return;
        };
        match end {
            LoopEnd::Top(top) => {
                // `ITER` goes to the top, which is placed already.
                let _ = iter;
                self.b.term(format!("br {top}"));
            }
            LoopEnd::Bottom(cond, body) => {
                self.b.place(&iter);
                // A DOU without its condition was reported at the DOU.
                if matches!(*cond, Expr::Empty(_)) {
                    self.b.term(format!("br {leave}"));
                } else if let Some(r) = self.cond(&cond) {
                    self.b.term(format!("brif {r}, {leave}, {body}"));
                }
            }
            LoopEnd::BottomCmp(sfx, a, b, body) => {
                self.b.place(&iter);
                if let Some(r) = self.cmp_suffix(&sfx, &a, &b) {
                    self.b.term(format!("brif {r}, {leave}, {body}"));
                }
            }
            LoopEnd::For { top, index, step, down } => {
                self.b.place(&iter);
                let o = if down { "dec.sub" } else { "dec.add" };
                let s = step.unwrap_or_else(|| "1".into());
                self.b.ins(format!("{o} {index}, {index}, {s} prec(31,9)"));
                self.b.term(format!("br {top}"));
            }
            LoopEnd::Counted { counter, top } => {
                self.b.place(&iter);
                let step = match inc {
                    Some(e) => self.expr(e, Want::default()),
                    None => Some(V::lit(Ty::Packed(1, 0), "1")),
                };
                if let Some(step) = step {
                    let c = V::place(Ty::Packed(15, 0), counter.clone());
                    if let Some(sum) = self.arith(BinOp::Add, &c, &step, Want::default()) {
                        self.b.ins(format!("dec.mov {counter}, {}", sum.op));
                    }
                }
                self.b.term(format!("br {top}"));
            }
        }
        self.b.place(&leave);
    }

    fn for_loop(&mut self, s: &Stmt) {
        let op = &s.op;
        let Some(Expr::Assign(_, idx, start, _)) = &op.xf2 else {
            self.err(ids::UNSUPPORTED, 30, "FOR");
            return;
        };
        let Some(iv) = self.target(idx) else { return };
        if let Some(v) = self.expr(start, Want::default()) {
            // The start is a number.
            if !v.ty.is_numeric() && v.fig.is_none() {
                self.err(ids::FOR_INITIAL, 20, "FOR");
            } else {
                self.assign(&iv, &v, false);
            }
        }
        let mut limit = None;
        let mut down = false;
        let mut by = None;
        for (w, e) in &op.more {
            match w.as_str() {
                "TO" => limit = Some(e.clone()),
                "DOWNTO" => {
                    limit = Some(e.clone());
                    down = true;
                }
                _ => by = Some(e.clone()),
            }
        }
        // The limit and step are evaluated once.
        let lim = limit.as_ref().and_then(|e| {
            let v = self.expr(e, Want::default())?;
            let t = self.b.temp("packed(31,9)");
            let opnd = self.dec_operand(&v);
            self.b.ins(format!("dec.mov {t}, {}", opnd));
            Some(t)
        });
        let step = match &by {
            Some(e) => {
                let v = self.expr(e, Want::default());
                v.map(|v| {
                    let t = self.b.temp("packed(31,9)");
                    let opnd = self.dec_operand(&v);
                    self.b.ins(format!("dec.mov {t}, {}", opnd));
                    t
                })
            }
            None => None,
        };
        let top = self.b.label();
        let body = self.b.label();
        let inc = self.b.label();
        let end = self.b.label();
        self.b.place(&top);
        if let Some(l) = &lim {
            let c = if down { "ge" } else { "le" };
            let r = self.b.reg();
            self.b.ins(format!("{r} = dec.cmp.{c} {}, {l}", iv.op));
            self.b.term(format!("brif {r}, {body}, {end}"));
        }
        self.b.place(&body);
        // The increment block, placed at ENDFOR.
        self.open.push(Open::Loop {
            iter: inc,
            leave: end,
            end: LoopEnd::For {
                top,
                index: iv.op.clone(),
                step,
                down,
            },
        });
    }

    fn counted_do(&mut self, s: &Stmt) {
        let op = &s.op;
        let counter = match op.result.first() {
            Some(r) => match self.target(r) {
                Some(v) => v.op,
                None => return,
            },
            None => self.b.temp("packed(15,0)"),
        };
        let start = op.f1.first().and_then(|e| self.expr(e, Want::default())).unwrap_or_else(|| V::lit(Ty::Packed(1, 0), "1"));
        let opnd = self.dec_operand(&start);
        self.b.ins(format!("dec.mov {counter}, {}", opnd));
        let limit = op.f2.first().and_then(|e| self.expr(e, Want::default())).unwrap_or_else(|| V::lit(Ty::Packed(1, 0), "1"));
        // The limit is a number.
        if !limit.ty.is_numeric() && limit.fig.is_none() {
            self.err(ids::BAD_FACTOR2, 30, "DO");
        }
        let lim = self.b.temp("packed(31,9)");
        let opnd = self.dec_operand(&limit);
        self.b.ins(format!("dec.mov {lim}, {}", opnd));
        let top = self.b.label();
        let body = self.b.label();
        let end = self.b.label();
        self.b.place(&top);
        let r = self.b.reg();
        self.b.ins(format!("{r} = dec.cmp.le {counter}, {lim}"));
        self.b.term(format!("brif {r}, {body}, {end}"));
        self.b.place(&body);
        let iter = self.b.label();
        self.open.push(Open::Loop {
            iter,
            leave: end,
            end: LoopEnd::Counted { counter, top },
        });
    }

    /// `RETURN [value]`.
    fn return_op(&mut self, v: Option<&Expr>) {
        if let Some(exit) = self.main_exit.clone() {
            // The main procedure: the cycle's end of calculations; it
            // returns no value.
            if v.is_some() {
                self.err(ids::RETURN_VALUE_UNWANTED, 20, "RETURN");
            }
            self.b.term(format!("br {exit}"));
            return;
        }
        // With ON-EXIT, the value waits while the exit section runs.
        if let Some((exit, temp)) = self.on_exit.clone() {
            if let (Some(e), Some(t), Some(rt)) = (v, &temp, self.ret.clone()) {
                let mode = qdecimal::Mode {
                    result_scale: rt.shape().map_or(0, |(_, s)| s),
                    ..qdecimal::Mode::default()
                };
                if let Some(val) = self.expr(e, Want { mode, compared: false }) {
                    self.assign(&V::place(rt, t.clone()), &val, false);
                }
            }
            self.b.term(format!("br {exit}"));
            return;
        }
        match (v, self.ret.clone()) {
            (Some(e), Some(rt)) => {
                let mode = qdecimal::Mode {
                    result_scale: rt.shape().map_or(0, |(_, s)| s),
                    ..qdecimal::Mode::default()
                };
                let Some(val) = self.expr(e, Want { mode, compared: false }) else { return };
                let ir = self.ir_ty(&rt);
                if is_reg_type(&rt) {
                    let r = match rt {
                        Ty::Ind => self.ind_reg(&val),
                        Ty::Ptr | Ty::ProcPtr => Some(self.ptr_reg(&val)),
                        Ty::Float(_) => Some(self.float_reg(&val)),
                        _ => {
                            let t = self.b.temp(&ir);
                            self.assign(&V::place(rt.clone(), t.clone()), &val, false);
                            let r = self.b.reg();
                            self.b.ins(format!("{r} = load {t}"));
                            Some(r)
                        }
                    };
                    if let Some(r) = r {
                        let r = if rt == Ty::Float(4) {
                            let c = self.b.reg();
                            self.b.ins(format!("{c} = cvt.f32 {r}"));
                            c
                        } else {
                            r
                        };
                        self.b.term(format!("ret {r}"));
                    }
                } else {
                    let t = self.b.temp(&ir);
                    self.assign(&V::place(rt, t.clone()), &val, false);
                    self.b.term(format!("ret {t}"));
                }
            }
            (None, None) => self.b.term("ret"),
            // The value is ignored.
            (Some(_), None) => {
                self.err(ids::RETURN_VALUE_UNWANTED, 20, "RETURN");
                self.b.term("ret");
            }
            (None, Some(_)) => self.err(ids::RETURN_VALUE_MISSING, 30, "RETURN"),
        }
    }

    fn dsply(&mut self, op: &rpgfront::ast::Op) {
        let Some(m) = op.f1.first().cloned() else {
            self.b.ins("dsply \" \"");
            return;
        };
        let Some(v) = self.expr(&m, Want::default()) else { return };
        let v = self.as_chars(&v);
        let src = if v.ty.is_numeric() && v.kind != Vk::Place {
            let t = self.b.temp(&self.ir_ty(&Ty::Packed(31, v.ty.shape().map_or(0, |s| s.1))));
            self.b.ins(format!("dec.mov {t}, {}", v.op));
            t
        } else {
            v.op.clone()
        };
        match op.result.first() {
            Some(r) => {
                // The reply as text; an empty one (a batch job's default)
                // leaves the field as it was.
                if let Some(t) = self.target(r) {
                    let reply = self.tmp(&Ty::VarChar(52, 2));
                    self.b.ins(format!("dsply {src} reply({})", reply.op));
                    let n = self.b.reg();
                    self.b.ins(format!("{n} = str.len {}", reply.op));
                    let some = self.b.reg();
                    self.b.ins(format!("{some} = cmp.gt {n}, 0"));
                    let (yes, no) = (self.b.label(), self.b.label());
                    self.b.term(format!("brif {some}, {yes}, {no}"));
                    self.b.place(&yes);
                    if t.ty.is_numeric() {
                        self.char_to_num(&t, &reply, false);
                    } else {
                        self.assign(&t, &reply, false);
                    }
                    self.b.term(format!("br {no}"));
                    self.b.place(&no);
                }
            }
            None => self.b.ins(format!("dsply {src}")),
        }
    }

    /// `WRITE file ds` to a program-described printer, and externally
    /// described formats.
    fn write_op(&mut self, op: &rpgfront::ast::Op) {
        let Some(name) = op.f2.first().and_then(|e| e.name().map(str::to_string)) else {
            self.db_op(op, "WRITE");
            return;
        };
        let f = self.files.iter().find(|f| f.name == name).cloned();
        let Some(f) = f.filter(|f| f.dev == Dev::Printer && !f.external) else {
            self.db_op(op, "WRITE");
            return;
        };
        let Some(r) = op.result.first() else {
            self.err(ids::WRITE_NO_DS, 30, name);
            return;
        };
        let Some(v) = self.expr(r, Want::default()) else { return };
        let v = self.as_chars(&v);
        // A file EXTIND left closed is not written.
        let skip = f.extind.is_some().then(|| {
            let po = self.open_flag(&f);
            let open = self.b.reg();
            self.b.ins(format!("{open} = load {po}"));
            let (yes, no) = (self.b.label(), self.b.label());
            self.b.term(format!("brif {open}, {yes}, {no}"));
            self.b.place(&yes);
            no
        });
        let st = self.b.reg();
        self.b.ins(format!(
            "{st} = prtio {} WRITE fmt({}) rec({}) ctl(0, 1, 0, 0)",
            mangle(&f.name),
            mangle(&f.name),
            v.op
        ));
        if let Some(ind) = &f.oflind {
            let ov = self.overflow_crossed(&f.name, &st);
            let on = self.b.label();
            let cont = self.b.label();
            self.b.term(format!("brif {ov}, {on}, {cont}"));
            self.b.place(&on);
            let ind = ind.clone();
            self.oflind_on(&ind);
            self.b.place(&cont);
        }
        if let Some(no) = skip {
            self.b.term(format!("br {no}"));
            self.b.place(&no);
        }
    }

    /// Fixed-form assignment: high-order digits are dropped, not an
    /// error.
    fn assign_fixed(&mut self, t: &V, v: &V, half: bool) {
        if t.ty.is_numeric() && v.ty.is_numeric() {
            let h = if half { " halfadj" } else { "" };
            let s = self.dec_operand(v);
            // TRUNCNBR(*NO): an overflow is an error, not dropped digits.
            let tr = if self.ctl.truncnbr { " trunc" } else { "" };
            self.b.ins(format!("dec.mov {}, {s}{h}{tr}", t.op));
        } else {
            self.assign(t, v, half);
        }
    }

    /// The resulting indicators of a numeric result: plus, minus, zero.
    fn num_resind(&mut self, op: &rpgfront::ast::Op, t: &V) {
        for (i, c) in ["gt", "lt", "eq"].iter().enumerate() {
            if op.resind[i].is_empty() {
                continue;
            }
            let r = self.b.reg();
            self.b.ins(format!("{r} = dec.cmp.{c} {}, 0", t.op));
            let ind = op.resind[i].clone();
            self.set_ind(&ind, &r);
        }
    }

    /// The resulting indicators of a comparison: high, low, equal.
    fn cmp_resind(&mut self, op: &rpgfront::ast::Op, a: &Expr, b: &Expr) {
        for (i, sfx) in ["GT", "LT", "EQ"].iter().enumerate() {
            if op.resind[i].is_empty() {
                continue;
            }
            if let Some(r) = self.cmp_suffix(sfx, a, b) {
                let ind = op.resind[i].clone();
                self.set_ind(&ind, &r);
            }
        }
    }

    fn fixed_arith(&mut self, base: &str, op: &rpgfront::ast::Op, half: bool) {
        let Some(r) = op.result.first().cloned() else { return };
        let Some(t) = self.target(&r) else { return };
        let (a, b) = match (op.f1.first(), op.f2.first()) {
            (Some(f1), Some(f2)) => (f1.clone(), f2.clone()),
            (None, Some(f2)) => (r.clone(), f2.clone()),
            _ => return,
        };
        let w = Want { mode: target_mode(self, &t, half), compared: false };
        let (Some(va), Some(vb)) = (self.expr(&a, w), self.expr(&b, w)) else { return };
        // DIV by a zero literal is refused as the operation's error.
        if base == "DIV" && vb.kind == Vk::Lit && vb.op.trim_start_matches('-').chars().all(|c| c == '0' || c == '.') {
            self.err(ids::DIV_FACTOR2_ZERO, 30, String::new());
            return;
        }
        if base == "DIV" {
            self.keep_div(&va, &vb, &t);
        }
        let bop = match base {
            "ADD" => BinOp::Add,
            "SUB" => BinOp::Sub,
            "MULT" => BinOp::Mul,
            _ => BinOp::Div,
        };
        if let Some(v) = self.arith(bop, &va, &vb, w) {
            self.assign_fixed(&t, &v, half);
            self.num_resind(op, &t);
        }
    }

    /// A type descriptor word for the runtime library: kind in the top
    /// byte, digits or length in the middle, decimals in the low byte.
    pub fn desc_of(&self, v: &V) -> i64 {
        let (kind, len, dec): (i64, i64, i64) = match &v.ty {
            Ty::Char(n) => (1, i64::from(*n), 0),
            Ty::VarChar(n, _) => (2, i64::from(*n), 0),
            Ty::Packed(d, s) => (3, i64::from(*d), i64::from(*s)),
            Ty::Zoned(d, s) => (4, i64::from(*d), i64::from(*s)),
            Ty::Bin(d, s) => (5, i64::from(*d), i64::from(*s)),
            Ty::Int(d) => (6, i64::from(*d), 0),
            Ty::Uns(d) => (7, i64::from(*d), 0),
            Ty::Ind => (8, 1, 0),
            Ty::Date(..) => (9, i64::from(self.t.size(&v.ty)), 0),
            Ty::Time(..) => (10, 8, 0),
            Ty::Timestamp(_) => (11, i64::from(self.t.size(&v.ty)), 0),
            Ty::Float(b) => (12, i64::from(*b), 0),
            other => (1, i64::from(self.t.size(other)), 0),
        };
        (kind << 56) | (len << 8) | dec
    }

}

/// Whether a type is returned in a register.
#[must_use]
pub fn is_reg_type(t: &Ty) -> bool {
    matches!(t, Ty::Int(_) | Ty::Uns(_) | Ty::Float(_) | Ty::Ind | Ty::Ptr | Ty::ProcPtr)
}

/// Put a type's default value into a place.
fn default_value(g: &mut Gen<'_>, ty: &Ty, dim: u32, place: &str) {
    let lit = match ty {
        t if t.is_numeric() => "*ZEROS",
        Ty::Ind => "*OFF",
        t if t.is_datetime() => &Gen::dt_default(t).unwrap_or_default(),
        Ty::Ptr | Ty::ProcPtr | Ty::Object => "",
        Ty::Ds(_) => "",
        _ => "*BLANKS",
    };
    let places: Vec<String> = if dim > 0 {
        (0..dim).map(|i| format!("{place}[{i}]")).collect()
    } else {
        vec![place.to_string()]
    };
    for p in places {
        match ty {
            Ty::Ptr | Ty::ProcPtr | Ty::Object => {
                let r = g.b.reg();
                let k = if *ty == Ty::ProcPtr { "procptr" } else { "ptr" };
                g.b.ins(format!("{r} = const.{k} *NULL"));
                g.b.ins(format!("store {p}, {r}"));
            }
            Ty::Ds(i) => {
                let len = g.t.ds_len(*i);
                g.b.ins(format!("mov {p}.all_, {}", quote(&" ".repeat(usize::try_from(len).unwrap_or(0)))));
            }
            _ => g.b.ins(format!("mov {p}, {lit}")),
        }
    }
}

/// Initialize one symbol: `INZ` values, defaults, a structure's
/// blanks and its subfields' values.
pub fn init_symbol(g: &mut Gen<'_>, id: SymId) {
    let s = g.t.syms[id].clone();
    let place = place_of(g.t, id);
    g.at = rpgfront::ast::Pos { line: s.def_line, col: 0 };
    match &s.kind {
        Some(Kind::Ds(di)) => {
            let ds = g.t.ds[*di].clone();
            let reps = s.dim.max(1);
            for r in 0..reps {
                let base = if s.dim > 0 { format!("{place}[{r}]") } else { place.clone() };
                g.b.ins(format!("mov {base}.all_, *BLANKS"));
                for &sub in &ds.subfields {
                    let ss = g.t.syms[sub].clone();
                    let p = format!("{base}.{}", mangle(&ss.name));
                    match (&ss.inz, ds.inz) {
                        (Some(Some(e)), _) => inz_value(g, &ss.ty, ss.dim, &p, e),
                        (Some(None), _) | (None, true) => default_value(g, &ss.ty, ss.dim, &p),
                        (None, false) => {
                            // Pointers are null even in a blank structure.
                            if matches!(ss.ty, Ty::Ptr | Ty::ProcPtr) {
                                default_value(g, &ss.ty, ss.dim, &p);
                            }
                        }
                    }
                }
            }
        }
        Some(Kind::Field) if s.ctdata => {
            default_value(g, &s.ty, s.dim, &place);
            load_ctdata(g, id, &place);
        }
        Some(Kind::Field) => match &s.inz {
            Some(Some(e)) => inz_value(g, &s.ty, s.dim, &place, e),
            _ => default_value(g, &s.ty, s.dim, &place),
        },
        _ => {}
    }
}

/// A compile-time array's data: its `**CTDATA name` section, else the
/// unnamed `**` section in the order the arrays are declared; `PERRCD`
/// entries per record, each the element's length; numbers as their
/// digits with the sign in the last zone.
fn load_ctdata(g: &mut Gen<'_>, id: SymId, place: &str) {
    let s = g.t.syms[id].clone();
    let order = (0..id).filter(|&i| g.t.syms[i].ctdata).count();
    let section = g
        .ctdata
        .iter()
        .find(|c| c.kind == "CTDATA" && c.name == s.name)
        .or_else(|| g.ctdata.iter().filter(|c| c.kind.is_empty()).nth(order))
        .cloned();
    let Some(section) = section else {
        g.err(ids::NOT_DEFINED, 30, format!("**CTDATA {}", s.name));
        return;
    };
    let width = usize::try_from(match &s.ty {
        t if t.is_numeric() => u32::from(t.shape().map_or(1, |x| x.0)),
        t => g.t.size(t),
    })
    .unwrap_or(1)
    .max(1);
    let per = usize::try_from(s.perrcd.max(1)).unwrap_or(1);
    let mut k = 0u32;
    for rec in &section.records {
        let chars: Vec<char> = rec.chars().collect();
        for e in 0..per {
            if k >= s.dim {
                return;
            }
            let from = e * width;
            if from >= chars.len() && !s.ty.is_numeric() {
                k += 1;
                continue;
            }
            let text: String = (from..from + width).map(|i| chars.get(i).copied().unwrap_or(' ')).collect();
            let p = format!("{place}[{k}]");
            if s.ty.is_numeric() {
                let (_, scale) = s.ty.shape().unwrap_or((1, 0));
                let digits: String = text.chars().map(|c| if c.is_ascii_digit() { c } else { '0' }).collect();
                let neg = text.chars().last().is_some_and(|c| "}JKLMNOPQR".contains(c) || c == '-');
                let digits: String = text
                    .chars()
                    .enumerate()
                    .map(|(i, c)| {
                        if i + 1 == width && neg {
                            match c {
                                '}' => '0',
                                'J'..='R' => char::from(b'1' + (c as u8 - b'J')),
                                _ => c,
                            }
                        } else if c.is_ascii_digit() {
                            c
                        } else {
                            '0'
                        }
                    })
                    .collect::<String>()
                    .chars()
                    .zip(digits.chars())
                    .map(|(a, _)| a)
                    .collect();
                let sc = usize::from(scale);
                let (i, f) = digits.split_at(digits.len().saturating_sub(sc));
                let lit = if sc > 0 { format!("{}{i}.{f}", if neg { "-" } else { "" }) } else { format!("{}{i}", if neg { "-" } else { "" }) };
                let lit = if lit.starts_with('.') || lit.starts_with("-.") { lit.replacen('.', "0.", 1) } else { lit };
                g.b.ins(format!("dec.mov {p}, {lit}"));
            } else {
                g.b.ins(format!("mov {p}, {}", quote(&text)));
            }
            k += 1;
        }
    }
}

/// Whether a literal fits a type as an initial value.
fn inz_fits(ty: &Ty, v: &V) -> bool {
    if v.kind != Vk::Lit || v.fig.is_some() {
        return true;
    }
    match ty {
        Ty::Int(d) | Ty::Uns(d) => {
            let Ok(n) = v.op.split('.').next().unwrap_or("0").parse::<i128>() else { return false };
            let bits = match d {
                3 => 8,
                5 => 16,
                10 => 32,
                _ => 64,
            };
            if matches!(ty, Ty::Int(_)) {
                n >= -(1i128 << (bits - 1)) && n < (1i128 << (bits - 1))
            } else {
                n >= 0 && n < (1i128 << bits)
            }
        }
        t if t.is_numeric() && !matches!(t, Ty::Float(_)) => {
            let (d, s) = t.shape().unwrap_or((63, 0));
            // The literal's integer digits as IBM counts them: a zero
            // written without a point is one digit; 0.4 has none.
            let lit = v.op.trim_start_matches('-');
            let whole = lit.split('.').next().unwrap_or("");
            let int_digits = match whole.trim_start_matches('0').len() {
                0 if !whole.is_empty() && !lit.contains('.') => 1,
                n => n,
            };
            int_digits <= usize::from(d - s)
        }
        t if t.is_char() && !t.is_varying() => v.ty.str_len().unwrap_or(0) <= t.str_len().unwrap_or(0),
        _ => true,
    }
}

/// Whether a literal initial value is of the field's type: a number
/// for a number, characters for characters, '0' or '1' for an
/// indicator.
fn inz_type_ok(ty: &Ty, e: &Expr) -> bool {
    match e {
        _ if *ty == Ty::Unknown => true,
        Expr::Lit(Lit::Char(s), _) => match ty {
            Ty::Ind => s == "0" || s == "1",
            t => !t.is_numeric(),
        },
        Expr::Lit(Lit::Num(_), _) => ty.is_numeric(),
        _ => true,
    }
}

/// An `INZ(value)`: the value into every element.
fn inz_value(g: &mut Gen<'_>, ty: &Ty, dim: u32, place: &str, e: &Expr) {
    if !inz_type_ok(ty, e) {
        g.err(ids::INZ_TYPE, 20, String::new());
        default_value(g, ty, dim, place);
        return;
    }
    if dim == 0 && g.dt_inz(ty, place, e) {
        return;
    }
    // More decimals than the field keeps: a warning, the rest dropped.
    if let (Expr::Lit(Lit::Num(n), _), Ty::Packed(_, s) | Ty::Zoned(_, s) | Ty::Bin(_, s)) = (e, ty)
        && n.split_once('.').is_some_and(|(_, f)| f.trim_end_matches('0').len() > usize::from(*s))
    {
        g.err(ids::INZ_DECIMALS, 10, String::new());
    }
    let Some(v) = g.expr(e, Want::default()) else { return };
    if !inz_fits(ty, &v) {
        g.err(ids::INZ_TOO_LONG, 20, String::new());
        default_value(g, ty, dim, place);
        return;
    }
    let places: Vec<String> = if dim > 0 {
        (0..dim).map(|i| format!("{place}[{i}]")).collect()
    } else {
        vec![place.to_string()]
    };
    for p in places {
        g.assign(&V::place(ty.clone(), p), &v, false);
    }
}

/// The main procedure's mainline calculations, and its subroutines by
/// name with their bodies (without `BEGSR`, with the `ENDSR`).
pub fn split_subroutines<'a>(g: &mut Gen<'_>, stmts: &[&'a Stmt]) -> (Vec<&'a Stmt>, Vec<(String, Vec<&'a Stmt>)>) {
    let mut main = Vec::new();
    let mut after: Vec<&'a Stmt> = Vec::new();
    let mut subrs: Vec<(String, Vec<&'a Stmt>)> = Vec::new();
    let mut cur: Option<(String, Vec<&'a Stmt>)> = None;
    for s in stmts {
        match s.op.code.as_str() {
            "BEGSR" => {
                if let Some(c) = cur.take() {
                    subrs.push(c);
                }
                let name = s.op.f1.first().and_then(|e| match e {
                    Expr::Ref { name, .. } => Some(name.clone()),
                    Expr::Special { word, .. } => Some(word.clone()),
                    _ => None,
                });
                let name = name.unwrap_or_default();
                // A subroutine named like a field, a record format or any
                // other name is refused; its body is not code.
                if let Some(other) = g.t.find(g.scope, &name).filter(|_| !name.starts_with('*')) {
                    // The name still counts as a use of a record format.
                    if let Some(crate::syms::Kind::Format(i)) = g.t.syms[other].kind {
                        g.used_formats.insert(i);
                    }
                    g.at = s.at;
                    g.err(ids::SUBR_NAME_TAKEN, 30, String::new());
                    g.bad_subrs.insert(name.to_ascii_uppercase());
                }
                cur = Some((name, vec![*s]));
            }
            "ENDSR" => {
                if let Some(mut c) = cur.take() {
                    c.1.push(*s);
                    subrs.push(c);
                } else {
                    main.push(*s);
                }
            }
            _ => match &mut cur {
                Some(c) => c.1.push(*s),
                None if subrs.is_empty() => main.push(*s),
                // After the subroutines: an error, and ignored.
                None => after.push(*s),
            },
        }
    }
    if let Some(c) = cur {
        subrs.push(c);
    }
    for s in after {
        g.diags.push(rpgfront::Diag::new(ids::AFTER_ENDSR, 30, s.at));
    }
    (main, subrs)
}

/// A main-procedure subroutine as a procedure: `-> i32`, the code of
/// its return point (`codegen::exc::rp`).
pub fn subroutine_proc(g: &mut Gen<'_>, name: &str, body: &[&Stmt]) {
    let saved = std::mem::replace(&mut g.b, Body::new());
    let saved_open = std::mem::take(&mut g.open);
    g.in_subr_proc = true;
    g.defined_subrs.insert(name.to_string());
    let inner: Vec<&Stmt> = body
        .iter()
        .copied()
        .filter(|s| s.op.code != "BEGSR" && s.op.code != "ENDSR")
        .collect();
    g.debug_vars(0);
    if let Some(b) = body.first() {
        g.mark(b.at);
    }
    calcs(g, &inner);
    let end = body.iter().find(|s| s.op.code == "ENDSR");
    let code = end
        .and_then(|e| match e.op.f2.first() {
            Some(Expr::Lit(Lit::Char(s), _)) => Some(crate::codegen::exc::return_point(s)),
            _ => None,
        })
        .unwrap_or(crate::codegen::exc::rp::NONE);
    if let Some(e) = end {
        g.mark(e.at);
    }
    g.b.term(format!("ret {code}"));
    g.in_subr_proc = false;
    let b = std::mem::replace(&mut g.b, saved);
    g.open = saved_open;
    g.subr_bodies.push(b.finish(&format!("proc {}() -> i32", Gen::subr_proc(name)), "ret 0"));
}

/// Generate calculations, from start to end, with their structure.
/// Subroutines come last: a calculation outside `BEGSR`..`ENDSR` after
/// the first subroutine is `RNF5005` and ignored.
pub fn calcs(g: &mut Gen<'_>, stmts: &[&Stmt]) {
    let mut k = 0;
    let mut in_subrs = false;
    while k < stmts.len() {
        let code = stmts[k].op.code.as_str();
        if code == "BEGSR" {
            in_subrs = true;
        }
        let inside = g.open.iter().any(|o| matches!(o, Open::Subr(_)));
        if in_subrs && !inside && code != "BEGSR" {
            g.diags.push(rpgfront::Diag::new(ids::AFTER_ENDSR, 30, stmts[k].at));
            k += 1;
            continue;
        }
        k = g.statement(stmts, k);
    }
    while let Some(o) = g.open.pop() {
        let what = match o {
            Open::If { .. } => "ENDIF",
            Open::Loop { .. } => "ENDDO",
            Open::Select { cas: false, .. } => "ENDSL",
            Open::Select { cas: true, .. } => "ENDCS",
            Open::Monitor { .. } => "ENDMON",
            Open::Subr(_) => "ENDSR",
        };
        g.err(ids::UNBALANCED, 30, what);
    }
}

/// Fill each subroutine's `ENDSR` switch now that every `EXSR` is known.
pub fn finish_subrs(g: &mut Gen<'_>) {
    let subrs: Vec<(String, Subr)> = g.subrs.drain().collect();
    for (_, s) in subrs {
        if !s.defined {
            // Reported with its EXSR (`blocks`). Keep the IR valid:
            // the entry label returns at once.
            g.b.place(&s.label);
            g.b.place(&s.end);
            let idx = g.b.lines.len();
            g.b.term("unreachable");
            fill_switch(g, idx, &s);
            continue;
        }
        if let Some(idx) = s.switch_line {
            fill_switch(g, idx, &s);
        }
    }
}

fn fill_switch(g: &mut Gen<'_>, idx: usize, s: &Subr) {
    if s.returns.is_empty() {
        g.b.lines[idx] = "  unreachable".into();
        return;
    }
    let r = g.b.reg();
    let arms: Vec<String> = s.returns.iter().enumerate().map(|(i, l)| format!("{}: {l}", i + 1)).collect();
    let dft = s.returns[0].clone();
    g.b.lines[idx] = format!("  {r} = load {}\n  switch {r}, [{}], default {dft}", s.slot, arms.join(", "));
}

/// A procedure body with an `ON-EXIT` section: the body in a monitor
/// region; `RETURN`, the end, or an exception go to the section; then
/// the value is returned, or the exception sent on.
fn on_exit_body(g: &mut Gen<'_>, body: &[&Stmt], at: &Stmt, exit: &[&Stmt]) {
    let value = g.ret.clone().map(|rt| {
        let t = g.b.temp(&g.ir_ty(&rt));
        default_value(g, &rt, 0, &t);
        t
    });
    let abnormal = g.b.temp("ind");
    g.b.ins(format!("store {abnormal}, *OFF"));
    g.monitors += 1;
    let name = format!("X{}", g.monitors);
    let handler = g.b.label();
    let exit_l = g.b.label();
    g.b.ins(format!("monitor {name}, *ALL -> {handler}"));
    g.on_exit = Some((exit_l.clone(), value.clone()));
    calcs(g, body);
    g.on_exit = None;
    if g.b.is_open() {
        g.b.term(format!("br {exit_l}"));
    }
    let close = g.b.label();
    g.b.place(&close);
    g.b.ins(format!("endmon {name}"));
    g.b.term(format!("br {exit_l}"));
    g.b.place(&handler);
    g.psds_error();
    g.b.ins(format!("store {abnormal}, *ON"));
    g.b.term(format!("br {exit_l}"));
    g.b.place(&exit_l);
    g.mark(at.at);
    // ON-EXIT ind: whether the procedure ended abnormally.
    if let Some(e) = &at.op.xf2
        && let Some(t) = g.target(e)
    {
        g.b.ins(format!("mov {}, {abnormal}", t.op));
    }
    calcs(g, exit);
    if g.b.is_open() {
        let again = g.b.label();
        let normal = g.b.label();
        let r = g.b.reg();
        g.b.ins(format!("{r} = load {abnormal}"));
        g.b.term(format!("brif {r}, {again}, {normal}"));
        g.b.place(&again);
        g.reraise();
        g.b.term(format!("br {normal}"));
        g.b.place(&normal);
        match (value, g.ret.clone()) {
            (Some(t), Some(rt)) if is_reg_type(&rt) => {
                let r = g.b.reg();
                g.b.ins(format!("{r} = load {t}"));
                g.b.term(format!("ret {r}"));
            }
            (Some(t), Some(_)) => g.b.term(format!("ret {t}")),
            _ => g.b.term("ret"),
        }
    }
}

/// A subprocedure.
pub fn subprocedure(g: &mut Gen<'_>, p: &Proc, scope: usize) -> String {
    g.b = Body::new();
    g.scope = scope;
    g.subrs.clear();
    g.tags.clear();
    g.scan_labels(&p.calcs);
    g.open.clear();
    g.main_exit = None;
    let name = p.name.to_ascii_uppercase();
    g.cur_proc.clone_from(&name);
    let proto = g
        .t
        .protos
        .iter()
        .find(|x| x.name == name && x.def_line == p.pi.as_ref().map_or(0, |d| d.at.line))
        .cloned()
        .or_else(|| g.t.protos.iter().rev().find(|x| x.name == name).cloned());
    g.ret = proto.as_ref().and_then(|x| x.ret.clone());
    let mut params = Vec::new();
    if let Some(pr) = &proto {
        for prm in &pr.parms {
            let id = g.t.find_in(scope, &prm.name);
            let ir = id.map_or_else(|| format!("$p_{}", mangle(&prm.name)), |i| g.t.syms[i].ir.clone());
            let ty = g.t.ir_type(&prm.ty);
            let ty = if prm.dim > 0 { format!("array[{}] of {ty}", prm.dim) } else { ty };
            let mut s = String::new();
            if prm.value {
                s.push_str("byval ");
            } else if prm.konst {
                s.push_str("const ");
            }
            s.push_str(&format!("{ir}: {ty}"));
            if prm.nopass {
                s.push_str(" *NOPASS");
            }
            // The linear main procedure's parameters may be absent: the
            // program entry passes *OMIT for those its caller left out.
            if prm.omit || g.is_linear_main(&name) && !prm.value {
                s.push_str(" *OMIT");
            }
            if prm.varsize {
                s.push_str(" *VARSIZE");
            }
            params.push(s);
        }
    }
    // Locals.
    let mut local_ids: Vec<SymId> = (0..g.t.syms.len())
        .filter(|&i| {
            let s = &g.t.syms[i];
            s.scope == scope && s.parent.is_none() && matches!(s.kind, Some(Kind::Field | Kind::Ds(_)))
        })
        .collect();
    // Pointers BASED declared come before the fields based on them.
    local_ids.sort_by_key(|&i| !(g.t.syms[i].implicit && g.t.syms[i].ty == Ty::Ptr));
    for &i in &local_ids {
        let s = g.t.syms[i].clone();
        let ty = g.t.ir_storage(&s);
        match &s.storage {
            Storage::Auto => g.b.decls.push(format!("auto {}: {ty}", s.ir)),
            // Kept between calls: initialized once, from a literal INZ.
            Storage::Static => g.b.decls.push(format!("static {}: {ty}{}", s.ir, crate::codegen::static_inz(&s))),
            Storage::Based(ptr) => g.b.decls.push(format!("based {}: {ty} on {}", s.ir, g.t.syms[*ptr].ir)),
            _ => {}
        }
    }
    for &i in &local_ids {
        if g.t.syms[i].storage == Storage::Auto {
            init_symbol(g, i);
        }
    }
    g.debug_vars(scope);
    let stmts: Vec<&Stmt> = p.calcs.iter().collect();
    if let Some(k) = stmts.iter().position(|s| s.op.code == "ON-EXIT") {
        on_exit_body(g, &stmts[..k], stmts[k], &stmts[k + 1..]);
    } else {
        calcs(g, &stmts);
    }
    if g.b.is_open() {
        match g.ret.clone() {
            None => g.b.term("ret"),
            Some(rt) => {
                // Falling off the end returns the type's default.
                let ir = g.ir_ty(&rt);
                if is_reg_type(&rt) {
                    let r = g.b.reg();
                    let k = match rt {
                        Ty::Ind => "const.ind *OFF".to_string(),
                        Ty::Float(4) => "const.f32 0.0e0".to_string(),
                        Ty::Float(_) => "const.f64 0.0e0".to_string(),
                        Ty::Ptr => "const.ptr *NULL".to_string(),
                        Ty::ProcPtr => "const.procptr *NULL".to_string(),
                        _ => format!("const.{ir} 0"),
                    };
                    g.b.ins(format!("{r} = {k}"));
                    g.b.term(format!("ret {r}"));
                } else {
                    let t = g.b.temp(&ir);
                    default_value(g, &rt, 0, &t);
                    g.b.term(format!("ret {t}"));
                }
            }
        }
    }
    finish_subrs(g);
    let export = p.kws.iter().any(|k| k.name == "EXPORT");
    let ret = g.ret.as_ref().map(|t| format!(" -> {}", g.t.ir_type(t))).unwrap_or_default();
    let body = std::mem::take(&mut g.b);
    let header = format!(
        "proc {}({}){ret}{}",
        mangle(&name),
        params.join(", "),
        if export { " export" } else { "" }
    );
    let _ = decl::int_arg;
    let _ = Lit::Num(String::new());
    body.finish(&header, "ret")
}

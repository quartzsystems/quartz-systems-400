// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The fixed-form string, move and bit operations: `MOVE`, `MOVEL`,
//! `MOVEA`, the zone moves, `CAT`, `SCAN`, `CHECK`, `CHECKR`, `XLATE`,
//! `SUBST`, `BITON`, `BITOFF`, `TESTB`, `TESTN`, `TESTZ`, `COMP`,
//! `MVR`, `SQRT`, `EVALR` and `EVAL-CORR`. Most of them are `qrpgrt`
//! procedures over operand images (`_QRNX_MOVE`, `_QRNX_CAT`, ...);
//! resulting indicators are set from what the procedure returns.

use rpgfront::ast::{AssignOp, BinOp, Expr, Fig, Lit, Op, Stmt};

use crate::codegen::Gen;
use crate::codegen::emit::quote;
use crate::codegen::expr::{V, Vk, Want};
use crate::diag::ids;
use crate::names::mangle;
use crate::syms::{ConstVal, Kind, SymId};
use crate::types::Ty;

/// What the string, array and move operations keep while a module is
/// generated.
#[derive(Clone, Debug, Default)]
pub struct OpState {
    /// The last `DIV`'s dividend, divisor and quotient, for `MVR`.
    pub div: Option<(V, V, V)>,
    /// Tables whose current element is kept.
    pub tables: std::collections::BTreeSet<SymId>,
    /// Multiple-occurrence structures in use.
    pub occurs: std::collections::BTreeSet<SymId>,
    /// Globals `RESET` restores.
    pub resets: std::collections::BTreeSet<SymId>,
}

/// An operand as a runtime procedure takes it: storage and its
/// descriptor word.
#[derive(Clone, Debug)]
pub struct Rt {
    /// The place.
    pub op: String,
    /// The descriptor.
    pub desc: i64,
}

/// The operations this module generates.
pub const OPS: &[&str] = &[
    "MOVE", "MOVEL", "MOVEA", "MHHZO", "MHLZO", "MLHZO", "MLLZO", "CAT", "SCAN", "CHECK", "CHECKR",
    "XLATE", "SUBST", "BITON", "BITOFF", "TESTB", "TESTN", "TESTZ", "COMP", "MVR", "SQRT", "EVALR",
    "EVAL-CORR", "XFOOT", "LOOKUP", "SORTA", "OCCUR", "CLEAR", "RESET", "FOR-EACH",
];

impl Gen<'_> {
    /// Generate one of [`OPS`]; false when `base` is not one of them.
    pub fn strop(&mut self, base: &str, s: &Stmt) -> bool {
        if !OPS.contains(&base) {
            return false;
        }
        let op = &s.op;
        let pad = op.ext.contains('P');
        match base {
            "MOVE" | "MOVEL" => self.move_rt(base == "MOVEL", pad, op),
            "MOVEA" => self.movea(pad, op),
            "MHHZO" | "MHLZO" | "MLHZO" | "MLLZO" => self.zone_op(base, op),
            "CAT" => self.cat_op(pad, op),
            "SCAN" => self.scan_op(op),
            "CHECK" | "CHECKR" => self.check_op(base == "CHECKR", op),
            "XLATE" => self.xlate_op(pad, op),
            "SUBST" => self.subst_op(pad, op),
            "BITON" | "BITOFF" | "TESTB" => self.bit_op(base, op),
            "TESTN" | "TESTZ" => self.testnz(base == "TESTN", op),
            "COMP" => {
                if let (Some(a), Some(b)) = (op.f1.first().cloned(), op.f2.first().cloned()) {
                    self.resind3(op, &a, &b);
                }
            }
            "MVR" => self.mvr(op),
            "SQRT" => self.sqrt_op(op),
            "EVALR" => {
                if let Some(Expr::Assign(aop, t, v, _)) = &op.xf2 {
                    self.evalr(*aop, t, v);
                }
            }
            "EVAL-CORR" => {
                if let Some(Expr::Assign(_, t, v, _)) = &op.xf2 {
                    self.eval_corr(t, v, op.ext.contains('H'));
                }
            }
            "XFOOT" => self.xfoot_op(op),
            "LOOKUP" => self.lookup_op(op),
            "SORTA" => self.sorta_op(op),
            "OCCUR" => self.occur_op(op),
            "FOR-EACH" => self.for_each(op),
            _ => self.clear_reset(base == "RESET", op),
        }
        true
    }

    /// Declare a runtime library procedure.
    fn rtp(&mut self, name: &str, decl: &str) {
        self.rt_proc(name, decl);
    }

    /// A value as storage for a runtime procedure. A figurative constant
    /// takes the type of `like` (character when there is none).
    pub fn rt_of(&mut self, v: &V, like: Option<&V>) -> Rt {
        let v = self.as_chars(v);
        if let Some(f) = v.fig.clone()
            && !matches!(f, Fig::On | Fig::Off)
        {
            let ty = like.map_or(Ty::Char(1), |l| match &l.ty {
                Ty::VarChar(n, _) => Ty::Char((*n).max(1)),
                Ty::Ind => Ty::Char(1),
                t if t.is_numeric() && matches!(f, Fig::Blanks) => Ty::Char(u32::from(t.shape().map_or(1, |x| x.0))),
                t => t.clone(),
            });
            let t = self.b.temp(&self.ir_ty(&ty));
            let tv = V::place(ty.clone(), t.clone());
            self.fill_fig(&tv, &f);
            return Rt { desc: self.desc_of(&tv), op: t };
        }
        match (&v.ty, v.kind) {
            (_, Vk::Place) => Rt { desc: self.desc_of(&v), op: v.op.clone() },
            (Ty::Ind, _) => {
                let t = self.b.temp("ind");
                self.b.ins(format!("store {t}, {}", v.op));
                Rt { desc: self.desc_of(&V::place(Ty::Ind, t.clone())), op: t }
            }
            (t, _) if t.is_numeric() => {
                let p = self.num_place(&v);
                Rt { desc: self.desc_of(&p), op: p.op }
            }
            (t, _) if t.str_len() == Some(0) => {
                // An empty literal: a varying value of no characters.
                let tmp = self.b.temp("varchar(1)");
                self.b.ins(format!("mov {tmp}, \"\""));
                Rt { desc: self.desc_of(&V::place(Ty::VarChar(1, 2), tmp.clone())), op: tmp }
            }
            (t, _) => {
                let n = t.str_len().unwrap_or(1).max(1);
                let tmp = self.b.temp(&format!("char({n})"));
                self.b.ins(format!("mov {tmp}, {}", v.op));
                Rt { desc: self.desc_of(&V::place(Ty::Char(n), tmp.clone())), op: tmp }
            }
        }
    }

    /// A figurative constant into a place of its own type.
    fn fill_fig(&mut self, t: &V, f: &Fig) {
        let n = usize::try_from(match &t.ty {
            ty if ty.is_numeric() => u32::from(ty.shape().map_or(1, |s| s.0)),
            ty => self.t.size(ty),
        })
        .unwrap_or(1);
        match f {
            Fig::All(p) | Fig::AllG(p) => {
                if t.ty.is_numeric() {
                    let lit: String = p.chars().cycle().take(n).collect();
                    let digits: String = lit.chars().map(|c| if c.is_ascii_digit() { c } else { '0' }).collect();
                    let s = t.ty.shape().map_or(0, |x| x.1) as usize;
                    let (i, d) = digits.split_at(digits.len() - s.min(digits.len()));
                    let text = if s > 0 { format!("{}.{d}", if i.is_empty() { "0" } else { i }) } else { i.to_string() };
                    self.b.ins(format!("dec.mov {}, {text}", t.op));
                } else {
                    let lit: String = p.chars().cycle().take(n).collect();
                    self.b.ins(format!("mov {}, {}", t.op, quote(&lit)));
                }
            }
            Fig::AllX(h) => {
                let pairs: Vec<String> = h.as_bytes().chunks(2).map(|c| String::from_utf8_lossy(c).to_string()).collect();
                let bytes: String = pairs.iter().cycle().take(n).cloned().collect();
                self.b.ins(format!("mov {}, x\"{bytes}\"", t.op));
            }
            _ => self.b.ins(format!("mov {}, {}", t.op, crate::codegen::expr::fig_text(f))),
        }
    }

    /// An `i64` register for an optional integer operand, or a default.
    fn int_or(&mut self, e: Option<&Expr>, dft: i64) -> Option<String> {
        match e {
            Some(e) => {
                let v = self.expr(e, Want::default())?;
                self.int_reg(&v)
            }
            None => Some(dft.to_string()),
        }
    }

    /// The resulting indicators of a numeric result: plus, minus, zero.
    pub fn sign_inds(&mut self, op: &Op, t: &V) {
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

    /// High, low and equal indicators of `a` against `b`.
    pub fn resind3(&mut self, op: &Op, a: &Expr, b: &Expr) {
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

    /// Indicators from a code a procedure returned: the indicator of
    /// column `i` is on when the code is `codes[i]`.
    fn code_inds(&mut self, op: &Op, code: &str, codes: [i32; 3]) {
        // The same indicator in two columns is on for either.
        let mut done: Vec<String> = Vec::new();
        for i in 0..3 {
            let ind = op.resind[i].clone();
            if ind.is_empty() || done.contains(&ind) {
                continue;
            }
            let mut acc: Option<String> = None;
            for (j, want) in codes.iter().enumerate().skip(i) {
                if op.resind[j] == ind {
                    let r = self.b.reg();
                    self.b.ins(format!("{r} = cmp.eq {code}, {want}"));
                    acc = Some(match acc {
                        None => r,
                        Some(a) => {
                            let o = self.b.reg();
                            self.b.ins(format!("{o} = select {a}, {a}, {r}"));
                            o
                        }
                    });
                }
            }
            if let Some(a) = acc {
                self.set_ind(&ind, &a);
            }
            done.push(ind);
        }
    }

    /// `MOVE` and `MOVEL`.
    fn move_rt(&mut self, left: bool, pad: bool, op: &Op) {
        let (Some(f2), Some(r)) = (op.f2.first().cloned(), op.result.first().cloned()) else { return };
        let Some(t) = self.target(&r) else { return };
        let Some(v) = self.expr(&f2, Want::default()) else { return };
        // An indicator takes only '0' or '1'.
        if t.ty == Ty::Ind
            && let Expr::Lit(Lit::Char(s), _) = &f2
            && s != "0"
            && s != "1"
        {
            self.err(ids::IND_NOT_01, 30, String::new());
            return;
        }
        // A structure and its own subfield overlap.
        if self.overlapping(&f2, &r) {
            self.err(ids::OVERLAPPING, 30, String::new());
            return;
        }
        let t = self.as_chars(&t);
        if matches!(t.ty, Ty::Float(_)) || matches!(v.ty, Ty::Float(_)) {
            self.err(ids::TYPE_MISMATCH, 30, op.code.clone());
            return;
        }
        let src = self.rt_of(&v, Some(&t));
        let dd = self.desc_of(&t);
        self.rtp(
            "_QRNX_MOVE",
            "(byval $left: i32, byval $pad: i32, $dst: bytes(1) *VARSIZE, byval $dd: i64, $src: bytes(1) *VARSIZE, byval $sd: i64)",
        );
        self.b.ins(format!(
            "callb _QRNX_MOVE({}, {}, {}, {dd}, {}, {})",
            i32::from(left),
            i32::from(pad),
            t.op,
            src.op,
            src.desc
        ));
        self.move_inds(op, &t);
    }

    /// Whether two operands are a structure and one of its subfields.
    fn overlapping(&self, a: &Expr, b: &Expr) -> bool {
        let sym = |e: &Expr| match e {
            Expr::Ref { name, steps, .. } if steps.is_empty() => self.t.find(self.scope, name),
            _ => None,
        };
        let (Some(x), Some(y)) = (sym(a), sym(b)) else { return false };
        let within = |s: SymId, ds: SymId| {
            let mut p = self.t.syms[s].parent;
            while let Some(q) = p {
                if q == ds {
                    return true;
                }
                p = self.t.syms[q].parent;
            }
            false
        };
        within(x, y) || within(y, x)
    }

    /// The resulting indicators of a move: plus, minus, zero for a
    /// number; blank (equal column) for character data.
    fn move_inds(&mut self, op: &Op, t: &V) {
        if t.ty.is_numeric() {
            self.sign_inds(op, t);
        } else if !op.resind[2].is_empty() && t.ty.is_string() {
            let r = self.b.reg();
            self.b.ins(format!("{r} = str.cmp.eq {}, *BLANKS", t.op));
            let ind = op.resind[2].clone();
            self.set_ind(&ind, &r);
        }
    }

    /// An array operand from the element it names on: the place of that
    /// element, its type, and the number of elements to the end.
    pub fn array_from(&mut self, e: &Expr) -> Option<(V, String)> {
        match e {
            Expr::Special { word, index, .. } if word == "*IN" => {
                let (place, count) = match index {
                    Some(i) => {
                        let z = self.index(i, 99)?;
                        let c = self.remaining(99, &z);
                        (format!("$in.x[{z}]"), c)
                    }
                    None => ("$in.x[0]".to_string(), "99".to_string()),
                };
                Some((V::place(Ty::Ind, place), count))
            }
            Expr::Ref { name, steps, .. } => {
                let id = self.t.find(self.scope, name)?;
                let sym = self.t.syms[id].clone();
                if sym.dim == 0 || steps.len() > 1 {
                    return None;
                }
                self.t.refer(id, self.at.line, true);
                let base = crate::codegen::place_of(self.t, id);
                let (place, count) = match steps.first() {
                    Some(rpgfront::ast::Step::Args(a)) if a.len() == 1 => {
                        let z = self.index(&a[0], sym.dim)?;
                        let c = self.remaining(sym.dim, &z);
                        (format!("{base}[{z}]"), c)
                    }
                    None => (format!("{base}[0]"), sym.dim.to_string()),
                    _ => return None,
                };
                Some((V { sym: Some(id), ..V::place(sym.ty.clone(), place) }, count))
            }
            _ => None,
        }
    }

    /// The elements from 0-based `z` to the end of `dim`.
    pub fn remaining(&mut self, dim: u32, z: &str) -> String {
        if let Ok(k) = z.parse::<u32>() {
            return dim.saturating_sub(k).to_string();
        }
        let c = self.b.reg();
        self.b.ins(format!("{c} = sub {dim}, {z}"));
        c
    }

    /// `MOVEA`: arrays, fields and literals, from the elements named.
    fn movea(&mut self, pad: bool, op: &Op) {
        let (Some(f2), Some(r)) = (op.f2.first().cloned(), op.result.first().cloned()) else { return };
        let (dst, dcount, dst_array) = match self.array_from(&r) {
            Some((d, c)) => (d, c, true),
            None => {
                let Some(t) = self.target(&r) else { return };
                (self.as_chars(&t), "1".to_string(), false)
            }
        };
        let (src, scount) = match self.array_from(&f2) {
            Some((v, c)) => {
                // Numeric arrays move between elements of one length.
                if dst_array
                    && v.ty.is_numeric()
                    && dst.ty.is_numeric()
                    && v.ty.shape() != dst.ty.shape()
                {
                    self.err(ids::MOVEA_TYPES, 30, String::new());
                    return;
                }
                (self.rt_of(&v, None), c)
            }
            None => {
                let Some(v) = self.expr(&f2, Want::default()) else { return };
                if v.fig.is_some() && !matches!(v.fig, Some(Fig::On | Fig::Off)) {
                    // A figurative constant fills the whole target.
                    let total = self.movea_len(&r).max(1);
                    let like = if dst.ty.is_numeric() { V::place(Ty::Zoned(u8::try_from(total.min(63)).unwrap_or(63), 0), "") } else { V::place(Ty::Char(total), "") };
                    (self.rt_of(&v, Some(&like)), "1".to_string())
                } else {
                    (self.rt_of(&v, None), "1".to_string())
                }
            }
        };
        let dd = self.desc_of(&dst);
        self.rtp(
            "_QRNX_MOVEA",
            "(byval $pad: i32, $dst: bytes(1) *VARSIZE, byval $dd: i64, byval $dn: i64, $src: bytes(1) *VARSIZE, byval $sd: i64, byval $sn: i64)",
        );
        self.b.ins(format!(
            "callb _QRNX_MOVEA({}, {}, {dd}, {dcount}, {}, {}, {scount})",
            i32::from(pad),
            dst.op,
            src.op,
            src.desc
        ));
    }

    /// The bytes a `MOVEA` target holds from its first element named.
    fn movea_len(&mut self, e: &Expr) -> u32 {
        match e {
            Expr::Special { .. } => 99,
            Expr::Ref { name, .. } => self.t.find(self.scope, name).map_or(1, |id| {
                let s = &self.t.syms[id];
                let one = match &s.ty {
                    t if t.is_numeric() => u32::from(t.shape().map_or(1, |x| x.0)),
                    t => self.t.size(t),
                };
                one * s.dim.max(1)
            }),
            _ => 1,
        }
    }

    /// `MHHZO`, `MHLZO`, `MLHZO`, `MLLZO`.
    fn zone_op(&mut self, base: &str, op: &Op) {
        let (Some(f2), Some(r)) = (op.f2.first().cloned(), op.result.first().cloned()) else { return };
        let Some(t) = self.target(&r) else { return };
        let Some(v) = self.expr(&f2, Want::default()) else { return };
        let t = self.as_chars(&t);
        let src = self.rt_of(&v, Some(&t));
        let code = match base {
            "MHHZO" => 0,
            "MHLZO" => 1,
            "MLHZO" => 2,
            _ => 3,
        };
        let dd = self.desc_of(&t);
        self.rtp(
            "_QRNX_ZONE",
            "(byval $op: i32, $dst: bytes(1) *VARSIZE, byval $dd: i64, $src: bytes(1) *VARSIZE, byval $sd: i64)",
        );
        self.b.ins(format!("callb _QRNX_ZONE({code}, {}, {dd}, {}, {})", t.op, src.op, src.desc));
    }

    /// `CAT`: factor 1 (or the result) then factor 2, with `:n` blanks
    /// between them in place of factor 1's trailing blanks.
    fn cat_op(&mut self, pad: bool, op: &Op) {
        let (Some(f2), Some(r)) = (op.f2.first().cloned(), op.result.first().cloned()) else { return };
        let Some(t) = self.target(&r) else { return };
        let t = self.as_chars(&t);
        if !t.ty.is_char() {
            self.err(ids::TYPE_MISMATCH, 30, "CAT");
            return;
        }
        let a = match op.f1.first() {
            Some(e) => {
                let Some(v) = self.expr(e, Want::default()) else { return };
                self.rt_of(&v, None)
            }
            None => Rt { desc: self.desc_of(&t), op: t.op.clone() },
        };
        let Some(bv) = self.expr(&f2, Want::default()) else { return };
        let b = self.rt_of(&bv, None);
        let Some(blanks) = self.int_or(op.f2.get(1), -1) else { return };
        let dd = self.desc_of(&t);
        self.rtp(
            "_QRNX_CAT",
            "(byval $pad: i32, byval $n: i64, $dst: bytes(1) *VARSIZE, byval $dd: i64, $a: bytes(1) *VARSIZE, byval $ad: i64, $b: bytes(1) *VARSIZE, byval $bd: i64)",
        );
        self.b.ins(format!(
            "callb _QRNX_CAT({}, {blanks}, {}, {dd}, {}, {}, {}, {})",
            i32::from(pad),
            t.op,
            a.op,
            a.desc,
            b.op,
            b.desc
        ));
    }

    /// The result of `SCAN`, `CHECK`, `CHECKR`: a numeric field or
    /// array, or none (a dummy place and no elements).
    fn positions_result(&mut self, op: &Op) -> Option<(String, i64, String)> {
        match op.result.first() {
            Some(r) => {
                let t = self.target(r)?;
                if !t.ty.is_numeric() {
                    self.err(ids::TYPE_MISMATCH, 30, op.code.clone());
                    return None;
                }
                let n = t.dim.max(1);
                let place = if t.dim > 0 { format!("{}[0]", t.op) } else { t.op.clone() };
                let d = self.desc_of(&V::place(t.ty.clone(), place.clone()));
                Some((place, d, n.to_string()))
            }
            None => {
                let tmp = self.b.temp("packed(5,0)");
                let d = self.desc_of(&V::place(Ty::Packed(5, 0), tmp.clone()));
                Some((tmp, d, "0".into()))
            }
        }
    }

    /// `SCAN`: factor 1 the string (`:length`), factor 2 the base
    /// (`:start`); the equal indicator when found, the low on error.
    fn scan_op(&mut self, op: &Op) {
        let (Some(f1), Some(f2)) = (op.f1.first().cloned(), op.f2.first().cloned()) else { return };
        let Some(nv) = self.expr(&f1, Want::default()) else { return };
        let needle = self.rt_of(&nv, None);
        let Some(nlen) = self.int_or(op.f1.get(1), -1) else { return };
        let Some(hv) = self.expr(&f2, Want::default()) else { return };
        let hay = self.rt_of(&hv, None);
        let Some(start) = self.int_or(op.f2.get(1), 1) else { return };
        let Some((res, rd, n)) = self.positions_result(op) else { return };
        self.rtp(
            "_QRNX_SCAN",
            "($n: bytes(1) *VARSIZE, byval $nd: i64, byval $nl: i64, $h: bytes(1) *VARSIZE, byval $hd: i64, byval $st: i64, $r: bytes(1) *VARSIZE, byval $rd: i64, byval $rn: i64) -> i32",
        );
        let c = self.b.reg();
        self.b.ins(format!(
            "{c} = callb _QRNX_SCAN({}, {}, {nlen}, {}, {}, {start}, {res}, {rd}, {n})",
            needle.op, needle.desc, hay.op, hay.desc
        ));
        self.code_inds(op, &c, [-1, 2, 1]);
    }

    /// `CHECK` and `CHECKR`.
    fn check_op(&mut self, right: bool, op: &Op) {
        let (Some(f1), Some(f2)) = (op.f1.first().cloned(), op.f2.first().cloned()) else { return };
        let Some(sv) = self.expr(&f1, Want::default()) else { return };
        let set = self.rt_of(&sv, None);
        let Some(bv) = self.expr(&f2, Want::default()) else { return };
        let base = self.rt_of(&bv, None);
        let Some(start) = self.int_or(op.f2.get(1), 0) else { return };
        let Some((res, rd, n)) = self.positions_result(op) else { return };
        self.rtp(
            "_QRNX_CHECK",
            "(byval $right: i32, $s: bytes(1) *VARSIZE, byval $sd: i64, $b: bytes(1) *VARSIZE, byval $bd: i64, byval $st: i64, $r: bytes(1) *VARSIZE, byval $rd: i64, byval $rn: i64) -> i32",
        );
        let c = self.b.reg();
        self.b.ins(format!(
            "{c} = callb _QRNX_CHECK({}, {}, {}, {}, {}, {start}, {res}, {rd}, {n})",
            i32::from(right),
            set.op,
            set.desc,
            base.op,
            base.desc
        ));
        self.code_inds(op, &c, [-1, 2, 1]);
    }

    /// `XLATE from:to source:start result`.
    fn xlate_op(&mut self, pad: bool, op: &Op) {
        let (Some(fe), Some(te), Some(f2), Some(r)) =
            (op.f1.first().cloned(), op.f1.get(1).cloned(), op.f2.first().cloned(), op.result.first().cloned())
        else {
            self.err(ids::BAD_EXPRESSION_TYPE, 30, "XLATE");
            return;
        };
        let Some(fv) = self.expr(&fe, Want::default()) else { return };
        let from = self.rt_of(&fv, None);
        let Some(tv) = self.expr(&te, Want::default()) else { return };
        let to = self.rt_of(&tv, None);
        let Some(sv) = self.expr(&f2, Want::default()) else { return };
        let src = self.rt_of(&sv, None);
        let Some(start) = self.int_or(op.f2.get(1), 1) else { return };
        let Some(t) = self.target(&r) else { return };
        let t = self.as_chars(&t);
        let dd = self.desc_of(&t);
        self.rtp(
            "_QRNX_XLATE",
            "(byval $pad: i32, $f: bytes(1) *VARSIZE, byval $fd: i64, $t: bytes(1) *VARSIZE, byval $td: i64, $s: bytes(1) *VARSIZE, byval $sd: i64, byval $st: i64, $d: bytes(1) *VARSIZE, byval $dd: i64) -> i32",
        );
        let c = self.b.reg();
        self.b.ins(format!(
            "{c} = callb _QRNX_XLATE({}, {}, {}, {}, {}, {}, {}, {start}, {}, {dd})",
            i32::from(pad),
            from.op,
            from.desc,
            to.op,
            to.desc,
            src.op,
            src.desc,
            t.op
        ));
        self.code_inds(op, &c, [-1, 2, -1]);
    }

    /// `SUBST length source:start result`.
    fn subst_op(&mut self, pad: bool, op: &Op) {
        let (Some(f2), Some(r)) = (op.f2.first().cloned(), op.result.first().cloned()) else { return };
        let Some(len) = self.int_or(op.f1.first(), -1) else { return };
        let Some(sv) = self.expr(&f2, Want::default()) else { return };
        // Constant positions past a fixed-length base.
        let lit = |e: Option<&Expr>| match e {
            Some(Expr::Lit(Lit::Num(n), _)) => n.parse::<u32>().ok(),
            _ => None,
        };
        if let Some(n) = sv.ty.str_len().filter(|_| !sv.ty.is_varying()) {
            let (cl, cs) = (lit(op.f1.first()), lit(op.f2.get(1)));
            let mut bad = false;
            if cs.is_some_and(|s| s > n) {
                self.err(ids::SUBST_START_PAST, 30, String::new());
                bad = true;
            }
            if cl.is_some_and(|l| cs.unwrap_or(1) + l - 1 > n) {
                self.err(ids::SUBST_LEN_PAST, 30, String::new());
                bad = true;
            }
            if bad {
                return;
            }
        }
        let src = self.rt_of(&sv, None);
        let Some(start) = self.int_or(op.f2.get(1), 1) else { return };
        let Some(t) = self.target(&r) else { return };
        let t = self.as_chars(&t);
        let dd = self.desc_of(&t);
        self.rtp(
            "_QRNX_SUBST",
            "(byval $pad: i32, byval $l: i64, $s: bytes(1) *VARSIZE, byval $sd: i64, byval $st: i64, $d: bytes(1) *VARSIZE, byval $dd: i64) -> i32",
        );
        let c = self.b.reg();
        self.b.ins(format!(
            "{c} = callb _QRNX_SUBST({}, {len}, {}, {}, {start}, {}, {dd})",
            i32::from(pad),
            src.op,
            src.desc,
            t.op
        ));
        self.code_inds(op, &c, [-1, 2, -1]);
    }

    /// The bit mask of a `BITON`, `BITOFF` or `TESTB` factor 2: bit
    /// numbers in a character literal or constant, a hexadecimal
    /// literal, or a one-byte field.
    fn bit_mask(&mut self, e: &Expr) -> Option<String> {
        let lit = match e {
            Expr::Lit(l, _) => Some(l.clone()),
            Expr::Ref { name, steps, .. } if steps.is_empty() => self.t.find(self.scope, name).and_then(|id| match &self.t.syms[id].kind {
                Some(Kind::Const(ConstVal::Expr(Expr::Lit(l, _)))) => Some(l.clone()),
                _ => None,
            }),
            _ => None,
        };
        let byte = match lit {
            Some(Lit::Char(bits)) => {
                let mut m = 0u8;
                for c in bits.chars() {
                    match c.to_digit(10) {
                        Some(d) if d <= 7 => m |= 0x80 >> d,
                        _ => {
                            self.err(ids::BAD_EXPRESSION_TYPE, 30, bits.clone());
                            return None;
                        }
                    }
                }
                Some(m)
            }
            Some(Lit::Hex(h)) => u8::from_str_radix(&h, 16).ok(),
            _ => None,
        };
        if let Some(m) = byte {
            let t = self.b.temp("char(1)");
            self.b.ins(format!("mov {t}, x\"{m:02X}\""));
            return Some(t);
        }
        let v = self.expr(e, Want::default())?;
        let v = self.as_chars(&v);
        if v.kind != Vk::Place || v.ty != Ty::Char(1) {
            self.err(ids::TYPE_MISMATCH, 30, "");
            return None;
        }
        Some(v.op)
    }

    /// `BITON`, `BITOFF`, `TESTB` on a one-byte character field.
    fn bit_op(&mut self, base: &str, op: &Op) {
        let (Some(f2), Some(r)) = (op.f2.first().cloned(), op.result.first().cloned()) else { return };
        let Some(mask) = self.bit_mask(&f2) else { return };
        let Some(t) = (if base == "TESTB" { self.expr(&r, Want::default()) } else { self.target(&r) }) else { return };
        let t = self.as_chars(&t);
        if t.kind != Vk::Place || t.ty != Ty::Char(1) {
            self.err(ids::TYPE_MISMATCH, 30, op.code.clone());
            return;
        }
        let code = match base {
            "BITON" => 0,
            "BITOFF" => 1,
            _ => 2,
        };
        self.rtp("_QRNX_BITS", "(byval $op: i32, $d: bytes(1) *VARSIZE, $m: bytes(1) *VARSIZE) -> i32");
        let c = self.b.reg();
        self.b.ins(format!("{c} = callb _QRNX_BITS({code}, {}, {mask})", t.op));
        if code == 2 {
            self.code_inds(op, &c, [1, 2, 3]);
        }
    }

    /// `TESTN` and `TESTZ` of a character field.
    fn testnz(&mut self, n: bool, op: &Op) {
        let Some(r) = op.result.first().cloned() else { return };
        let Some(v) = self.expr(&r, Want::default()) else { return };
        let v = self.as_chars(&v);
        if !v.ty.is_char() {
            self.err(ids::TYPE_MISMATCH, 30, op.code.clone());
            return;
        }
        let src = self.rt_of(&v, None);
        let name = if n { "_QRNX_TESTN" } else { "_QRNX_TESTZ" };
        self.rtp(name, "($s: bytes(1) *VARSIZE, byval $sd: i64) -> i32");
        let c = self.b.reg();
        self.b.ins(format!("{c} = callb {name}({}, {})", src.op, src.desc));
        self.code_inds(op, &c, [1, 2, 3]);
    }

    /// Keep a `DIV`'s operands for an `MVR` after it.
    pub fn keep_div(&mut self, a: &V, b: &V, q: &V) {
        let a = self.num_place(a);
        let b = self.num_place(b);
        let ta = self.b.temp(&self.ir_ty(&a.ty));
        let tb = self.b.temp(&self.ir_ty(&b.ty));
        let (oa, ob) = (self.dec_operand(&a), self.dec_operand(&b));
        self.b.ins(format!("dec.mov {ta}, {oa}"));
        self.b.ins(format!("dec.mov {tb}, {ob}"));
        self.ops.div = Some((V::place(a.ty.clone(), ta), V::place(b.ty.clone(), tb), q.clone()));
    }

    /// `MVR`: the dividend less the quotient times the divisor of the
    /// `DIV` before it.
    fn mvr(&mut self, op: &Op) {
        let Some((a, b, q)) = self.ops.div.clone() else {
            self.err(ids::UNBALANCED, 30, "MVR");
            return;
        };
        let Some(r) = op.result.first().cloned() else { return };
        let Some(t) = self.target(&r) else { return };
        let (_, sa) = a.ty.shape().unwrap_or((31, 0));
        let (_, sb) = b.ty.shape().unwrap_or((31, 0));
        let (_, sq) = q.ty.shape().unwrap_or((31, 0));
        let sp = sq + sb;
        let s = sa.max(sp).min(31);
        let prod = self.b.temp(&format!("packed(63,{})", sp.min(62)));
        let rem = self.b.temp(&format!("packed(63,{s})"));
        let qo = self.dec_operand(&q);
        self.b.ins(format!("dec.mul {prod}, {qo}, {} prec(63,{})", b.op, sp.min(62)));
        self.b.ins(format!("dec.sub {rem}, {}, {prod} prec(63,{s})", a.op));
        self.b.ins(format!("dec.mov {}, {rem} trunc", t.op));
        self.sign_inds(op, &t);
    }

    /// `SQRT value result`.
    fn sqrt_op(&mut self, op: &Op) {
        let (Some(f2), Some(r)) = (op.f2.first().cloned(), op.result.first().cloned()) else { return };
        let Some(t) = self.target(&r) else { return };
        let Some(v) = self.expr(&f2, Want::default()) else { return };
        let f = self.float_reg(&v);
        self.rt_proc("_QRNX_SQRT", "(byval $a: f64) -> f64");
        let s = self.b.reg();
        self.b.ins(format!("{s} = callb _QRNX_SQRT({f})"));
        match &t.ty {
            Ty::Float(4) => {
                let c = self.b.reg();
                self.b.ins(format!("{c} = cvt.f32 {s}"));
                self.b.ins(format!("store {}, {c}", t.op));
            }
            Ty::Float(_) => self.b.ins(format!("store {}, {s}", t.op)),
            ty if ty.is_numeric() => {
                let h = if op.ext.contains('H') { " halfadj" } else { "" };
                self.b.ins(format!("dec.mov {}, {s}{h} trunc", t.op));
            }
            _ => self.err(ids::TYPE_MISMATCH, 30, "SQRT"),
        }
    }

    /// `EVALR`: the value right-adjusted in a fixed-length target.
    fn evalr(&mut self, aop: AssignOp, t: &Expr, v: &Expr) {
        let Some(tv) = self.target(t) else { return };
        // The target is fixed-length character or graphic.
        if tv.ty.is_varying() {
            self.err(ids::EVALR_VARYING, 30, t.name().unwrap_or_default().to_string());
            return;
        }
        let tv = self.as_chars(&tv);
        let Some(val) = self.expr(v, Want::default()) else { return };
        if aop != AssignOp::Set || !matches!(tv.ty, Ty::Char(_)) || val.fig.is_some() {
            self.assign(&tv, &val, false);
            return;
        }
        let val = self.as_chars(&val);
        if !val.ty.is_char() {
            self.err(ids::ASSIGN_TYPE, 30, "EVALR");
            return;
        }
        let src = self.rt_of(&val, None);
        let dd = self.desc_of(&tv);
        self.rtp(
            "_QRNX_MOVE",
            "(byval $left: i32, byval $pad: i32, $dst: bytes(1) *VARSIZE, byval $dd: i64, $src: bytes(1) *VARSIZE, byval $sd: i64)",
        );
        self.b.ins(format!("callb _QRNX_MOVE(0, 1, {}, {dd}, {}, {})", tv.op, src.op, src.desc));
    }

    /// `EVAL-CORR`: each subfield of the target with a namesake in the
    /// source gets its value; structures nested in both, recursively.
    fn eval_corr(&mut self, t: &Expr, v: &Expr, half: bool) {
        let (Some(tv), Some(sv)) = (self.target(t), self.expr(v, Want::default())) else { return };
        let (Ty::Ds(td), Ty::Ds(sd)) = (tv.ty.clone(), sv.ty.clone()) else {
            self.err(ids::NOT_A_DS, 30, "EVAL-CORR");
            return;
        };
        if !self.corr_complete(td, sd) {
            self.err(ids::CORR_IGNORED, 0, "EVAL-CORR");
        }
        self.corr(td, &tv.op, sd, &sv.op, half);
    }

    /// Whether every subfield of each structure has a namesake in the
    /// other.
    fn corr_complete(&self, td: usize, sd: usize) -> bool {
        let names = |d: usize| -> Vec<String> { self.t.ds[d].subfields.iter().map(|&s| self.t.syms[s].name.clone()).collect() };
        let (a, b) = (names(td), names(sd));
        a.iter().all(|n| b.contains(n)) && b.iter().all(|n| a.contains(n))
    }

    fn corr(&mut self, td: usize, tp: &str, sd: usize, sp: &str, half: bool) {
        let subs = self.t.ds[td].subfields.clone();
        for t in subs {
            let ts = self.t.syms[t].clone();
            let Some(s) = self.t.subfield(sd, &ts.name) else { continue };
            let ss = self.t.syms[s].clone();
            let tplace = format!("{tp}.{}", mangle(&ts.name));
            let splace = format!("{sp}.{}", mangle(&ss.name));
            if ts.dim != ss.dim {
                continue;
            }
            match (&ts.ty, &ss.ty) {
                (Ty::Ds(a), Ty::Ds(b)) => {
                    if ts.dim == 0 {
                        self.corr(*a, &tplace, *b, &splace, half);
                    }
                }
                (a, b) if a.is_numeric() == b.is_numeric() && a.is_string() == b.is_string() => {
                    let n = ts.dim.max(1);
                    for i in 0..n {
                        let (x, y) = if ts.dim > 0 { (format!("{tplace}[{i}]"), format!("{splace}[{i}]")) } else { (tplace.clone(), splace.clone()) };
                        self.assign(&V::place(ts.ty.clone(), x), &V::place(ss.ty.clone(), y), half);
                    }
                }
                _ => {}
            }
        }
    }

    /// A fixed-form numeric assignment: high-order digits dropped.
    pub fn assign_trunc(&mut self, t: &V, v: &V, half: bool) {
        if t.ty.is_numeric() && v.ty.is_numeric() {
            let h = if half { " halfadj" } else { "" };
            let s = self.dec_operand(v);
            self.b.ins(format!("dec.mov {}, {s}{h} trunc", t.op));
        } else {
            self.assign(t, v, half);
        }
    }

    /// The comparison of two values as a condition register.
    pub fn cmp_values(&mut self, op: BinOp, a: &V, b: &V) -> Option<String> {
        self.compare(op, a, b).map(|v| v.op)
    }
}

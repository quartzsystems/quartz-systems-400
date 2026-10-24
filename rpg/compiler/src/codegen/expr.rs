// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Expressions: each becomes an IR operand of a known RPG type: a
//! place, a literal or a register. Decimal arithmetic states its
//! intermediate precision from `qdecimal::arith::intermediate`, with
//! the assignment's decimal positions as the floor; integer operands
//! use integer arithmetic; a float operand makes the operation float.
//! `AND` and `OR` evaluate their right operand only when needed.

use qdecimal::arith::{self, Mode};
use qdecimal::Shape;
use rpgfront::Diag;
use rpgfront::ast::{BinOp, Expr, Fig, Lit, Step, UnOp};

use crate::diag::ids;
use crate::codegen::emit::quote;
use crate::codegen::{Gen, place_of};
use crate::syms::{ConstVal, Kind, SymId};
use crate::types::Ty;

/// How a value is held.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Vk {
    /// A place in storage.
    Place,
    /// A literal.
    Lit,
    /// A register.
    Reg,
}

/// A generated value.
#[derive(Clone, Debug)]
pub struct V {
    /// Its type; `Unknown` for a figurative constant.
    pub ty: Ty,
    /// The operand text.
    pub op: String,
    /// How it is held.
    pub kind: Vk,
    /// The figurative constant it is, if any.
    pub fig: Option<Fig>,
    /// Elements, for a whole array.
    pub dim: u32,
    /// The symbol it names, for references and assignment.
    pub sym: Option<SymId>,
}

impl V {
    fn new(ty: Ty, op: impl Into<String>, kind: Vk) -> Self {
        Self {
            ty,
            op: op.into(),
            kind,
            fig: None,
            dim: 0,
            sym: None,
        }
    }

    /// A place of a type.
    #[must_use]
    pub fn place(ty: Ty, op: impl Into<String>) -> Self {
        Self::new(ty, op, Vk::Place)
    }

    /// A register of a type.
    #[must_use]
    pub fn reg(ty: Ty, op: impl Into<String>) -> Self {
        Self::new(ty, op, Vk::Reg)
    }

    /// A literal of a type.
    #[must_use]
    pub fn lit(ty: Ty, op: impl Into<String>) -> Self {
        Self::new(ty, op, Vk::Lit)
    }
}

/// What the context wants from an expression.
#[derive(Clone, Copy, Debug, Default)]
pub struct Want {
    /// The decimal positions of the result field (the precision
    /// floor), half-adjust and `*RESDECPOS`.
    pub mode: Mode,
    /// An operand of a comparison: no result field, so decimals are
    /// lost against the operation's own scale.
    pub compared: bool,
}

/// The shape of a numeric literal as written.
#[must_use]
pub fn literal_shape(text: &str) -> (u8, u8) {
    let t = text.trim_start_matches(['-', '+']);
    let (i, f) = t.split_once(['.', ',']).unwrap_or((t, ""));
    let i = i.trim_start_matches('0');
    let digits = (i.len() + f.len()).max(1);
    let digits = u8::try_from(digits.min(63)).unwrap_or(63);
    let scale = u8::try_from(f.len().min(63)).unwrap_or(0);
    (digits.max(scale).max(1), scale)
}

/// An IR numeric literal from RPG text.
fn num_text(text: &str) -> String {
    let t = text.replace(',', ".");
    let t = if let Some(rest) = t.strip_prefix('+') { rest.to_string() } else { t };
    // `5.` is `5`.
    let t = t.strip_suffix('.').map_or(t.clone(), str::to_string);
    if t.starts_with('.') {
        format!("0{t}")
    } else if let Some(r) = t.strip_prefix("-.") {
        format!("-0.{r}")
    } else {
        t
    }
}

impl Gen<'_> {
    /// Evaluate an expression.
    pub fn expr(&mut self, e: &Expr, w: Want) -> Option<V> {
        match e {
            Expr::Lit(l, _) => self.literal(l),
            Expr::Fig(f, _) => Some(V {
                fig: Some(f.clone()),
                ..V::lit(
                    if matches!(f, Fig::On | Fig::Off) { Ty::Ind } else { Ty::Unknown },
                    fig_text(f),
                )
            }),
            Expr::Special { word, index, at } => {
                self.ref_at = *at;
                self.special(word, index.as_deref())
            }
            Expr::Ref { name, steps, at } => {
                self.ref_at = *at;
                let v = self.reference(name, steps, false)?;
                // FIXNBR(*ZONED): a zoned value's bad digits repaired.
                if self.ctl.fixnbr_zoned
                    && let Ty::Zoned(d, sc) = v.ty
                    && v.kind == Vk::Place
                    && v.dim == 0
                {
                    let t = self.tmp(&Ty::Zoned(d, sc));
                    self.rt_proc("_QRNX_FIXZONED", "($dst: bytes(1) *VARSIZE, $src: bytes(1) *VARSIZE, byval $n: i32)");
                    self.b.ins(format!("callb _QRNX_FIXZONED({}, {}, {d})", t.op, v.op));
                    return Some(V { sym: v.sym, ..t });
                }
                Some(v)
            }
            Expr::Bif { name, args, .. } => self.bif(name, args, w),
            Expr::Unary(op, a, _) => self.unary(*op, a, w),
            Expr::Binary(op, a, b, _) => self.binary(*op, a, b, w),
            Expr::List(..) | Expr::Assign(..) | Expr::Empty(_) => {
                self.err(ids::BAD_EXPRESSION_TYPE, 30, "");
                None
            }
        }
    }

    /// A literal as a value.
    pub fn literal(&mut self, l: &Lit) -> Option<V> {
        Some(match l {
            Lit::Num(n) => {
                if n.to_ascii_uppercase().contains('E') {
                    let f: f64 = n.parse().ok()?;
                    V::lit(Ty::Float(8), format!("{f:e}"))
                } else {
                    let (d, s) = literal_shape(n);
                    V::lit(Ty::Packed(d, s), num_text(n))
                }
            }
            Lit::Char(s) => {
                if !s.chars().all(ccsid::is_representable) {
                    self.err(ids::BAD_EXPRESSION_TYPE, 30, s.clone());
                }
                V::lit(Ty::Char(u32::try_from(s.chars().count()).unwrap_or(0)), quote(s))
            }
            Lit::Hex(h) => {
                let n = u32::try_from(h.len() / 2).unwrap_or(0);
                V::lit(Ty::Char(n), format!("x\"{}\"", h.to_ascii_uppercase()))
            }
            Lit::Date(_) | Lit::Time(_) | Lit::Timestamp(_) => return self.dt_literal(l),
            Lit::Ucs2(h) => return Some(self.ucs2_literal(h)),
            Lit::Graph(_) => {
                self.err(ids::UNSUPPORTED, 30, "literal");
                return None;
            }
        })
    }

    /// `*INxx`, `*IN(n)`, `*INLR` and the other special words.
    fn special(&mut self, word: &str, index: Option<&Expr>) -> Option<V> {
        if word == "*IN" {
            if let Some(i) = index {
                let r = self.index(i, 99)?;
                return Some(V::place(Ty::Ind, format!("$in.x[{r}]")));
            }
            return Some(V {
                dim: 99,
                ..V::place(Ty::Ind, "$in.x")
            });
        }
        if let Some(n) = word.strip_prefix("*IN") {
            if let Ok(k) = n.parse::<u32>()
                && (1..=99).contains(&k)
                && n.len() == 2
            {
                return Some(V::place(Ty::Ind, format!("$in.x[{}]", k - 1)));
            }
            let up = format!("IN{n}");
            if crate::codegen::SPECIAL_INDS.contains(&up.as_str()) {
                return Some(V::place(Ty::Ind, format!("${}", up.to_ascii_lowercase())));
            }
        }
        if word == "*DATE" {
            return Some(self.job_date(8));
        }
        if let Some(n) = crate::codegen::xml::word_value(word) {
            return self.literal(&Lit::Num(n.to_string()));
        }
        let at = self.ref_at;
        self.undefined_name(word, at);
        None
    }

    /// The job date as `*DATE` (`mmddyyyy`) or `UDATE` (`mmddyy`): QS/400
    /// jobs date in `*MDY` (`docs/RPG.md`).
    pub fn job_date(&mut self, digits: u8) -> V {
        let date = V::place(Ty::Zoned(digits, 0), self.b.temp(&format!("zoned({digits},0)")));
        // *USA has the four-digit year, *MDY the two-digit one.
        let code = if digits == 8 { 2 } else { 5 };
        self.dt_now_as(&date, code);
        date
    }

    /// An index expression as an IR index: a literal (checked against
    /// the element count) or a register, zero-based.
    pub fn index(&mut self, e: &Expr, dim: u32) -> Option<String> {
        if let Expr::Lit(Lit::Num(n), _) = e {
            let k: i64 = n.parse().ok()?;
            if k < 1 || k > i64::from(dim) {
                self.err(ids::BAD_INDEX, 20, n.clone());
                return None;
            }
            return Some((k - 1).to_string());
        }
        let v = self.expr(e, Want::default())?;
        let r = self.int_reg(&v)?;
        let z = self.b.reg();
        self.b.ins(format!("{z} = sub {r}, 1"));
        Some(z)
    }

    /// A numeric value as an `i64` register, truncating decimals.
    pub fn int_reg(&mut self, v: &V) -> Option<String> {
        if !v.ty.is_numeric() {
            self.err(ids::TYPE_MISMATCH, 30, "");
            return None;
        }
        let r = self.b.reg();
        match v.kind {
            Vk::Reg => match v.ty {
                Ty::Float(_) => self.b.ins(format!("{r} = cvt.i64.chk {}", v.op)),
                _ => self.b.ins(format!("{r} = cvt.i64 {}", v.op)),
            },
            Vk::Lit => {
                let n: i64 = v.op.split('.').next().unwrap_or("0").parse().unwrap_or(0);
                self.b.ins(format!("{r} = const.i64 {n}"));
            }
            Vk::Place => self.b.ins(format!("{r} = cvt.i64.chk {}", v.op)),
        }
        Some(r)
    }

    /// A name with its steps: a field, an element, a subfield, a
    /// constant, or a call.
    pub fn reference(&mut self, name: &str, steps: &[Step], modify: bool) -> Option<V> {
        let Some(id) = self.t.find(self.scope, name) else {
            if name == "UDATE" && steps.is_empty() && !modify {
                return Some(self.job_date(6));
            }
            let at = self.ref_at;
            self.undefined_name(name, at);
            return None;
        };
        self.t.refer(id, self.at.line, modify);
        let sym = self.t.syms[id].clone();
        match &sym.kind {
            Some(Kind::Const(ConstVal::Expr(e))) => {
                let e = e.clone();
                self.expr(&e, Want::default())
            }
            Some(Kind::Proto(_)) => self.call_expr(id, steps),
            Some(Kind::Field | Kind::Subfield | Kind::Param | Kind::Ds(_)) => {
                // A template has no storage.
                if sym.template || sym.parent.is_some_and(|p| self.t.syms[p].template) {
                    self.err(ids::TEMPLATE_USED, 30, name.to_string());
                    // And again for the subfield it qualifies.
                    if let Some(Step::Field(f)) = steps.first() {
                        self.err(ids::TEMPLATE_USED, 30, f.to_ascii_uppercase());
                    }
                    return None;
                }
                let mut place = place_of(self.t, id);
                let mut ty = sym.ty.clone();
                let mut dim = sym.dim;
                let mut cur = id;
                // A table takes no index: it is ignored.
                let steps = if sym.table && dim > 0 && matches!(steps.first(), Some(Step::Args(_))) {
                    self.err(ids::TABLE_INDEX, 20, name.to_string());
                    &steps[1..]
                } else {
                    steps
                };
                if sym.table && dim > 0 && steps.is_empty() {
                    place = self.table_elem(id, &place);
                    dim = 0;
                }
                let mut steps = steps.iter().peekable();
                // An element of an array (or of a structure array).
                if dim > 0
                    && let Some(Step::Args(a)) = steps.peek()
                {
                    let a = a.clone();
                    steps.next();
                    if a.len() != 1 {
                        self.err(ids::INDEX_RANGE, 30, name.to_string());
                        return None;
                    }
                    let i = self.elem_index(id, &a[0], dim, modify)?;
                    place = format!("{place}[{i}]");
                    dim = 0;
                }
                for st in steps {
                    match st {
                        Step::Field(f) => {
                            let Ty::Ds(di) = ty else {
                                self.err(ids::NOT_A_DS, 30, name.to_string());
                                return None;
                            };
                            // Only a qualified structure qualifies a name.
                            if cur == id && matches!(sym.kind, Some(Kind::Ds(_))) && (!self.t.ds[di].qualified || self.t.ds[di].unqualified_array) {
                                self.err(ids::BAD_QUALIFIED, 30, name.to_string());
                                return None;
                            }
                            let Some(sid) = self.t.subfield(di, f) else {
                                // Where the name is written, an undefined operand.
                                let at = self.ref_at;
                                self.undefined_name(f, at);
                                return None;
                            };
                            self.t.refer(sid, self.at.line, modify);
                            let s = self.t.syms[sid].clone();
                            place = format!("{place}.{}", crate::names::mangle(&s.name));
                            ty = s.ty;
                            dim = s.dim;
                            cur = sid;
                        }
                        Step::Args(a) => {
                            if dim == 0 {
                                self.err(ids::NOT_ARRAY, 20, name.to_string());
                                return None;
                            }
                            if a.len() != 1 {
                                self.err(ids::INDEX_RANGE, 30, name.to_string());
                                return None;
                            }
                            let i = self.index(&a[0], dim)?;
                            place = format!("{place}[{i}]");
                            dim = 0;
                        }
                    }
                }
                Some(V {
                    dim,
                    sym: Some(cur),
                    ..V::place(ty, place)
                })
            }
            _ => {
                self.err(ids::BAD_EXPRESSION_TYPE, 30, name.to_string());
                None
            }
        }
    }

    /// A structure as character data.
    #[must_use]
    pub fn as_chars(&self, v: &V) -> V {
        if let Ty::Ds(i) = v.ty {
            let len = self.t.ds_len(i);
            return V {
                ty: Ty::Char(len),
                op: format!("{}.all_", v.op),
                ..v.clone()
            };
        }
        v.clone()
    }

    fn unary(&mut self, op: UnOp, a: &Expr, w: Want) -> Option<V> {
        let v = self.expr(a, w)?;
        match op {
            UnOp::Plus => Some(v),
            UnOp::Not => {
                let r = self.ind_reg(&v)?;
                let n = self.b.reg();
                self.b.ins(format!("{n} = cmp.eq {r}, *OFF"));
                Some(V::reg(Ty::Ind, n))
            }
            UnOp::Neg => {
                if !v.ty.is_numeric() {
                    self.err(ids::TYPE_MISMATCH, 30, "-");
                    return None;
                }
                match (&v.ty, v.kind) {
                    (Ty::Float(_), _) => {
                        let f = self.float_reg(&v);
                        let n = self.b.reg();
                        self.b.ins(format!("{n} = neg {f}"));
                        Some(V::reg(Ty::Float(8), n))
                    }
                    (t, _) if t.is_integer() => {
                        let r = self.int_reg(&v)?;
                        let n = self.b.reg();
                        self.b.ins(format!("{n} = sub.chk 0, {r}"));
                        Some(V::reg(Ty::Int(20), n))
                    }
                    _ => {
                        let (d, s) = v.ty.shape()?;
                        let t = self.b.temp(&format!("packed({d},{s})"));
                        self.b.ins(format!("dec.neg {t}, {}", v.op));
                        Some(V::place(Ty::Packed(d, s), t))
                    }
                }
            }
        }
    }

    /// An indicator value as an `ind` register.
    pub fn ind_reg(&mut self, v: &V) -> Option<String> {
        match (&v.ty, v.kind) {
            (Ty::Ind, Vk::Reg) => Some(v.op.clone()),
            (Ty::Ind, Vk::Place) => {
                let r = self.b.reg();
                self.b.ins(format!("{r} = load {}", v.op));
                Some(r)
            }
            (Ty::Ind, Vk::Lit) => {
                let r = self.b.reg();
                self.b.ins(format!("{r} = const.ind {}", v.op));
                Some(r)
            }
            _ => {
                self.err(ids::TYPE_MISMATCH, 30, "indicator");
                None
            }
        }
    }

    /// A numeric value as an `f64` register.
    pub fn float_reg(&mut self, v: &V) -> String {
        let r = self.b.reg();
        match v.kind {
            Vk::Reg if matches!(v.ty, Ty::Float(8)) => return v.op.clone(),
            Vk::Reg => self.b.ins(format!("{r} = cvt.f64 {}", v.op)),
            Vk::Lit => {
                let f: f64 = v.op.parse().unwrap_or(0.0);
                self.b.ins(format!("{r} = const.f64 {f:e}"));
            }
            Vk::Place => self.b.ins(format!("{r} = cvt.f64 {}", v.op)),
        }
        r
    }

    /// The decimal shape of a numeric operand for precision.
    fn dec_shape(v: &V) -> (u8, u8) {
        match &v.ty {
            Ty::Int(d) | Ty::Uns(d) => (*d, 0),
            Ty::Float(_) => (31, 15),
            t => t.shape().unwrap_or((1, 0)),
        }
    }

    fn binary(&mut self, op: BinOp, a: &Expr, b: &Expr, w: Want) -> Option<V> {
        match op {
            BinOp::And | BinOp::Or => return self.logic(op, a, b),
            BinOp::In => return self.in_op(a, b),
            _ => {}
        }
        let relational = matches!(op, BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge);
        let w = if relational { Want { compared: true, ..w } } else { w };
        // Both operands are looked at, so each undefined name is told.
        let va = self.expr(a, w);
        let vb = self.expr(b, w);
        let (va, vb) = (va?, vb?);
        // An operand whose type could not be resolved: its own error
        // was given; the expression is reported as undefined.
        if [&va, &vb].iter().any(|v| v.ty == Ty::Unknown && v.kind == Vk::Place) {
            self.err(ids::UNDEFINED_OPERAND, 30, "");
            return None;
        }
        let before = self.diags.len();
        let r = match op {
            BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge => self.compare(op, &va, &vb),
            BinOp::Add
                if va.ty.is_string()
                    || vb.ty.is_string()
                    || matches!(va.ty, Ty::Ds(_))
                    || va.ty == Ty::Ind && vb.ty == Ty::Ind =>
            {
                self.concat(&va, &vb)
            }
            BinOp::Add | BinOp::Sub if va.ty == Ty::Ptr || vb.ty == Ty::Ptr => self.ptr_arith(op, &va, &vb),
            _ => {
                self.rhs_at = Some(b.pos());
                let r = self.arith(op, &va, &vb, w);
                self.rhs_at = None;
                r
            }
        };
        // Operands that do not match: IBM goes on with the left one's
        // type, so an assignment of the result is checked too.
        let arith = matches!(op, BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Pow);
        if r.is_none() && arith && self.diags[before..].iter().any(|d| d.msgid == ids::TYPE_MISMATCH) {
            return Some(va);
        }
        r
    }

    /// Pointer arithmetic: a pointer plus or minus a number of bytes,
    /// or the bytes between two pointers.
    pub fn ptr_arith(&mut self, op: BinOp, va: &V, vb: &V) -> Option<V> {
        if va.ty == Ty::Ptr && vb.ty == Ty::Ptr && op == BinOp::Sub {
            let (p, q) = (self.ptr_reg(va), self.ptr_reg(vb));
            let r = self.b.reg();
            self.b.ins(format!("{r} = ptrdiff {p}, {q}"));
            return Some(V::reg(Ty::Int(20), r));
        }
        let (p, n) = match (&va.ty, &vb.ty) {
            (Ty::Ptr, t) if t.is_numeric() => (va, vb),
            (t, Ty::Ptr) if t.is_numeric() && op == BinOp::Add => (vb, va),
            _ => {
                self.err(ids::TYPE_MISMATCH, 30, "");
                return None;
            }
        };
        let p = self.ptr_reg(p);
        let mut n = self.int_reg(n)?;
        if op == BinOp::Sub {
            let m = self.b.reg();
            self.b.ins(format!("{m} = sub 0, {n}"));
            n = m;
        }
        let r = self.b.reg();
        self.b.ins(format!("{r} = ptradd {p}, {n}"));
        Some(V::reg(Ty::Ptr, r))
    }

    /// Numeric arithmetic.
    pub fn arith(&mut self, op: BinOp, va: &V, vb: &V, w: Want) -> Option<V> {
        // An operand whose type could not be resolved: its own error
        // was given; the expression is reported as undefined.
        if [va, vb].iter().any(|v| v.ty == Ty::Unknown && v.kind == Vk::Place) {
            self.err(ids::UNDEFINED_OPERAND, 30, "");
            return None;
        }
        if va.ty.is_datetime() || vb.ty.is_datetime() || matches!(va.ty, Ty::Dur(_)) || matches!(vb.ty, Ty::Dur(_)) {
            return self.dt_arith(op, va, vb);
        }
        if !va.ty.is_numeric() || !vb.ty.is_numeric() {
            self.err(ids::TYPE_MISMATCH, 30, "");
            return None;
        }
        // A constant zero divisor is refused at compile time.
        if op == BinOp::Div && vb.kind == Vk::Lit && vb.op.trim_start_matches('-').chars().all(|c| c == '0' || c == '.') {
            self.err(ids::DIVIDE_BY_ZERO, 30, String::new());
            return None;
        }
        let float = matches!(va.ty, Ty::Float(_)) || matches!(vb.ty, Ty::Float(_)) || op == BinOp::Pow;
        // A float with a decimal or integer operand: done exactly, the
        // result the double toward zero from it, as IBM does it.
        let fa_only = matches!(va.ty, Ty::Float(_)) && !matches!(vb.ty, Ty::Float(_));
        let fb_only = matches!(vb.ty, Ty::Float(_)) && !matches!(va.ty, Ty::Float(_));
        if (fa_only || fb_only) && matches!(op, BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div) {
            let (f, d) = if fa_only { (va, vb) } else { (vb, va) };
            let fr = self.float_reg(f);
            let dp = self.num_place(d);
            let desc = self.desc_of(&dp);
            let code = match op {
                BinOp::Add => 0,
                BinOp::Sub => 1,
                BinOp::Mul => 2,
                _ => 3,
            };
            self.rt_proc(
                "_QRNX_FLTMIX",
                "(byval $op: i32, byval $f: f64, $d: bytes(1) *VARSIZE, byval $desc: i64, byval $first: i32) -> f64",
            );
            let r = self.b.reg();
            self.b.ins(format!("{r} = callb _QRNX_FLTMIX({code}, {fr}, {}, {desc}, {})", dp.op, i32::from(fa_only)));
            return Some(V::reg(Ty::Float(8), r));
        }
        if float {
            let fa = self.float_reg(va);
            let fb = self.float_reg(vb);
            let r = self.b.reg();
            let o = match op {
                BinOp::Add => "add",
                BinOp::Sub => "sub",
                BinOp::Mul => "mul",
                BinOp::Div => "div",
                _ => {
                    // `**` through the runtime library's power.
                    self.b.ins(format!(
                        "{r} = callb _QRNX_POW({fa}, {fb})"
                    ));
                    self.externs.insert(
                        "_QRNX_POW".into(),
                        "extern proc _QRNX_POW(byval $a: f64, byval $b: f64) -> f64".into(),
                    );
                    return Some(V::reg(Ty::Float(8), r));
                }
            };
            self.b.ins(format!("{r} = {o} {fa}, {fb}"));
            return Some(V::reg(Ty::Float(8), r));
        }
        // Unsigned arithmetic: unsigned operands, or one with a literal
        // integer that is not negative; a negative result is an error.
        let uns_like = |v: &V| {
            matches!(v.ty, Ty::Uns(_))
                || v.kind == Vk::Lit && matches!(v.ty, Ty::Packed(_, 0)) && !v.op.starts_with('-')
        };
        let unsigned = (matches!(va.ty, Ty::Uns(_)) || matches!(vb.ty, Ty::Uns(_))) && uns_like(va) && uns_like(vb);
        if unsigned && !self.signed_ops && matches!(op, BinOp::Add | BinOp::Sub | BinOp::Mul) {
            let ra = self.int_reg(va)?;
            let rb = self.int_reg(vb)?;
            let r = self.b.reg();
            let o = match op {
                BinOp::Add => "add.chk",
                BinOp::Sub => "sub.chk",
                _ => "mul.chk",
            };
            self.b.ins(format!("{r} = {o} {ra}, {rb}"));
            let c = self.b.reg();
            self.b.ins(format!("{c} = cvt.u64.chk {r}"));
            let wide = matches!(va.ty, Ty::Uns(20)) || matches!(vb.ty, Ty::Uns(20));
            return Some(V::reg(Ty::Uns(if wide { 20 } else { 10 }), r));
        }
        if va.ty.is_integer() && vb.ty.is_integer() && op != BinOp::Div {
            let ra = self.int_reg(va)?;
            let rb = self.int_reg(vb)?;
            let r = self.b.reg();
            let o = match op {
                BinOp::Add => "add.chk",
                BinOp::Sub => "sub.chk",
                _ => "mul.chk",
            };
            self.b.ins(format!("{r} = {o} {ra}, {rb}"));
            let wide = matches!(va.ty, Ty::Int(20) | Ty::Uns(20)) || matches!(vb.ty, Ty::Int(20) | Ty::Uns(20));
            return Some(V::reg(Ty::Int(if wide { 20 } else { 10 }), r));
        }
        // Decimal arithmetic.
        let (da, sa) = Self::dec_shape(va);
        let (db, sb) = Self::dec_shape(vb);
        let sha = Shape::new(da.max(1), sa).ok()?;
        let shb = Shape::new(db.max(1), sb).ok()?;
        let aop = match op {
            BinOp::Add => arith::Op::Add,
            BinOp::Sub => arith::Op::Sub,
            BinOp::Mul => arith::Op::Mul,
            _ => arith::Op::Div,
        };
        let mode = Mode {
            resdecpos: w.mode.resdecpos || self.ctl.resdecpos,
            ..w.mode
        };
        let Ok(sh) = arith::intermediate(aop, sha, shb, mode) else {
            self.err(ids::BAD_LENGTH, 30, "");
            return None;
        };
        // Decimals are lost when the limit (63 digits, or 30 when no
        // operand is longer) leaves fewer than both the target's and the
        // operation's own (a product's sum, a sum's larger); a compared
        // operand has no target.
        let natural = match op {
            BinOp::Mul => sa.saturating_add(sb),
            BinOp::Add | BinOp::Sub => sa.max(sb),
            _ => u8::MAX,
        };
        let floor = mode.result_scale + u8::from(mode.half_adjust);
        // Compared, only a quotient's decimals count as lost.
        let quotient = self.b.quotients.contains(&va.op) || self.b.quotients.contains(&vb.op);
        // At the 63-digit limit a quotient's decimals are lost when it
        // keeps fewer than the dividend or the target has (EXP126,
        // EXP074); under *RESDECPOS only the target counts (EXP094).
        let wanted = if op == BinOp::Div && sh.digits == 63 && !w.compared && !mode.resdecpos {
            sa.max(floor)
        } else if w.compared {
            if quotient && op != BinOp::Div { natural } else { 0 }
        } else {
            floor.min(natural)
        };
        if sh.scale < wanted && (sh.digits == 63 || sh.digits == 30) {
            // Reported at the right operand: its operator, or its
            // first token.
            let at = self.rhs_at.take().unwrap_or(self.at);
            self.diags.push(Diag::new(ids::DECIMALS_LOST, 10, at).arg(sh.scale.to_string()));
        }
        let t = self.b.temp(&format!("packed({},{})", sh.digits, sh.scale));
        if op == BinOp::Div || quotient {
            self.b.quotients.insert(t.clone());
        }
        let o = match op {
            BinOp::Add => "dec.add",
            BinOp::Sub => "dec.sub",
            BinOp::Mul => "dec.mul",
            _ => "dec.div",
        };
        let fa = self.dec_operand(va);
        let fb = self.dec_operand(vb);
        self.b.ins(format!(
            "{o} {t}, {fa}, {fb} prec({},{}){}",
            sh.digits,
            sh.scale,
            if mode.resdecpos { " resdecpos" } else { "" }
        ));
        Some(V::place(Ty::Packed(sh.digits, sh.scale), t))
    }

    /// A numeric operand as a decimal instruction takes it.
    pub fn dec_operand(&mut self, v: &V) -> String {
        v.op.clone()
    }

    /// An indicator as character data: one character, `'0'` or `'1'`.
    pub fn ind_as_char(&mut self, v: &V) -> V {
        if v.ty != Ty::Ind {
            return v.clone();
        }
        let t = self.b.temp("char(1)");
        match v.kind {
            // A blank byte (an indicator subfield of a structure never
            // set) stays blank.
            Vk::Place => {
                let p = self.b.reg();
                self.b.ins(format!("{p} = addr {}", v.op));
                let byte = self.b.reg();
                self.b.ins(format!("{byte} = load [{p} +0]:u8"));
                let blank = self.b.reg();
                self.b.ins(format!("{blank} = cmp.eq {byte}, 64"));
                let (yes, no, done) = (self.b.label(), self.b.label(), self.b.label());
                self.b.term(format!("brif {blank}, {yes}, {no}"));
                self.b.place(&yes);
                self.b.ins(format!("mov {t}, {}", quote(" ")));
                self.b.term(format!("br {done}"));
                self.b.place(&no);
                self.b.ins(format!("cvt {t}, {}", v.op));
                self.b.term(format!("br {done}"));
                self.b.place(&done);
            }
            Vk::Lit => self.b.ins(format!("mov {t}, {}", if v.op == "*ON" { quote("1") } else { quote("0") })),
            Vk::Reg => {
                let i = self.b.temp("ind");
                self.b.ins(format!("store {i}, {}", v.op));
                self.b.ins(format!("cvt {t}, {i}"));
            }
        }
        V::place(Ty::Char(1), t)
    }

    fn concat(&mut self, va: &V, vb: &V) -> Option<V> {
        let va = self.as_chars(va);
        let vb = self.as_chars(vb);
        let va = self.ind_as_char(&va);
        let vb = self.ind_as_char(&vb);
        // Two classes meet in the wider: UCS-2, graphic, character.
        let Some(cls) = Self::meet(&va, &vb) else {
            self.err(ids::TYPE_MISMATCH, 30, "+");
            return None;
        };
        let va = self.to_class(&va, cls)?;
        let vb = self.to_class(&vb, cls)?;
        let la = va.ty.str_len().unwrap_or(0);
        let lb = vb.ty.str_len().unwrap_or(0);
        let varying = va.ty.is_varying() || vb.ty.is_varying();
        let n = (la + lb).max(1);
        let ty = match (cls, varying) {
            (super::ucs2::TextClass::Ucs2, true) => Ty::VarUcs2(n, 2),
            (super::ucs2::TextClass::Ucs2, false) => Ty::Ucs2(n),
            (super::ucs2::TextClass::Graph, _) => Ty::Graph(n),
            (_, true) => Ty::VarChar(n, 2),
            (_, false) => Ty::Char(n),
        };
        let t = self.b.temp(&self.ir_ty(&ty));
        if la + lb == 0 {
            self.b.ins(format!("mov {t}, \"\""));
        } else if lb == 0 {
            // An empty literal adds nothing.
            self.b.ins(format!("mov {t}, {}", va.op));
        } else if la == 0 {
            self.b.ins(format!("mov {t}, {}", vb.op));
        } else {
            self.b.ins(format!("str.cat {t}, {}, {}", va.op, vb.op));
        }
        Some(V::place(ty, t))
    }

    /// A comparison, an `ind` register.
    pub fn compare(&mut self, op: BinOp, va: &V, vb: &V) -> Option<V> {
        let c = match op {
            BinOp::Eq => "eq",
            BinOp::Ne => "ne",
            BinOp::Lt => "lt",
            BinOp::Gt => "gt",
            BinOp::Le => "le",
            _ => "ge",
        };
        let va = self.as_chars(va);
        let vb = self.as_chars(vb);
        // A figurative constant takes the other operand's type.
        let (va, vb) = (self.fig_as(&va, &vb), self.fig_as(&vb, &va));
        let r = self.b.reg();
        let numeric = va.ty.is_numeric() || vb.ty.is_numeric();
        let fig_zero = |v: &V| matches!(v.fig, Some(Fig::Zeros | Fig::Hival | Fig::Loval));
        if numeric && (va.ty.is_numeric() || fig_zero(&va)) && (vb.ty.is_numeric() || fig_zero(&vb)) {
            if matches!(va.ty, Ty::Float(_)) || matches!(vb.ty, Ty::Float(_)) {
                let fa = self.float_reg(&va);
                let fb = self.float_reg(&vb);
                self.b.ins(format!("{r} = cmp.{c} {fa}, {fb}"));
            } else {
                let a = if va.kind == Vk::Lit && vb.kind == Vk::Lit {
                    // The IR compares a literal only against storage.
                    let (d, s) = va.ty.shape().unwrap_or((1, 0));
                    let t = self.b.temp(&format!("packed({d},{s})"));
                    self.b.ins(format!("dec.mov {t}, {}", va.op));
                    t
                } else {
                    self.dec_operand(&va)
                };
                let b = self.dec_operand(&vb);
                self.b.ins(format!("{r} = dec.cmp.{c} {a}, {b}"));
            }
            return Some(V::reg(Ty::Ind, r));
        }
        if va.ty == Ty::Ind || vb.ty == Ty::Ind {
            if !matches!(op, BinOp::Eq | BinOp::Ne) {
                let a = self.ind_char(&va);
                let b = self.ind_char(&vb);
                self.b.ins(format!("{r} = str.cmp.{c} {a}, {b}"));
                return Some(V::reg(Ty::Ind, r));
            }
            let a = self.ind_reg_or_lit(&va)?;
            let b = self.ind_reg_or_lit(&vb)?;
            self.b.ins(format!("{r} = cmp.{c} {a}, {b}"));
            return Some(V::reg(Ty::Ind, r));
        }
        if va.ty.is_datetime() || vb.ty.is_datetime() {
            // Dates, times and timestamps compare by value.
            return self.dt_compare(c, &va, &vb);
        }
        if (va.ty.is_string() || va.fig.is_some()) && (vb.ty.is_string() || vb.fig.is_some()) {
            // Two classes compare in the wider.
            let (va, vb) = match Self::meet(&va, &vb) {
                Some(cls) => (self.to_class(&va, cls)?, self.to_class(&vb, cls)?),
                None => (va.clone(), vb.clone()),
            };
            // The IR compares a literal only against storage.
            let a = if va.kind == Vk::Lit && vb.kind == Vk::Lit {
                let n = va.ty.str_len().unwrap_or(1).max(1);
                let t = self.b.temp(&format!("char({n})"));
                self.b.ins(format!("mov {t}, {}", va.op));
                t
            } else {
                va.op.clone()
            };
            self.b.ins(format!("{r} = str.cmp.{c} {a}, {}", vb.op));
            return Some(V::reg(Ty::Ind, r));
        }
        if matches!(va.ty, Ty::Ptr | Ty::ProcPtr | Ty::Object) || matches!(vb.ty, Ty::Ptr | Ty::ProcPtr | Ty::Object) {
            // *NULL takes the other side's kind of pointer; an object
            // reference compares as a pointer.
            let proc = va.ty == Ty::ProcPtr || vb.ty == Ty::ProcPtr;
            let a = self.ptr_reg_of(&va, proc);
            let b = self.ptr_reg_of(&vb, proc);
            self.b.ins(format!("{r} = cmp.{c} {a}, {b}"));
            return Some(V::reg(Ty::Ind, r));
        }
        self.err(ids::TYPE_MISMATCH, 30, "");
        None
    }

    /// A figurative constant as a temporary of `other`'s type, for a
    /// comparison; anything else as it is.
    fn fig_as(&mut self, v: &V, other: &V) -> V {
        let Some(f) = &v.fig else { return v.clone() };
        if other.fig.is_some() || !other.ty.is_numeric() {
            return v.clone();
        }
        if matches!(f, Fig::On | Fig::Off | Fig::Null) {
            return v.clone();
        }
        let ty = match &other.ty {
            Ty::Float(_) | Ty::Int(_) | Ty::Uns(_) | Ty::Bin(..) => other.ty.clone(),
            t if t.is_numeric() => t.clone(),
            t => t.clone(),
        };
        let t = self.b.temp(&self.ir_ty(&ty));
        self.b.ins(format!("mov {t}, {}", v.op));
        V::place(ty, t)
    }

    fn ind_reg_or_lit(&mut self, v: &V) -> Option<String> {
        if v.kind == Vk::Lit && matches!(v.fig, Some(Fig::On | Fig::Off)) {
            return Some(v.op.clone());
        }
        if v.kind == Vk::Lit {
            return Some(if v.op.contains('1') { "*ON".into() } else { "*OFF".into() });
        }
        self.ind_reg(v)
    }

    /// An indicator as one character, for ordering comparisons.
    fn ind_char(&mut self, v: &V) -> String {
        if v.kind == Vk::Place {
            let t = self.b.temp("char(1)");
            self.b.ins(format!("cvt {t}, {}", v.op));
            return t;
        }
        if v.kind == Vk::Lit {
            return if v.op == "*ON" { quote("1") } else if v.op == "*OFF" { quote("0") } else { v.op.clone() };
        }
        let t = self.b.temp("ind");
        self.b.ins(format!("store {t}, {}", v.op));
        let c = self.b.temp("char(1)");
        self.b.ins(format!("cvt {c}, {t}"));
        c
    }

    /// A pointer value as a register.
    pub fn ptr_reg(&mut self, v: &V) -> String {
        self.ptr_reg_of(v, v.ty == Ty::ProcPtr)
    }

    /// A pointer operand as a register; `*NULL` as a procedure pointer
    /// when `proc`.
    pub fn ptr_reg_of(&mut self, v: &V, proc: bool) -> String {
        match v.kind {
            Vk::Reg => v.op.clone(),
            Vk::Lit => {
                let r = self.b.reg();
                let k = if proc { "procptr" } else { "ptr" };
                self.b.ins(format!("{r} = const.{k} *NULL"));
                r
            }
            Vk::Place => {
                let r = self.b.reg();
                self.b.ins(format!("{r} = load {}", v.op));
                r
            }
        }
    }

    /// `AND` and `OR`: the right operand only when it decides.
    fn logic(&mut self, op: BinOp, a: &Expr, b: &Expr) -> Option<V> {
        let va = self.expr(a, Want::default())?;
        let ra = self.ind_reg(&va)?;
        let t = self.b.temp("ind");
        self.b.ins(format!("store {t}, {ra}"));
        let rhs = self.b.label();
        let done = self.b.label();
        if op == BinOp::And {
            self.b.term(format!("brif {ra}, {rhs}, {done}"));
        } else {
            self.b.term(format!("brif {ra}, {done}, {rhs}"));
        }
        self.b.place(&rhs);
        let vb = self.expr(b, Want::default())?;
        let rb = self.ind_reg(&vb)?;
        self.b.ins(format!("store {t}, {rb}"));
        self.b.place(&done);
        let r = self.b.reg();
        self.b.ins(format!("{r} = load {t}"));
        Some(V::reg(Ty::Ind, r))
    }
}

/// A figurative constant as an IR literal.
#[must_use]
pub fn fig_text(f: &Fig) -> String {
    match f {
        Fig::Blanks => "*BLANKS".into(),
        Fig::Zeros => "*ZEROS".into(),
        Fig::Hival => "*HIVAL".into(),
        Fig::Loval => "*LOVAL".into(),
        Fig::On => "*ON".into(),
        Fig::Off => "*OFF".into(),
        Fig::Null => "*NULL".into(),
        Fig::All(s) => quote(s),
        Fig::AllX(h) => format!("x\"{h}\""),
        Fig::AllG(s) => quote(s),
    }
}

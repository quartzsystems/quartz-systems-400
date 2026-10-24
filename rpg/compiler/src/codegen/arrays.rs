// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Arrays, tables and multiple-occurrence structures: `LOOKUP`,
//! `XFOOT`, `SORTA`, `OCCUR`, `CLEAR`, `RESET`, whole-array and
//! `%SUBARR` assignment, and the array built-in functions.
//!
//! A table's name alone is its current element, kept in a static index
//! (`$tix_NAME`) that `LOOKUP` and `%TLOOKUP` move. A multiple-occurrence
//! structure is its current occurrence in its own storage, with every
//! occurrence in a static array beside it (`$NAME__occ`) and the current
//! number (`$NAME__cur`): `OCCUR` saves the current one there and
//! fetches the next. `RESET` restores a copy (`$NAME__rst`) taken when
//! initialization (and `*INZSR`) is done, in `_qrni_snap`.

use rpgfront::ast::{AssignOp, Expr, Lit, Module, Op, Step};

use crate::codegen::Gen;
use crate::codegen::emit::{Body, quote};
use crate::codegen::expr::{V, Want};
use crate::codegen::place_of;
use crate::diag::ids;
use crate::names::mangle;
use crate::syms::{Kind, Storage, SymId};
use crate::types::Ty;

/// What a search wants: higher, lower, equal (`qrpgrt::arrays`).
const HI: i32 = 1;
const LO: i32 = 2;
const EQ: i32 = 4;

/// An array range: the place of its first element, the element type,
/// the stride, the first element (0-based register or literal) and the
/// count.
#[derive(Clone, Debug)]
pub struct Range {
    /// The whole array's place (element 0 is at its start).
    pub base: String,
    /// The array's symbol.
    pub sym: SymId,
    /// The element (or key) type.
    pub ty: Ty,
    /// Bytes from one element to the next.
    pub stride: u32,
    /// The key's offset in an element.
    pub keyoff: u32,
    /// The first element, 0-based.
    pub start: String,
    /// How many elements.
    pub count: String,
    /// Elements in the array.
    pub dim: u32,
}

impl Gen<'_> {
    /// A static of the module, declared once.
    fn ops_static(&mut self, name: &str, ty: &str) -> String {
        self.externs.entry(name.to_string()).or_insert_with(|| format!("static {name}: {ty}"));
        name.to_string()
    }

    /// A table's current-element index.
    pub fn tix(&mut self, id: SymId) -> String {
        self.ops.tables.insert(id);
        let n = format!("$tix_{}", mangle(&self.t.syms[id].name));
        self.ops_static(&n, "i64")
    }

    /// A table named alone: its current element.
    pub fn table_elem(&mut self, id: SymId, place: &str) -> String {
        let ix = self.tix(id);
        let r = self.b.reg();
        self.b.ins(format!("{r} = load {ix}"));
        format!("{place}[{r}]")
    }

    /// The symbol an array operand names.
    fn array_sym(&mut self, e: &Expr) -> Option<SymId> {
        let Expr::Ref { name, .. } = e else { return None };
        let id = self.t.find(self.scope, name)?;
        (self.t.syms[id].dim > 0).then_some(id)
    }

    /// An array operand: `ARR`, `ARR(i)` (from element i), a table,
    /// `%SUBARR(ARR:start:count)`, `DS(*).SUB` (by a subfield).
    pub fn range(&mut self, e: &Expr) -> Option<Range> {
        if let Expr::Bif { name, args, .. } = e
            && name == "%SUBARR"
        {
            let mut r = self.range(args.first()?)?;
            let s = self.expr(args.get(1)?, Want::default())?;
            let s = self.int_reg(&s)?;
            let z = self.b.reg();
            self.b.ins(format!("{z} = sub {s}, 1"));
            r.start.clone_from(&z);
            r.count = match args.get(2) {
                Some(c) => {
                    let c = self.expr(c, Want::default())?;
                    self.int_reg(&c)?
                }
                None => {
                    let c = self.b.reg();
                    self.b.ins(format!("{c} = sub {}, {z}", r.dim));
                    c
                }
            };
            return Some(r);
        }
        let Expr::Ref { name, steps, .. } = e else {
            self.err(ids::BAD_EXPRESSION_TYPE, 30, "");
            return None;
        };
        let Some(id) = self.t.find(self.scope, name) else {
            let at = e.pos();
            self.undefined_name(name, at);
            return None;
        };
        self.t.refer(id, self.at.line, false);
        let sym = self.t.syms[id].clone();
        // A qualified structure's array subfield: `DS.ARR`.
        if sym.dim == 0
            && let (Ty::Ds(di), [Step::Field(f)]) = (&sym.ty, steps.as_slice())
            && let Some(sub) = self.t.subfield(*di, f)
            && self.t.syms[sub].dim > 0
        {
            let s = self.t.syms[sub].clone();
            let size = self.t.size(&s.ty);
            return Some(Range {
                base: place_of(self.t, sub),
                sym: sub,
                ty: s.ty,
                stride: size,
                keyoff: 0,
                start: "0".into(),
                count: s.dim.to_string(),
                dim: s.dim,
            });
        }
        if sym.dim == 0 {
            self.err(ids::BAD_EXPRESSION_TYPE, 30, name.clone());
            return None;
        }
        let base = place_of(self.t, id);
        let size = self.t.size(&sym.ty);
        let mut r = Range {
            base,
            sym: id,
            ty: sym.ty.clone(),
            stride: size,
            keyoff: 0,
            start: "0".into(),
            count: sym.dim.to_string(),
            dim: sym.dim,
        };
        if sym.vardim > 0 {
            let cv = self.elem_var(id);
            let c = self.b.reg();
            self.b.ins(format!("{c} = load {cv}"));
            r.count = c;
        }
        match steps.as_slice() {
            [] => {}
            [Step::Args(a)] if a.len() == 1 => {
                let z = self.index(&a[0], sym.dim)?;
                let c = self.remaining(sym.dim, &z);
                r.start = z;
                r.count = c;
            }
            [Step::Args(a), Step::Field(f)] if matches!(a.as_slice(), [Expr::Special { word, .. }] if word == "*") => {
                let Ty::Ds(di) = sym.ty else {
                    self.err(ids::NOT_A_DS, 30, name.clone());
                    return None;
                };
                let Some(sub) = self.t.subfield(di, f) else {
                    self.err(ids::NOT_DEFINED, 30, f.clone());
                    return None;
                };
                let s = self.t.syms[sub].clone();
                r.ty = s.ty;
                r.keyoff = s.offset;
            }
            _ => {
                self.err(ids::BAD_EXPRESSION_TYPE, 30, name.clone());
                return None;
            }
        }
        Some(r)
    }

    /// Search a range; the `i64` the runtime returns (index and match).
    pub fn search(&mut self, arg: &V, r: &Range, want: i32) -> Option<String> {
        let tmp = self.b.temp(&self.ir_ty(&r.ty));
        let tv = V::place(r.ty.clone(), tmp.clone());
        self.assign(&tv, arg, false);
        let ed = self.desc_of(&tv);
        self.rt_proc(
            "_QRNX_LOOKUP",
            "($arg: bytes(1) *VARSIZE, $arr: bytes(1) *VARSIZE, byval $ed: i64, byval $st: i64, byval $n: i64, byval $want: i32, byval $stride: i64, byval $off: i64) -> i64",
        );
        let res = self.b.reg();
        self.b.ins(format!(
            "{res} = callb _QRNX_LOOKUP({tmp}, {}, {ed}, {}, {}, {want}, {}, {})",
            r.base, r.start, r.count, r.stride, r.keyoff
        ));
        Some(res)
    }

    /// `LOOKUP`: factor 1 the argument, factor 2 the array (from an
    /// index) or table, the result an alternate table; the high, low and
    /// equal indicators say what to look for and what was found.
    pub fn lookup_op(&mut self, op: &Op) {
        let (Some(f1), Some(f2)) = (op.f1.first().cloned(), op.f2.first().cloned()) else { return };
        let Some(arg) = self.expr(&f1, Want::default()) else { return };
        // Factor 2 is an array or a table.
        if let Expr::Ref { name, .. } = &f2
            && let Some(id) = self.t.find(self.scope, name)
            && self.t.syms[id].dim == 0
        {
            self.t.refer(id, self.at.line, false);
            self.err(ids::LOOKUP_NOT_ARRAY, 30, name.clone());
            return;
        }
        let Some(r) = self.range(&f2) else { return };
        // The argument has the elements' type and length.
        if arg.fig.is_none() && !same_type_and_length(&arg.ty, &r.ty) {
            self.err(ids::LOOKUP_ARG_TYPE, 30, String::new());
            return;
        }
        let mut want = 0;
        for (i, w) in [HI, LO, EQ].iter().enumerate() {
            if !op.resind[i].is_empty() {
                want |= w;
            }
        }
        let Some(res) = self.search(&arg, &r, want) else { return };
        let kind = self.b.reg();
        self.b.ins(format!("{kind} = shr {res}, 32"));
        let idx = self.b.reg();
        self.b.ins(format!("{idx} = and {res}, 4294967295"));
        let found = self.b.reg();
        self.b.ins(format!("{found} = cmp.ne {res}, 0"));
        for (i, w) in [HI, LO, EQ].iter().enumerate() {
            let ind = op.resind[i].clone();
            if ind.is_empty() {
                continue;
            }
            let c = self.b.reg();
            self.b.ins(format!("{c} = cmp.eq {kind}, {w}"));
            // The same indicator in two columns is on for either.
            let mut acc = c;
            for (j, w2) in [HI, LO, EQ].iter().enumerate() {
                if j != i && op.resind[j] == ind {
                    let c2 = self.b.reg();
                    self.b.ins(format!("{c2} = cmp.eq {kind}, {w2}"));
                    let o = self.b.reg();
                    self.b.ins(format!("{o} = select {acc}, {acc}, {c2}"));
                    acc = o;
                }
            }
            self.set_ind(&ind, &acc);
        }
        // The index: the element found, else 1.
        if let Expr::Ref { steps, .. } = &f2
            && let [Step::Args(a)] = steps.as_slice()
            && matches!(a.first(), Some(Expr::Ref { .. }))
            && let Some(t) = self.target(&a[0])
        {
            let one = self.b.reg();
            self.b.ins(format!("{one} = select {found}, {idx}, 1"));
            self.b.ins(format!("dec.mov {}, {one} trunc", t.op));
        }
        // Tables: the current elements move to the one found.
        let tables: Vec<SymId> = std::iter::once(r.sym)
            .chain(op.result.first().and_then(|e| self.array_sym(e)))
            .filter(|&id| self.t.syms[id].table)
            .collect();
        if !tables.is_empty() {
            let set = self.b.label();
            let done = self.b.label();
            self.b.term(format!("brif {found}, {set}, {done}"));
            self.b.place(&set);
            let z = self.b.reg();
            self.b.ins(format!("{z} = sub {idx}, 1"));
            for id in tables {
                let ix = self.tix(id);
                self.b.ins(format!("store {ix}, {z}"));
            }
            self.b.place(&done);
        }
    }

    /// `XFOOT array result`: the sum of the elements.
    pub fn xfoot_op(&mut self, op: &Op) {
        let (Some(f2), Some(res)) = (op.f2.first().cloned(), op.result.first().cloned()) else { return };
        let undefined = |g: &Self, e: &Expr| e.name().is_some_and(|n| g.t.find(g.scope, n).is_none());
        let v = self.expr(&f2, Want::default());
        // Factor 2 is a numeric array; the result a number.
        match &v {
            Some(x) if x.dim > 0 && !x.ty.is_numeric() => {
                self.err(ids::BAD_EXPRESSION_TYPE, 30, "XFOOT");
                return;
            }
            Some(x) if x.dim > 0 => {}
            None if !undefined(self, &f2) => return,
            _ => self.err(ids::SORTA_ARRAY, 30, "XFOOT"),
        }
        if undefined(self, &res) {
            let _ = self.target(&res);
            self.err(ids::NOT_NUMERIC_RESULT, 30, res.name().unwrap_or_default().to_string());
            return;
        }
        let Some(v) = v.filter(|x| x.dim > 0) else { return };
        let Some(sum) = self.sum_of(&v) else { return };
        let Some(t) = self.target(&res) else { return };
        self.assign_trunc(&t, &sum, op.ext.contains('H'));
        self.sign_inds(op, &t);
    }

    /// The sum of a numeric array, exact.
    pub fn sum_of(&mut self, v: &V) -> Option<V> {
        let s = match &v.ty {
            Ty::Float(_) => 15,
            t => t.shape().map_or(0, |x| x.1),
        };
        let acc = self.b.temp(&format!("packed(63,{s})"));
        self.b.ins(format!("mov {acc}, *ZEROS"));
        for i in 0..v.dim {
            let e = V::place(v.ty.clone(), format!("{}[{i}]", v.op));
            let o = self.dec_operand(&e);
            self.b.ins(format!("dec.add {acc}, {acc}, {o} prec(63,{s})"));
        }
        Some(V::place(Ty::Packed(63, s), acc))
    }

    /// `SORTA`: an array, a `%SUBARR` range or a structure array by a
    /// subfield; descending for `(D)` or a `DESCEND` array.
    pub fn sorta_op(&mut self, op: &Op) {
        let Some(e) = op.xf2.clone().or_else(|| op.f2.first().cloned()) else { return };
        if let Some(id) = e.name().and_then(|n| self.t.find(self.scope, n))
            && self.t.syms[id].dim == 0
        {
            self.err(ids::SORTA_ARRAY, 30, e.name().unwrap_or_default().to_string());
            return;
        }
        let Some(r) = self.range(&e) else { return };
        let seq = self.t.syms[r.sym].seq;
        let desc = op.ext.contains('D') || seq == 2 && !op.ext.contains('A');
        let kd = self.desc_of(&V::place(r.ty.clone(), String::new()));
        self.rt_proc(
            "_QRNX_SORTA",
            "($base: bytes(1) *VARSIZE, byval $st: i64, byval $n: i64, byval $size: i64, byval $off: i64, byval $kd: i64, byval $desc: i32)",
        );
        self.b.ins(format!(
            "callb _QRNX_SORTA({}, {}, {}, {}, {}, {kd}, {})",
            r.base,
            r.start,
            r.count,
            r.stride,
            r.keyoff,
            i32::from(desc)
        ));
    }

    /// The statics of a multiple-occurrence structure: every
    /// occurrence and the current one's number.
    fn occ_vars(&mut self, id: SymId) -> (String, String, u32) {
        self.ops.occurs.insert(id);
        let s = self.t.syms[id].clone();
        let Some(Kind::Ds(di)) = s.kind else { return (String::new(), String::new(), 0) };
        let n = self.t.ds[di].occurs.max(1);
        let ty = self.t.ds[di].ir_type.clone();
        let occ = self.ops_static(&format!("{}__occ", s.ir), &format!("array[{n}] of {ty}"));
        let cur = self.ops_static(&format!("{}__cur", s.ir), "i64");
        (occ, cur, n)
    }

    /// The multiple-occurrence structure an operand names.
    fn mods(&mut self, e: &Expr) -> Option<SymId> {
        let Expr::Ref { name, steps, .. } = e else { return None };
        if !steps.is_empty() {
            return None;
        }
        let id = self.t.find(self.scope, name)?;
        match self.t.syms[id].kind {
            Some(Kind::Ds(di)) if self.t.ds[di].occurs > 0 => Some(id),
            _ => None,
        }
    }

    /// Make occurrence `z` (0-based register) current; checked against
    /// the count, the error indicator set when given.
    fn set_occurrence(&mut self, id: SymId, z: &str, err_ind: &str) {
        let (occ, cur, n) = self.occ_vars(id);
        let place = place_of(self.t, id);
        let lo = self.b.reg();
        self.b.ins(format!("{lo} = cmp.ge {z}, 0"));
        let hi = self.b.reg();
        self.b.ins(format!("{hi} = cmp.lt {z}, {n}"));
        let ok = self.b.reg();
        self.b.ins(format!("{ok} = select {lo}, {hi}, {lo}"));
        let go = self.b.label();
        let bad = self.b.label();
        let done = self.b.label();
        self.b.term(format!("brif {ok}, {go}, {bad}"));
        self.b.place(&go);
        let c = self.b.reg();
        self.b.ins(format!("{c} = load {cur}"));
        self.b.ins(format!("mov {occ}[{c}], {place}"));
        self.b.ins(format!("mov {place}, {occ}[{z}]"));
        self.b.ins(format!("store {cur}, {z}"));
        if !err_ind.is_empty() {
            self.set_ind(err_ind, "*OFF");
        }
        self.b.term(format!("br {done}"));
        self.b.place(&bad);
        if !err_ind.is_empty() {
            self.set_ind(err_ind, "*ON");
        }
        self.b.place(&done);
    }

    /// `OCCUR`: factor 1 the occurrence to make current, factor 2 the
    /// structure, the result the current occurrence's number.
    pub fn occur_op(&mut self, op: &Op) {
        let Some(f2) = op.f2.first().cloned() else { return };
        let Some(id) = self.mods(&f2) else {
            self.err(ids::NOT_A_DS, 30, "OCCUR");
            return;
        };
        self.t.refer(id, self.at.line, true);
        if let Some(f1) = op.f1.first().cloned() {
            let v = if let Some(other) = self.mods(&f1) {
                let (_, cur, _) = self.occ_vars(other);
                let r = self.b.reg();
                self.b.ins(format!("{r} = load {cur}"));
                let one = self.b.reg();
                self.b.ins(format!("{one} = add {r}, 1"));
                one
            } else {
                let Some(v) = self.expr(&f1, Want::default()) else { return };
                let Some(r) = self.int_reg(&v) else { return };
                r
            };
            let z = self.b.reg();
            self.b.ins(format!("{z} = sub {v}, 1"));
            let err = op.resind[1].clone();
            self.set_occurrence(id, &z, &err);
        }
        if let Some(r) = op.result.first().cloned()
            && let Some(t) = self.target(&r)
        {
            let (_, cur, _) = self.occ_vars(id);
            let c = self.b.reg();
            self.b.ins(format!("{c} = load {cur}"));
            let one = self.b.reg();
            self.b.ins(format!("{one} = add {c}, 1"));
            self.b.ins(format!("dec.mov {}, {one} trunc", t.op));
        }
    }

    /// `%OCCUR(ds)`: the current occurrence.
    pub fn occur_bif(&mut self, args: &[Expr]) -> Option<V> {
        let id = self.mods(args.first()?)?;
        let (_, cur, _) = self.occ_vars(id);
        let c = self.b.reg();
        self.b.ins(format!("{c} = load {cur}"));
        let one = self.b.reg();
        self.b.ins(format!("{one} = add {c}, 1"));
        let r = self.b.reg();
        self.b.ins(format!("{r} = cvt.i32 {one}"));
        Some(V::reg(Ty::Int(10), r))
    }

    /// `CLEAR` and `RESET`.
    pub fn clear_reset(&mut self, reset: bool, op: &Op) {
        let Some(e) = op.result.first().cloned() else { return };
        let all = op.f2.iter().any(|x| matches!(x, Expr::Special { word, .. } if word == "*ALL"));
        if let Expr::Special { word, index: None, .. } = &e
            && word == "*IN"
        {
            self.b.ins("mov $in.all_, *ZEROS");
            return;
        }
        let Some(t) = self.target(&e) else { return };
        let id = t.sym;
        let whole = id.is_some_and(|i| place_of(self.t, i) == t.op);
        let global = id.is_some_and(|i| {
            let s = &self.t.syms[i];
            let root = s.parent.unwrap_or(i);
            self.t.syms[root].scope == 0 && matches!(self.t.syms[root].storage, Storage::Static | Storage::Export)
        });
        let mods = if whole { id.filter(|&i| matches!(self.t.syms[i].kind, Some(Kind::Ds(di)) if self.t.ds[di].occurs > 0)) } else { None };
        if reset {
            let Some(i) = id else { return };
            if !global {
                // Automatic storage: its initial value again.
                let root = self.t.syms[i].parent.unwrap_or(i);
                crate::codegen::stmt::init_symbol(self, root);
                return;
            }
            let root = self.t.syms[i].parent.unwrap_or(i);
            let rst = self.reset_copy(root);
            let rplace = place_of(self.t, root);
            let src = format!("{rst}{}", &t.op[rplace.len()..]);
            if let Some(m) = mods {
                let (occ, cur, _) = self.occ_vars(m);
                let orst = format!("{}__orst", self.t.syms[m].ir);
                let c = self.b.reg();
                self.b.ins(format!("{c} = load {cur}"));
                if all {
                    self.b.ins(format!("mov {occ}, {orst}"));
                }
                self.b.ins(format!("mov {}, {orst}[{c}]", t.op));
                return;
            }
            self.b.ins(format!("mov {}, {src}", t.op));
            return;
        }
        self.clear_place(&t);
        if let (Some(m), true) = (mods, all) {
            let (occ, _, n) = self.occ_vars(m);
            for k in 0..n {
                self.b.ins(format!("mov {occ}[{k}], {}", t.op));
            }
        }
    }

    /// The copy `RESET` restores a global from.
    fn reset_copy(&mut self, id: SymId) -> String {
        self.ops.resets.insert(id);
        let s = self.t.syms[id].clone();
        let ty = self.t.ir_storage(&s);
        if let Some(Kind::Ds(di)) = s.kind
            && self.t.ds[di].occurs > 0
        {
            let (_, _, n) = self.occ_vars(id);
            let dty = self.t.ds[di].ir_type.clone();
            self.ops_static(&format!("{}__orst", s.ir), &format!("array[{n}] of {dty}"));
        }
        self.ops_static(&format!("{}__rst", s.ir), &ty)
    }

    /// Every element or subfield of a place to its default.
    fn clear_place(&mut self, t: &V) {
        let n = t.dim;
        let places: Vec<String> = if n > 0 { (0..n).map(|i| format!("{}[{i}]", t.op)).collect() } else { vec![t.op.clone()] };
        for p in places {
            match &t.ty {
                Ty::Ds(di) => {
                    let subs = self.t.ds[*di].subfields.clone();
                    for s in subs {
                        let sym = self.t.syms[s].clone();
                        if sym.name.is_empty() || matches!(sym.ty, Ty::Unknown) {
                            continue;
                        }
                        let sp = format!("{p}.{}", mangle(&sym.name));
                        self.clear_place(&V { dim: sym.dim, ..V::place(sym.ty.clone(), sp) });
                    }
                }
                Ty::Ptr | Ty::ProcPtr | Ty::Object => {
                    let r = self.b.reg();
                    let k = if t.ty == Ty::ProcPtr { "procptr" } else { "ptr" };
                    self.b.ins(format!("{r} = const.{k} *NULL"));
                    self.b.ins(format!("store {p}, {r}"));
                }
                Ty::Ind => self.b.ins(format!("store {p}, *OFF")),
                ty if ty.is_numeric() => self.b.ins(format!("mov {p}, *ZEROS")),
                ty if ty.is_datetime() => {
                    let v = Self::dt_default(ty).unwrap_or_default();
                    self.b.ins(format!("mov {p}, {v}"));
                }
                ty if ty.is_varying() => self.b.ins(format!("mov {p}, \"\"")),
                _ => self.b.ins(format!("mov {p}, *BLANKS")),
            }
        }
    }

    /// An assignment to a whole array, `%SUBARR` or `%OCCUR`; false when
    /// the target is none of them.
    pub fn eval_hook(&mut self, aop: AssignOp, t: &Expr, v: &Expr, half: bool) -> bool {
        if let Expr::Bif { name, args, .. } = t {
            if name == "%ELEM" {
                let _ = self.elem_assign(args, v);
                return true;
            }
            if name == "%STR" {
                let _ = self.str_assign(args, v);
                return true;
            }
            if name == "%OCCUR" {
                let Some(id) = args.first().and_then(|a| self.mods(a)) else { return false };
                let Some(n) = self.expr(v, Want::default()) else { return true };
                let Some(r) = self.int_reg(&n) else { return true };
                let z = self.b.reg();
                self.b.ins(format!("{z} = sub {r}, 1"));
                self.set_occurrence(id, &z, "");
                return true;
            }
            if name != "%SUBARR" {
                return false;
            }
        } else {
            let whole = match t {
                Expr::Ref { name, steps, .. } if steps.is_empty() => {
                    self.t.find(self.scope, name).is_some_and(|id| self.t.syms[id].dim > 0 && !self.t.syms[id].table)
                }
                _ => false,
            };
            if !whole {
                return false;
            }
        }
        if aop != AssignOp::Set {
            self.err(ids::UNSUPPORTED, 30, "array");
            return true;
        }
        let Some(dst) = self.range(t) else { return true };
        if let Some(id) = self.array_sym(t).or(Some(dst.sym)) {
            self.t.refer(id, self.at.line, true);
        }
        let src = match v {
            Expr::Bif { name, .. } if name == "%SUBARR" => self.range(v),
            Expr::Bif { name, args, .. } if name == "%SPLIT" => self.split_range(args),
            Expr::Ref { name, steps, .. } if steps.is_empty() && self.t.find(self.scope, name).is_some_and(|id| self.t.syms[id].dim > 0) => self.range(v),
            _ => None,
        };
        let w = Want {
            mode: qdecimal::Mode {
                result_scale: dst.ty.shape().map_or(0, |(_, s)| s),
                half_adjust: half,
                resdecpos: self.ctl.resdecpos,
            },
            compared: false,
        };
        // The element count: the shorter range for an array source.
        let n = match &src {
            Some(s) if s.count.parse::<u32>().is_ok() && dst.count.parse::<u32>().is_ok() => {
                let (a, b): (u32, u32) = (s.count.parse().unwrap_or(0), dst.count.parse().unwrap_or(0));
                a.min(b).to_string()
            }
            Some(s) => {
                let c = self.b.reg();
                self.b.ins(format!("{c} = cmp.lt {}, {}", s.count, dst.count));
                let m = self.b.reg();
                self.b.ins(format!("{m} = select {c}, {}, {}", s.count, dst.count));
                m
            }
            None => dst.count.clone(),
        };
        // An expression over whole arrays is evaluated per element.
        let mut arrays = Vec::new();
        self.whole_arrays(v, &mut arrays);
        if src.is_none() && !arrays.is_empty() {
            self.elementwise(&dst, v, &arrays, w, half);
            return true;
        }
        let value = if src.is_none() {
            let Some(val) = self.expr(v, w) else { return true };
            // Evaluated once, into a temporary of the element type.
            if val.fig.is_some() {
                Some(val)
            } else {
                let tmp = self.b.temp(&self.ir_ty(&dst.ty));
                let tv = V::place(dst.ty.clone(), tmp);
                self.assign(&tv, &val, half);
                Some(tv)
            }
        } else {
            None
        };
        let i = self.b.temp("i64");
        self.b.ins(format!("store {i}, 0"));
        let top = self.b.label();
        let body = self.b.label();
        let end = self.b.label();
        self.b.place(&top);
        let k = self.b.reg();
        self.b.ins(format!("{k} = load {i}"));
        let c = self.b.reg();
        self.b.ins(format!("{c} = cmp.lt {k}, {n}"));
        self.b.term(format!("brif {c}, {body}, {end}"));
        self.b.place(&body);
        let de = self.b.reg();
        self.b.ins(format!("{de} = add {}, {k}", dst.start));
        let dplace = V::place(dst.ty.clone(), format!("{}[{de}]", dst.base));
        match (&src, &value) {
            (Some(s), _) => {
                let se = self.b.reg();
                self.b.ins(format!("{se} = add {}, {k}", s.start));
                let sv = V::place(s.ty.clone(), format!("{}[{se}]", s.base));
                self.assign(&dplace, &sv, half);
            }
            (None, Some(val)) => self.assign(&dplace, val, half),
            _ => {}
        }
        let k1 = self.b.reg();
        self.b.ins(format!("{k1} = add {k}, 1"));
        self.b.ins(format!("store {i}, {k1}"));
        self.b.term(format!("br {top}"));
        self.b.place(&end);
        true
    }

    /// The whole arrays an expression names (bare names of arrays that
    /// are not tables), each once.
    fn whole_arrays(&self, e: &Expr, out: &mut Vec<SymId>) {
        match e {
            Expr::Ref { name, steps, .. } if steps.is_empty() => {
                if let Some(id) = self.t.find(self.scope, name)
                    && self.t.syms[id].dim > 0
                    && !self.t.syms[id].table
                    && matches!(self.t.syms[id].kind, Some(Kind::Field | Kind::Subfield))
                    && !out.contains(&id)
                {
                    out.push(id);
                }
            }
            Expr::Unary(_, a, _) => self.whole_arrays(a, out),
            Expr::Binary(_, a, b, _) => {
                self.whole_arrays(a, out);
                self.whole_arrays(b, out);
            }
            Expr::Bif { name, args, .. } if !matches!(name.as_str(), "%ELEM" | "%SIZE" | "%ADDR" | "%XFOOT" | "%LOOKUP" | "%SUBARR") => {
                for a in args {
                    self.whole_arrays(a, out);
                }
            }
            _ => {}
        }
    }

    /// `dst = expr` over arrays: element `k` of each array in `expr`
    /// gives element `k` of the target, for as many elements as the
    /// shortest has.
    fn elementwise(&mut self, dst: &Range, v: &Expr, arrays: &[SymId], w: Want, half: bool) {
        // The element number the expression indexes by.
        let idx = self.b.temp(&self.ir_ty(&Ty::Int(20)));
        let name = format!("_AX{}", self.t.syms.len());
        let _ = self.t.add(crate::syms::Sym {
            name: name.clone(),
            kind: Some(Kind::Field),
            ty: Ty::Int(20),
            ir: idx.clone(),
            scope: self.scope,
            storage: crate::syms::Storage::Auto,
            ..crate::syms::Sym::default()
        });
        let names: Vec<String> = arrays.iter().map(|&id| self.t.syms[id].name.clone()).collect();
        let e = index_arrays(v, &names, &name);
        let mut n = dst.count.clone();
        for &id in arrays {
            let d = self.t.syms[id].dim;
            n = match n.parse::<u32>() {
                Ok(m) => m.min(d).to_string(),
                Err(_) => {
                    let c = self.b.reg();
                    self.b.ins(format!("{c} = cmp.lt {n}, {d}"));
                    let m = self.b.reg();
                    self.b.ins(format!("{m} = select {c}, {n}, {d}"));
                    m
                }
            };
        }
        let i = self.b.temp("i64");
        self.b.ins(format!("store {i}, 0"));
        let top = self.b.label();
        let body = self.b.label();
        let end = self.b.label();
        self.b.place(&top);
        let k = self.b.reg();
        self.b.ins(format!("{k} = load {i}"));
        let c = self.b.reg();
        self.b.ins(format!("{c} = cmp.lt {k}, {n}"));
        self.b.term(format!("brif {c}, {body}, {end}"));
        self.b.place(&body);
        let k1 = self.b.reg();
        self.b.ins(format!("{k1} = add {k}, 1"));
        self.b.ins(format!("store {idx}, {k1}"));
        let de = self.b.reg();
        self.b.ins(format!("{de} = add {}, {k}", dst.start));
        let dplace = V::place(dst.ty.clone(), format!("{}[{de}]", dst.base));
        if let Some(val) = self.expr(&e, w) {
            self.assign(&dplace, &val, half);
        }
        self.b.ins(format!("store {i}, {k1}"));
        self.b.term(format!("br {top}"));
        self.b.place(&end);
    }

    /// `%SPLIT(string [: separators])` into a temporary array of
    /// varying pieces.
    pub fn split_range(&mut self, args: &[Expr]) -> Option<Range> {
        let s = self.expr(args.first()?, Want::default())?;
        let s = self.as_chars(&s);
        let len = s.ty.str_len().unwrap_or(1).max(1);
        let src = self.rt_of(&s, None);
        let seps = match args.get(1) {
            Some(e) => {
                let v = self.expr(e, Want::default())?;
                self.rt_of(&v, None)
            }
            None => {
                let v = V::lit(Ty::Char(1), quote(" "));
                self.rt_of(&v, None)
            }
        };
        let cap = (len / 2 + 1).min(4096);
        let arr = self.b.temp(&format!("array[{cap}] of varchar({len})"));
        self.rt_proc(
            "_QRNX_SPLIT",
            "($d: bytes(1) *VARSIZE, byval $el: i64, byval $cap: i64, $s: bytes(1) *VARSIZE, byval $sd: i64, $p: bytes(1) *VARSIZE, byval $pd: i64) -> i64",
        );
        let n = self.b.reg();
        self.b.ins(format!("{n} = callb _QRNX_SPLIT({arr}, {len}, {cap}, {}, {}, {}, {})", src.op, src.desc, seps.op, seps.desc));
        let cnt = self.b.temp("i64");
        self.b.ins(format!("store {cnt}, {n}"));
        let c = self.b.reg();
        self.b.ins(format!("{c} = load {cnt}"));
        Some(Range {
            base: arr,
            sym: usize::MAX,
            ty: Ty::VarChar(len, 2),
            stride: len + 2,
            keyoff: 0,
            start: "0".into(),
            count: c,
            dim: cap,
        })
    }

    /// `FOR-EACH item IN array | %SUBARR | %SPLIT`, ended by `ENDFOR`.
    pub fn for_each(&mut self, op: &Op) {
        let Some(Expr::Binary(rpgfront::ast::BinOp::In, item, coll, _)) = op.f2.first().cloned() else {
            self.err(ids::BAD_EXPRESSION_TYPE, 30, "FOR-EACH");
            return;
        };
        let Some(t) = self.target(&item) else { return };
        let r = match &*coll {
            Expr::Bif { name, args, .. } if name == "%SPLIT" => self.split_range(args),
            e => self.range(e),
        };
        let Some(r) = r else { return };
        let cnt = self.b.temp("i64");
        self.b.ins(format!("store {cnt}, {}", r.count));
        let idx = self.b.temp("i64");
        self.b.ins(format!("store {idx}, 0"));
        let top = self.b.label();
        let body = self.b.label();
        let inc = self.b.label();
        let end = self.b.label();
        self.b.place(&top);
        let k = self.b.reg();
        self.b.ins(format!("{k} = load {idx}"));
        let n = self.b.reg();
        self.b.ins(format!("{n} = load {cnt}"));
        let c = self.b.reg();
        self.b.ins(format!("{c} = cmp.lt {k}, {n}"));
        self.b.term(format!("brif {c}, {body}, {end}"));
        self.b.place(&body);
        let e = self.b.reg();
        self.b.ins(format!("{e} = add {}, {k}", r.start));
        let elem = V::place(r.ty.clone(), format!("{}[{e}]", r.base));
        self.assign(&t, &elem, false);
        self.open.push(crate::codegen::Open::Loop {
            iter: inc,
            leave: end,
            end: crate::codegen::LoopEnd::For {
                top,
                index: idx,
                step: None,
                down: false,
            },
        });
    }

    /// The current element count of a `DIM(*AUTO)` or `DIM(*VAR)` array.
    pub fn elem_var(&mut self, id: SymId) -> String {
        let n = format!("{}__elem", self.t.syms[id].ir);
        self.ops_static(&n, "i64")
    }

    /// An index into an array: a `DIM(*AUTO)` array grows to an element
    /// assigned past its end, and `*NEXT` is the element after it.
    pub fn elem_index(&mut self, id: SymId, e: &Expr, dim: u32, modify: bool) -> Option<String> {
        let vd = self.t.syms.get(id).map_or(0, |s| s.vardim);
        if vd == 0 {
            return self.index(e, dim);
        }
        let cv = self.elem_var(id);
        let z = if matches!(e, Expr::Special { word, .. } if word == "*NEXT") {
            let c = self.b.reg();
            self.b.ins(format!("{c} = load {cv}"));
            c
        } else {
            let z = self.index(e, dim)?;
            if z.parse::<i64>().is_ok() {
                let r = self.b.reg();
                self.b.ins(format!("{r} = const.i64 {z}"));
                r
            } else {
                z
            }
        };
        if modify && vd == 1 {
            let c = self.b.reg();
            self.b.ins(format!("{c} = load {cv}"));
            let n = self.b.reg();
            self.b.ins(format!("{n} = add {z}, 1"));
            let g = self.b.reg();
            self.b.ins(format!("{g} = cmp.gt {n}, {c}"));
            let m = self.b.reg();
            self.b.ins(format!("{m} = select {g}, {n}, {c}"));
            self.b.ins(format!("store {cv}, {m}"));
        }
        Some(z)
    }

    /// `%ELEM` of a varying-dimension array: its current count, or with
    /// `*MAX` or `*ALLOC` its dimension. `None` for other arrays.
    pub fn elem_bif(&mut self, args: &[Expr]) -> Option<Option<V>> {
        let Some(Expr::Ref { name, steps, .. }) = args.first() else { return None };
        if !steps.is_empty() {
            return None;
        }
        let id = self.t.find(self.scope, name)?;
        let s = self.t.syms[id].clone();
        if s.vardim == 0 {
            return None;
        }
        self.t.refer(id, self.at.line, false);
        if matches!(args.get(1), Some(Expr::Special { word, .. }) if word == "*MAX" || word == "*ALLOC") {
            return Some(Some(V::lit(Ty::Packed(10, 0), s.dim.to_string())));
        }
        let cv = self.elem_var(id);
        let c = self.b.reg();
        self.b.ins(format!("{c} = load {cv}"));
        let r = self.b.reg();
        self.b.ins(format!("{r} = cvt.i32 {c}"));
        Some(Some(V::reg(Ty::Int(10), r)))
    }

    /// `%ELEM(array) = n`: the new count; elements it adds take their
    /// default value.
    fn elem_assign(&mut self, args: &[Expr], v: &Expr) -> Option<()> {
        let Some(Expr::Ref { name, .. }) = args.first() else { return None };
        let id = self.t.find(self.scope, name)?;
        let s = self.t.syms[id].clone();
        if s.vardim == 0 {
            self.err(ids::NOT_ASSIGNABLE, 30, "%ELEM");
            return None;
        }
        self.t.refer(id, self.at.line, true);
        let nv = self.expr(v, Want::default())?;
        let n = self.int_reg(&nv)?;
        let cv = self.elem_var(id);
        let old = self.b.reg();
        self.b.ins(format!("{old} = load {cv}"));
        let i = self.b.temp("i64");
        self.b.ins(format!("store {i}, {old}"));
        let top = self.b.label();
        let body = self.b.label();
        let end = self.b.label();
        self.b.place(&top);
        let k = self.b.reg();
        self.b.ins(format!("{k} = load {i}"));
        let c = self.b.reg();
        self.b.ins(format!("{c} = cmp.lt {k}, {n}"));
        self.b.term(format!("brif {c}, {body}, {end}"));
        self.b.place(&body);
        let place = format!("{}[{k}]", place_of(self.t, id));
        self.clear_place(&V::place(s.ty.clone(), place));
        let k1 = self.b.reg();
        self.b.ins(format!("{k1} = add {k}, 1"));
        self.b.ins(format!("store {i}, {k1}"));
        self.b.term(format!("br {top}"));
        self.b.place(&end);
        self.b.ins(format!("store {cv}, {n}"));
        Some(())
    }

    /// `%XFOOT` of a `%SUBARR` range or a varying-dimension array; `None`
    /// for a plain array.
    pub fn xfoot_bif(&mut self, args: &[Expr]) -> Option<Option<V>> {
        let a = args.first()?;
        let ranged = match a {
            Expr::Bif { name, .. } => name == "%SUBARR",
            Expr::Ref { name, steps, .. } => steps.is_empty() && self.t.find(self.scope, name).is_some_and(|id| self.t.syms[id].vardim > 0),
            _ => false,
        };
        if !ranged {
            return None;
        }
        let Some(r) = self.range(a) else { return Some(None) };
        let s = match &r.ty {
            Ty::Float(_) => 15,
            t => t.shape().map_or(0, |x| x.1),
        };
        let acc = self.b.temp(&format!("packed(63,{s})"));
        self.b.ins(format!("mov {acc}, *ZEROS"));
        let i = self.b.temp("i64");
        self.b.ins(format!("store {i}, 0"));
        let top = self.b.label();
        let body = self.b.label();
        let end = self.b.label();
        self.b.place(&top);
        let k = self.b.reg();
        self.b.ins(format!("{k} = load {i}"));
        let c = self.b.reg();
        self.b.ins(format!("{c} = cmp.lt {k}, {}", r.count));
        self.b.term(format!("brif {c}, {body}, {end}"));
        self.b.place(&body);
        let e = self.b.reg();
        self.b.ins(format!("{e} = add {}, {k}", r.start));
        let ev = V::place(r.ty.clone(), format!("{}[{e}]", r.base));
        let o = self.dec_operand(&ev);
        self.b.ins(format!("dec.add {acc}, {acc}, {o} prec(63,{s})"));
        let k1 = self.b.reg();
        self.b.ins(format!("{k1} = add {k}, 1"));
        self.b.ins(format!("store {i}, {k1}"));
        self.b.term(format!("br {top}"));
        self.b.place(&end);
        Some(Some(V::place(Ty::Packed(63, s), acc)))
    }

    /// Initialization the operations need: tables at their first
    /// element, alternating compile-time arrays loaded, every occurrence
    /// of a structure like its first.
    pub fn ops_init(&mut self) {
        for id in self.ops.tables.clone() {
            let ix = self.tix(id);
            self.b.ins(format!("store {ix}, 0"));
        }
        let vardims: Vec<SymId> = (0..self.t.syms.len()).filter(|&i| self.t.syms[i].vardim > 0 && self.t.syms[i].scope == 0).collect();
        for id in vardims {
            let cv = self.elem_var(id);
            self.b.ins(format!("store {cv}, 0"));
        }
        let alts: Vec<(SymId, SymId)> = (0..self.t.syms.len())
            .filter(|&i| !self.t.syms[i].alt.is_empty() && self.t.syms[i].scope == 0)
            .filter_map(|i| {
                let main = self.t.find(0, &self.t.syms[i].alt.clone())?;
                Some((main, i))
            })
            .collect();
        for (main, alt) in alts {
            self.load_alternating(main, alt);
        }
        for id in self.ops.occurs.clone() {
            let (occ, cur, n) = self.occ_vars(id);
            let place = place_of(self.t, id);
            for k in 0..n {
                self.b.ins(format!("mov {occ}[{k}], {place}"));
            }
            self.b.ins(format!("store {cur}, 0"));
        }
    }

    /// An alternating pair's compile-time data: each entry the main
    /// array's element then the alternate's.
    fn load_alternating(&mut self, main: SymId, alt: SymId) {
        let m = self.t.syms[main].clone();
        let a = self.t.syms[alt].clone();
        let order = (0..main).filter(|&i| self.t.syms[i].ctdata).count();
        let section = self
            .ctdata
            .iter()
            .find(|c| c.kind == "CTDATA" && c.name == m.name)
            .or_else(|| self.ctdata.iter().filter(|c| c.kind.is_empty()).nth(order))
            .cloned();
        let Some(section) = section else { return };
        let width = |g: &Self, t: &Ty| -> usize {
            usize::try_from(match t {
                t if t.is_numeric() => u32::from(t.shape().map_or(1, |x| x.0)),
                t => g.t.size(t),
            })
            .unwrap_or(1)
            .max(1)
        };
        let (wm, wa) = (width(self, &m.ty), width(self, &a.ty));
        let per = usize::try_from(m.perrcd.max(1)).unwrap_or(1);
        let (pm, pa) = (place_of(self.t, main), place_of(self.t, alt));
        let mut k = 0u32;
        for rec in &section.records {
            let chars: Vec<char> = rec.chars().collect();
            for e in 0..per {
                if k >= m.dim {
                    return;
                }
                let from = e * (wm + wa);
                let take = |at: usize, w: usize| -> String { (at..at + w).map(|i| chars.get(i).copied().unwrap_or(' ')).collect() };
                let tm = take(from, wm);
                let ta = take(from + wm, wa);
                self.ctdata_elem(&m.ty, &format!("{pm}[{k}]"), &tm);
                if k < a.dim {
                    self.ctdata_elem(&a.ty, &format!("{pa}[{k}]"), &ta);
                }
                k += 1;
            }
        }
    }

    /// One compile-time data entry into an element: characters as they
    /// are, numbers as digits with the sign in the last zone.
    fn ctdata_elem(&mut self, ty: &Ty, place: &str, text: &str) {
        if !ty.is_numeric() {
            self.b.ins(format!("mov {place}, {}", quote(text)));
            return;
        }
        let n = text.chars().count();
        let last = text.chars().last().unwrap_or('0');
        let neg = "}JKLMNOPQR".contains(last);
        let digits: String = text
            .chars()
            .enumerate()
            .map(|(i, c)| match c {
                '0'..='9' => c,
                '}' | '{' => '0',
                'A'..='I' if i + 1 == n => char::from(b'1' + (c as u8 - b'A')),
                'J'..='R' if i + 1 == n => char::from(b'1' + (c as u8 - b'J')),
                _ => '0',
            })
            .collect();
        let sc = usize::from(ty.shape().map_or(0, |x| x.1));
        let (i, f) = digits.split_at(digits.len().saturating_sub(sc));
        let i = if i.is_empty() { "0" } else { i };
        let lit = if sc > 0 { format!("{i}.{f}") } else { i.to_string() };
        self.b.ins(format!("dec.mov {place}, {}{lit}", if neg { "-" } else { "" }));
    }

    /// Pre-run-time arrays (`FROMFILE`): their elements read from the
    /// file's records, `PERRCD` to a record, before the first cycle.
    fn prerun_arrays(&mut self) {
        let arrays: Vec<SymId> = (0..self.t.syms.len()).filter(|&i| !self.t.syms[i].fromfile.is_empty()).collect();
        for id in arrays {
            let s = self.t.syms[id].clone();
            let Some(f) = self.files.iter().find(|f| f.name.eq_ignore_ascii_case(&s.fromfile)).cloned() else {
                self.err(ids::FILE_NOT_DEFINED, 30, s.fromfile.clone());
                continue;
            };
            let n = mangle(&f.name);
            let elen = self.t.size(&s.ty);
            let per = s.perrcd.max(1);
            let place = place_of(self.t, id);
            let done = self.b.label();
            for rec in 0..s.dim.div_ceil(per) {
                let st = self.b.reg();
                self.b.ins(format!("{st} = dbio {n} READ rec($rb_{n})"));
                let ok = self.b.reg();
                self.b.ins(format!("{ok} = cmp.eq {st}, 0"));
                let take = self.b.label();
                self.b.term(format!("brif {ok}, {take}, {done}"));
                self.b.place(&take);
                let a = self.b.reg();
                self.b.ins(format!("{a} = addr $rb_{n}"));
                for j in 0..per {
                    let k = rec * per + j;
                    if k >= s.dim {
                        break;
                    }
                    self.b.ins(format!("mov {place}[{k}], [{a} +{}]:{}", j * elen, self.ir_ty(&s.ty)));
                }
            }
            self.b.term(format!("br {done}"));
            self.b.place(&done);
        }
    }

    /// `TOFILE` arrays written when the program ends, `PERRCD` elements
    /// to a record.
    pub fn tofile_arrays(&mut self) {
        let arrays: Vec<SymId> = (0..self.t.syms.len()).filter(|&i| !self.t.syms[i].tofile.is_empty()).collect();
        for id in arrays {
            let s = self.t.syms[id].clone();
            let Some(f) = self.files.iter().find(|f| f.name.eq_ignore_ascii_case(&s.tofile)).cloned() else { continue };
            let n = mangle(&f.name);
            let elen = self.t.size(&s.ty);
            let per = s.perrcd.max(1);
            let len = if f.reclen == 0 { 132 } else { f.reclen };
            let place = place_of(self.t, id);
            for rec in 0..s.dim.div_ceil(per) {
                let line = self.b.temp(&format!("char({len})"));
                self.b.ins(format!("mov {line}, *BLANKS"));
                let a = self.b.reg();
                self.b.ins(format!("{a} = addr {line}"));
                for j in 0..per {
                    let k = rec * per + j;
                    if k >= s.dim || (j + 1) * elen > len {
                        break;
                    }
                    self.b.ins(format!("mov [{a} +{}]:{}, {place}[{k}]", j * elen, self.ir_ty(&s.ty)));
                }
                let st = self.b.reg();
                if f.dev == crate::files::Dev::Printer {
                    self.b.ins(format!("{st} = prtio {n} WRITE fmt({n}) rec({line}) ctl(0, 1, 0, 0)"));
                } else {
                    self.b.ins(format!("{st} = dbio {n} WRITE rec({line})"));
                }
            }
        }
    }

    /// The call `RESET` needs once initialization is done: `*INZSR`,
    /// then the copies taken.
    pub fn init_tail(&mut self, m: &Module) {
        self.prerun_arrays();
        let inzsr = m.calcs.iter().any(|s| {
            s.op.code == "BEGSR" && matches!(s.op.f1.first(), Some(Expr::Special { word, .. }) if word == "*INZSR")
        });
        if inzsr {
            if self.subr_procs {
                self.exsr_call("*INZSR");
            } else {
                self.exsr("*INZSR");
            }
        }
        self.b.ins("callb _qrni_snap()");
    }
}

/// The procedure that takes `RESET`'s copies.
pub fn snap_proc(g: &mut Gen<'_>) -> String {
    g.b = Body::new();
    g.scope = 0;
    for id in g.ops.resets.clone() {
        let s = g.t.syms[id].clone();
        let place = place_of(g.t, id);
        if let Some(Kind::Ds(di)) = s.kind
            && g.t.ds[di].occurs > 0
        {
            let (occ, cur, _) = g.occ_vars(id);
            let c = g.b.reg();
            g.b.ins(format!("{c} = load {cur}"));
            g.b.ins(format!("mov {occ}[{c}], {place}"));
            g.b.ins(format!("mov {}__orst, {occ}", s.ir));
        }
        g.b.ins(format!("mov {}__rst, {place}", s.ir));
    }
    let body = std::mem::take(&mut g.b);
    let _ = Lit::Num(String::new());
    body.finish("proc _qrni_snap()", "ret")
}

/// An expression with each of the arrays named indexed by `idx`.
fn index_arrays(e: &Expr, arrays: &[String], idx: &str) -> Expr {
    let at = e.pos();
    match e {
        Expr::Ref { name, steps, .. } if steps.is_empty() && arrays.contains(name) => Expr::Ref {
            name: name.clone(),
            steps: vec![rpgfront::ast::Step::Args(vec![Expr::Ref {
                name: idx.to_string(),
                steps: Vec::new(),
                at,
            }])],
            at,
        },
        Expr::Unary(op, a, p) => Expr::Unary(*op, Box::new(index_arrays(a, arrays, idx)), *p),
        Expr::Binary(op, a, b, p) => {
            Expr::Binary(*op, Box::new(index_arrays(a, arrays, idx)), Box::new(index_arrays(b, arrays, idx)), *p)
        }
        Expr::Bif { name, args, at } if !matches!(name.as_str(), "%ELEM" | "%SIZE" | "%ADDR" | "%XFOOT" | "%LOOKUP" | "%SUBARR") => {
            Expr::Bif {
                name: name.clone(),
                args: args.iter().map(|a| index_arrays(a, arrays, idx)).collect(),
                at: *at,
            }
        }
        _ => e.clone(),
    }
}

/// `LOOKUP`'s argument against the elements: numbers of the same
/// digits and decimals, strings of the same length.
fn same_type_and_length(a: &Ty, b: &Ty) -> bool {
    if a.is_numeric() && b.is_numeric() {
        return match (a, b) {
            (Ty::Float(x), Ty::Float(y)) => x == y,
            _ => a.shape().is_some() && a.shape() == b.shape(),
        };
    }
    if a.is_string() && b.is_string() {
        return a.str_len() == b.str_len();
    }
    a == b
}

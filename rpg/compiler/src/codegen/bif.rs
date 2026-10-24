// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Built-in functions and calls. A built-in becomes IR string, decimal
//! or conversion instructions where the IR has them and a `qrpgrt`
//! procedure (`_QRNX_...`) where it does not. Calls go through their
//! prototype: bound calls to procedures of this module or imported
//! ones, dynamic calls for `EXTPGM`; `CONST` and `VALUE` arguments are
//! converted into temporaries of the parameter's type.

use rpgfront::ast::{Expr, Fig, Lit, Step};

use crate::diag::ids;
use crate::codegen::emit::quote;
use crate::codegen::expr::{V, Vk, Want};
use crate::codegen::Gen;
use crate::names::mangle;
use crate::syms::{Kind, Parm, Proto, SymId};
use crate::types::{DatFmt, Ty};

impl Gen<'_> {
    fn bif_arity(&mut self, name: &str, args: &[Expr], min: usize, max: usize) -> bool {
        // IBM ignores the built-in function.
        if args.len() < min {
            self.err(ids::BIF_TOO_FEW, 20, name.to_string());
            return false;
        }
        if args.len() > max {
            self.err(ids::BIF_TOO_MANY, 20, name.to_string());
            return false;
        }
        true
    }

    /// A string argument (a structure counts as character data).
    fn str_arg(&mut self, e: &Expr) -> Option<V> {
        let v = self.expr(e, Want::default())?;
        let v = self.as_chars(&v);
        if !v.ty.is_string() && v.fig.is_none() {
            self.err(ids::BIF_ARGS, 30, "");
            return None;
        }
        Some(v)
    }

    /// A string argument; anything else is `id` (sev 20).
    fn str_arg_or(&mut self, e: &Expr, id: &'static str, name: &str) -> Option<V> {
        let v = self.expr(e, Want::default())?;
        let v = self.as_chars(&v);
        if !v.ty.is_string() && v.fig.is_none() {
            self.err(id, 20, name.to_string());
            return None;
        }
        Some(v)
    }

    /// An integer argument as an `i64` register.
    fn int_arg(&mut self, e: &Expr) -> Option<String> {
        let v = self.expr(e, Want::default())?;
        self.int_reg(&v)
    }

    /// A literal in a temporary of its own length, for a `*VARSIZE`
    /// parameter (the IR would place it in one of the parameter's).
    pub fn lit_storage(&mut self, v: &V) -> String {
        if v.kind != Vk::Lit {
            return v.op.clone();
        }
        let n = v.ty.str_len().unwrap_or(1).max(1);
        let t = self.b.temp(&format!("char({n})"));
        self.b.ins(format!("mov {t}, {}", v.op));
        t
    }

    /// A temporary of a type.
    fn tmp(&mut self, ty: &Ty) -> V {
        let t = self.b.temp(&self.ir_ty(ty));
        V::place(ty.clone(), t)
    }

    /// Declare a runtime library procedure once.
    pub fn rt_proc(&mut self, name: &str, decl: &str) {
        self.externs
            .entry(name.to_string())
            .or_insert_with(|| format!("extern proc {name}{decl}"));
    }

    /// A built-in function.
    #[allow(clippy::too_many_lines)]
    pub fn bif(&mut self, name: &str, args: &[Expr], w: Want) -> Option<V> {
        if let Some(v) = self.db_bif(name, args) {
            return v;
        }
        if let Some(v) = self.dt_bif(name, args) {
            return v;
        }
        match name {
            "%LEN" => {
                if !self.bif_arity(name, args, 1, 1) {
                    return None;
                }
                let v = self.expr(&args[0], w)?;
                let v = self.as_chars(&v);
                if v.ty.is_varying() {
                    let r = self.b.reg();
                    self.b.ins(format!("{r} = str.len {}", v.op));
                    return Some(V::reg(Ty::Int(10), r));
                }
                let n = match &v.ty {
                    t if t.is_string() => t.str_len().unwrap_or(0),
                    // A float's length is its size in bytes.
                    Ty::Float(n) => u32::from(*n),
                    t if t.is_numeric() => u32::from(t.shape().map_or(0, |s| s.0)),
                    t => self.t.size(t),
                };
                Some(V::lit(Ty::Packed(10, 0), n.to_string()))
            }
            "%DECPOS" => {
                if !self.bif_arity(name, args, 1, 1) {
                    return None;
                }
                let v = self.expr(&args[0], w)?;
                match (&v.ty, v.ty.shape()) {
                    (t, Some((_, s))) if t.is_numeric() && !matches!(t, Ty::Float(_)) => {
                        Some(V::lit(Ty::Packed(10, 0), s.to_string()))
                    }
                    _ => {
                        let shown = match &args[0] {
                            Expr::Ref { name, .. } => name.clone(),
                            _ => String::new(),
                        };
                        self.err(ids::DECPOS_ARG, 30, shown);
                        None
                    }
                }
            }
            "%SIZE" => {
                let Some(a) = args.first() else {
                    self.err(rpgfront::diag::ids::MISSING_OPERAND, 30, name.to_string());
                    return None;
                };
                let n = self.size_of(a)?;
                let all = args.get(1).is_some_and(|a| matches!(a, Expr::Special { word, .. } if word == "*ALL"));
                let n = if all { n.0 * n.1.max(1) } else { n.0 };
                Some(V::lit(Ty::Packed(10, 0), n.to_string()))
            }
            "%ELEM" => {
                if let Some(r) = self.elem_bif(args) {
                    return r;
                }
                let v = self.expr(args.first()?, w)?;
                if v.dim == 0 {
                    self.err(ids::ELEM_ARG, 20, name.to_string());
                    return None;
                }
                Some(V::lit(Ty::Packed(10, 0), v.dim.to_string()))
            }
            "%TRIM" | "%TRIML" | "%TRIMR" => {
                if !self.bif_arity(name, args, 1, 2) {
                    return None;
                }
                let s = self.str_arg_or(&args[0], ids::TRIM_PARM, name)?;
                let len = s.ty.str_len().unwrap_or(1).max(1);
                let t = self.tmp(&Ty::VarChar(len, 2));
                let op = match name {
                    "%TRIM" => "str.trim",
                    "%TRIML" => "str.triml",
                    _ => "str.trimr",
                };
                match args.get(1) {
                    Some(c) => {
                        let c = self.str_arg(c)?;
                        self.b.ins(format!("{op} {}, {}, {}", t.op, s.op, c.op));
                    }
                    None => self.b.ins(format!("{op} {}, {}", t.op, s.op)),
                }
                Some(t)
            }
            "%SUBST" | "%SUBSTR" => {
                if !self.bif_arity(name, args, 2, 3) {
                    return None;
                }
                let s = self.str_arg(&args[0])?;
                let sv = self.expr(&args[1], Want::default())?;
                if !sv.ty.is_numeric() {
                    self.err(ids::SUBST_START, 20, name.to_string());
                    return None;
                }
                let len = s.ty.str_len().unwrap_or(1).max(1);
                // Constant positions inside a fixed-length string.
                let lit = |e: Option<&Expr>| match e {
                    Some(Expr::Lit(Lit::Num(n), _)) => n.parse::<i64>().ok(),
                    _ => None,
                };
                if let Some(p) = lit(args.get(1))
                    && (p < 1 || !s.ty.is_varying() && p > i64::from(len))
                {
                    self.err(ids::SUBST_START, 20, name.to_string());
                    return None;
                }
                if let (Some(p), Some(n)) = (lit(args.get(1)), lit(args.get(2)))
                    && !s.ty.is_varying()
                    && p + n - 1 > i64::from(len)
                {
                    self.err(ids::SUBST_LEN, 20, name.to_string());
                    return None;
                }
                let start = self.int_reg(&sv)?;
                // A constant length gives a fixed result of that length.
                let (ty, l) = match args.get(2) {
                    Some(Expr::Lit(Lit::Num(n), _)) => {
                        let k: u32 = n.parse().unwrap_or(1);
                        (Ty::Char(k.max(1)), Some(self.int_arg(&args[2])?))
                    }
                    Some(e) => (Ty::VarChar(len, 2), Some(self.int_arg(e)?)),
                    None => (Ty::VarChar(len, 2), None),
                };
                let t = self.tmp(&ty);
                match l {
                    Some(l) => self.b.ins(format!("str.subst {}, {}, {start}, {l}", t.op, s.op)),
                    None => {
                        let r = self.b.reg();
                        self.b.ins(format!("{r} = str.len {}", s.op));
                        let r64 = self.b.reg();
                        self.b.ins(format!("{r64} = cvt.i64 {r}"));
                        let n = self.b.reg();
                        self.b.ins(format!("{n} = sub {r64}, {start}"));
                        let n1 = self.b.reg();
                        self.b.ins(format!("{n1} = add {n}, 1"));
                        self.b.ins(format!("str.subst {}, {}, {start}, {n1}", t.op, s.op));
                    }
                }
                Some(t)
            }
            "%SCAN" => {
                if !self.bif_arity(name, args, 2, 3) {
                    return None;
                }
                let n = self.str_arg_or(&args[0], ids::SCAN_PARM, name)?;
                let h = self.str_arg(&args[1])?;
                let r = self.b.reg();
                match args.get(2) {
                    Some(s) => {
                        let s = self.int_arg(s)?;
                        self.b.ins(format!("{r} = str.scan {}, {}, {s}", n.op, h.op));
                    }
                    None => self.b.ins(format!("{r} = str.scan {}, {}", n.op, h.op)),
                }
                Some(V::reg(Ty::Int(10), r))
            }
            "%CHECK" | "%CHECKR" => {
                if !self.bif_arity(name, args, 2, 3) {
                    return None;
                }
                let set = self.str_arg(&args[0])?;
                let s = self.str_arg(&args[1])?;
                let r = self.b.reg();
                let op = if name == "%CHECK" { "str.check" } else { "str.checkr" };
                match args.get(2) {
                    Some(st) => {
                        let st = self.int_arg(st)?;
                        self.b.ins(format!("{r} = {op} {}, {}, {st}", set.op, s.op));
                    }
                    None => self.b.ins(format!("{r} = {op} {}, {}", set.op, s.op)),
                }
                Some(V::reg(Ty::Int(10), r))
            }
            "%XLATE" => {
                if !self.bif_arity(name, args, 3, 4) {
                    return None;
                }
                let from = self.str_arg(&args[0])?;
                let to = self.str_arg(&args[1])?;
                let s = self.str_arg(&args[2])?;
                let t = self.tmp(&s.ty.clone());
                match args.get(3) {
                    Some(st) => {
                        let st = self.int_arg(st)?;
                        self.b.ins(format!("str.xlate {}, {}, {}, {}, {st}", t.op, from.op, to.op, s.op));
                    }
                    None => self.b.ins(format!("str.xlate {}, {}, {}, {}", t.op, from.op, to.op, s.op)),
                }
                Some(t)
            }
            "%UPPER" | "%LOWER" => {
                if !self.bif_arity(name, args, 1, 3) {
                    return None;
                }
                let s = self.str_arg(&args[0])?;
                let (lo, up) = (quote("abcdefghijklmnopqrstuvwxyz"), quote("ABCDEFGHIJKLMNOPQRSTUVWXYZ"));
                let (from, to) = if name == "%UPPER" { (lo, up) } else { (up, lo) };
                let t = self.tmp(&s.ty.clone());
                self.b.ins(format!("str.xlate {}, {from}, {to}, {}", t.op, s.op));
                Some(t)
            }
            "%REPLACE" => {
                if !self.bif_arity(name, args, 2, 4) {
                    return None;
                }
                let repl = self.str_arg_or(&args[0], ids::REPLACE_PARM, name)?;
                let src = self.str_arg(&args[1])?;
                // An empty replacement: a varying temporary of length 0.
                let repl = if repl.kind == Vk::Lit && repl.ty.str_len() == Some(0) {
                    let t = self.tmp(&Ty::VarChar(1, 2));
                    self.b.ins(format!("str.setlen {}, 0", t.op));
                    t
                } else {
                    repl
                };
                let n = repl.ty.str_len().unwrap_or(0) + src.ty.str_len().unwrap_or(0);
                let t = self.tmp(&Ty::VarChar(n.max(1), 2));
                let start = match args.get(2) {
                    Some(e) => self.int_arg(e)?,
                    None => "1".into(),
                };
                match args.get(3) {
                    Some(l) => {
                        let l = self.int_arg(l)?;
                        self.b.ins(format!("str.replace {}, {}, {}, {start}, {l}", t.op, src.op, repl.op));
                    }
                    None => self.b.ins(format!("str.replace {}, {}, {}, {start}", t.op, src.op, repl.op)),
                }
                Some(t)
            }
            "%UCS2" | "%GRAPH" => {
                if !self.bif_arity(name, args, 1, 2) {
                    return None;
                }
                let v = self.expr(&args[0], Want::default())?;
                let v = self.as_chars(&v);
                let to = if name == "%UCS2" { super::ucs2::TextClass::Ucs2 } else { super::ucs2::TextClass::Graph };
                if !v.ty.is_string() {
                    self.err(ids::BIF_ARGS, 30, name.to_string());
                    return None;
                }
                self.to_class(&v, to)
            }
            "%CHAR" => {
                if !self.bif_arity(name, args, 1, 2) {
                    return None;
                }
                let v = self.expr(&args[0], Want::default())?;
                let v = self.as_chars(&v);
                if v.ty.is_string() {
                    return self.to_class(&v, super::ucs2::TextClass::Char);
                }
                if v.ty.is_numeric() {
                    return Some(self.char_of_number(&v));
                }
                if v.ty.is_datetime() {
                    return self.dt_char(&v, args.get(1));
                }
                if v.ty == Ty::Ind {
                    let t = self.tmp(&Ty::Char(1));
                    self.assign_char_of_ind(&t, &v);
                    return Some(t);
                }
                self.err(ids::BIF_ARGS, 30, name.to_string());
                None
            }
            "%DEC" | "%DECH" => self.dec_bif(name, args),
            "%INT" | "%INTH" | "%UNS" | "%UNSH" => {
                if !self.bif_arity(name, args, 1, 1) {
                    return None;
                }
                let v = self.expr(&args[0], Want::default())?;
                let uns = name.starts_with("%UNS");
                let ty = if uns { Ty::Uns(20) } else { Ty::Int(20) };
                let t = self.tmp(&ty);
                let half = if name.ends_with('H') { " halfadj" } else { "" };
                if v.ty.is_string() {
                    self.char_to_num(&t, &v, name.ends_with('H'));
                } else if v.ty.is_numeric() {
                    let s = self.dec_operand(&v);
                    self.b.ins(format!("dec.mov {}, {s}{half}", t.op));
                } else {
                    self.err(ids::BIF_ARGS, 30, name.to_string());
                    return None;
                }
                Some(t)
            }
            "%FLOAT" => {
                let v = self.expr(args.first()?, Want::default())?;
                if v.ty.is_string() {
                    let t = self.tmp(&Ty::Float(8));
                    self.b.ins(format!("cvt {}, {}", t.op, v.op));
                    return Some(t);
                }
                let r = self.float_reg(&v);
                Some(V::reg(Ty::Float(8), r))
            }
            "%ABS" => {
                let v = self.expr(args.first()?, w)?;
                match &v.ty {
                    Ty::Float(_) => {
                        let f = self.float_reg(&v);
                        let r = self.b.reg();
                        self.b.ins(format!("{r} = abs {f}"));
                        Some(V::reg(Ty::Float(8), r))
                    }
                    t if t.is_integer() => {
                        let i = self.int_reg(&v)?;
                        let r = self.b.reg();
                        self.b.ins(format!("{r} = abs {i}"));
                        Some(V::reg(Ty::Int(20), r))
                    }
                    t if t.is_numeric() => {
                        let (d, s) = t.shape()?;
                        let tm = self.tmp(&Ty::Packed(d, s));
                        let opnd = self.dec_operand(&v);
                        self.b.ins(format!("dec.abs {}, {}", tm.op, opnd));
                        Some(tm)
                    }
                    _ => {
                        self.err(ids::BIF_PARM, 30, name.to_string());
                        None
                    }
                }
            }
            "%DIV" | "%REM" => {
                if !self.bif_arity(name, args, 2, 2) {
                    return None;
                }
                let a = self.expr(&args[0], Want::default())?;
                let b = self.expr(&args[1], Want::default())?;
                if a.ty.is_integer() && b.ty.is_integer() || a.ty.shape().is_some_and(|s| s.1 == 0) && b.ty.shape().is_some_and(|s| s.1 == 0) {
                    let ra = self.int_reg(&a)?;
                    let rb = self.int_reg(&b)?;
                    let r = self.b.reg();
                    let op = if name == "%DIV" { "div" } else { "rem" };
                    self.b.ins(format!("{r} = {op} {ra}, {rb}"));
                    return Some(V::reg(Ty::Int(20), r));
                }
                self.err(ids::BIF_ARGS, 30, name.to_string());
                None
            }
            "%EDITC" => self.editc(args),
            "%EDITW" => self.editw(args),
            "%STATUS" if args.is_empty() => Some(V::place(Ty::Zoned(5, 0), "$status")),
            "%ERROR" if args.is_empty() => Some(V::place(Ty::Ind, "$errind")),
            "%PARMS" => {
                let r = self.b.reg();
                // The main procedure's are the program's, which the
                // entry procedure kept.
                if self.scope == 0 || self.is_linear_main(&self.cur_proc.clone()) {
                    self.b.ins(format!("{r} = load $entry_n"));
                } else {
                    self.b.ins(format!("{r} = parms"));
                }
                Some(V::reg(Ty::Int(10), r))
            }
            "%ADDR" => {
                let v = self.expr(args.first()?, Want::default())?;
                if v.kind != Vk::Place {
                    self.err(ids::ADDR_PARM, 20, name.to_string());
                    return None;
                }
                let r = self.b.reg();
                self.b.ins(format!("{r} = addr {}", v.op));
                Some(V::reg(Ty::Ptr, r))
            }
            "%PADDR" => {
                let n = match args.first()? {
                    Expr::Lit(Lit::Char(s), _) => s.to_ascii_uppercase(),
                    Expr::Ref { name: n, .. } => {
                        if self.t.find(self.scope, n).is_none() {
                            self.err(ids::NOT_DEFINED, 30, n.clone());
                            self.err(ids::BIF_NAME_UNDEFINED, 30, name.to_string());
                            return None;
                        }
                        n.clone()
                    }
                    _ => return None,
                };
                let r = self.b.reg();
                self.b.ins(format!("{r} = procaddr {}", mangle(&n)));
                Some(V::reg(Ty::ProcPtr, r))
            }
            "%ALLOC" => {
                let n = self.int_arg(args.first()?)?;
                let r = self.b.reg();
                self.b.ins(format!("{r} = alloc {n}"));
                Some(V::reg(Ty::Ptr, r))
            }
            "%REALLOC" => {
                let p = self.expr(args.first()?, Want::default())?;
                let p = self.ptr_reg(&p);
                let n = self.int_arg(args.get(1)?)?;
                let r = self.b.reg();
                self.b.ins(format!("{r} = realloc {p}, {n}"));
                Some(V::reg(Ty::Ptr, r))
            }
            "%XFOOT" => {
                if let Some(r) = self.xfoot_bif(args) {
                    return r;
                }
                let v = self.expr(args.first()?, Want::default())?;
                if v.dim == 0 || !v.ty.is_numeric() {
                    self.err(ids::BIF_ARGS, 30, name.to_string());
                    return None;
                }
                let (d, s) = v.ty.shape().unwrap_or((31, 0));
                let acc = self.tmp(&Ty::Packed(u8::min(63, d + 10), s));
                self.b.ins(format!("mov {}, *ZEROS", acc.op));
                let digits = u8::min(63, d + 10);
                for i in 0..v.dim {
                    self.b.ins(format!("dec.add {}, {}, {}[{i}] prec({digits},{s})", acc.op, acc.op, v.op));
                }
                Some(acc)
            }
            "%SQRT" => {
                let v = self.expr(args.first()?, Want::default())?;
                let f = self.float_reg(&v);
                self.rt_proc("_QRNX_SQRT", "(byval $a: f64) -> f64");
                let r = self.b.reg();
                self.b.ins(format!("{r} = callb _QRNX_SQRT({f})"));
                Some(V::reg(Ty::Float(8), r))
            }
            "%MAX" | "%MIN" => {
                if args.len() < 2 {
                    self.err(ids::BIF_ARGS, 30, name.to_string());
                    return None;
                }
                let mut best = self.expr(&args[0], w)?;
                for a in &args[1..] {
                    let v = self.expr(a, w)?;
                    let op = if name == "%MAX" { rpgfront::ast::BinOp::Gt } else { rpgfront::ast::BinOp::Lt };
                    let c = self.compare(op, &v, &best)?;
                    let ty = if best.ty.is_numeric() && v.ty.is_numeric() {
                        let (d1, s1) = best.ty.shape().unwrap_or((31, 0));
                        let (d2, s2) = v.ty.shape().unwrap_or((31, 0));
                        let s = s1.max(s2);
                        let i = (d1 - s1).max(d2 - s2);
                        Ty::Packed((i + s).min(63), s)
                    } else {
                        Self::dt_result(&best.ty)
                    };
                    let t = self.tmp(&ty);
                    let take = self.b.label();
                    let keep = self.b.label();
                    let done = self.b.label();
                    self.b.term(format!("brif {}, {take}, {keep}", c.op));
                    self.b.place(&take);
                    self.assign(&t, &v, false);
                    self.b.term(format!("br {done}"));
                    self.b.place(&keep);
                    self.assign(&t, &best, false);
                    self.b.place(&done);
                    best = t;
                }
                Some(best)
            }
            _ => {
                if let Some(r) = self.bif_ext(name, args, w) {
                    return r;
                }
                self.err(ids::UNKNOWN_BIF, 20, name.to_string());
                None
            }
        }
    }

    /// `%SIZE`: one element's bytes and the elements.
    fn size_of(&mut self, e: &Expr) -> Option<(u32, u32)> {
        if let Expr::Ref { name, steps, .. } = e
            && steps.is_empty()
            && let Some(id) = self.t.find(self.scope, name)
        {
            let s = self.t.syms[id].clone();
            if let Some(Kind::Const(_)) = s.kind {
                // A numeric constant is as long as its digits, as a literal is.
                let v = self.expr(e, Want::default())?;
                if v.kind == Vk::Lit && v.ty.is_numeric() {
                    return Some((u32::from(v.ty.shape().map_or(1, |s| s.0)), 0));
                }
                return Some((v.ty.str_len().unwrap_or_else(|| self.t.size(&v.ty)), 0));
            }
            return Some((self.t.size(&s.ty), s.dim));
        }
        let v = self.expr(e, Want::default())?;
        if v.kind == Vk::Lit && v.ty.is_numeric() {
            let (d, _) = v.ty.shape().unwrap_or((1, 0));
            return Some((u32::from(d), 0));
        }
        Some((self.t.size(&v.ty), v.dim))
    }

    fn assign_char_of_ind(&mut self, t: &V, v: &V) {
        match v.kind {
            Vk::Place => self.b.ins(format!("cvt {}, {}", t.op, v.op)),
            Vk::Lit => self.b.ins(format!("mov {}, {}", t.op, if v.op == "*ON" { quote("1") } else { quote("0") })),
            Vk::Reg => {
                let i = self.b.temp("ind");
                self.b.ins(format!("store {i}, {}", v.op));
                self.b.ins(format!("cvt {}, {i}", t.op));
            }
        }
    }

    /// Character data to a number by RPG's rules (blanks around it, a
    /// sign before or after, the `DECEDIT` point); anything else is
    /// status 105.
    pub fn char_to_num(&mut self, dst: &V, src: &V, half: bool) {
        let s = self.lit_storage(src);
        let n = src.ty.str_len().unwrap_or(1);
        let sv = V::place(src.ty.clone(), s.clone());
        let sd = self.desc_of(&sv);
        let dd = self.desc_of(dst);
        self.rt_proc(
            "_QRNX_CHARTONUM",
            "($dst: bytes(1) *VARSIZE, byval $dd: i64, $src: bytes(1) *VARSIZE, byval $sd: i64, byval $flags: i32)",
        );
        let point = i32::from(u8::try_from(self.ctl.decedit.0).unwrap_or(b'.'));
        let _ = n;
        self.b.ins(format!(
            "callb _QRNX_CHARTONUM({}, {dd}, {s}, {sd}, {})",
            dst.op,
            point | (i32::from(half) << 8)
        ));
    }

    /// `%CHAR` of a number: IBM's form (no leading zeros, a leading
    /// minus, the `DECEDIT` point) through the runtime library.
    pub fn char_of_number(&mut self, v: &V) -> V {
        let (d, s) = match &v.ty {
            Ty::Float(_) => (30, 0),
            t => t.shape().unwrap_or((31, 0)),
        };
        let len = u32::from(d) + 2 + if s > 0 && d == s { 1 } else { 0 };
        let len = if matches!(v.ty, Ty::Float(_)) { 30 } else { len };
        let t = self.tmp(&Ty::VarChar(len.max(1), 2));
        let src = self.num_place(v);
        let desc = self.desc_of(&src);
        self.rt_proc("_QRNX_CHARNUM", "($dst: varchar(1) *VARSIZE, $src: bytes(1) *VARSIZE, byval $desc: i64, byval $point: i32)");
        let point = i32::from(u8::try_from(self.ctl.decedit.0).unwrap_or(b'.'));
        self.b.ins(format!("callb _QRNX_CHARNUM({}, {}, {desc}, {point})", t.op, src.op));
        t
    }

    /// A numeric value in storage: places stay, the rest go to a
    /// temporary of their own type (registers as 20-digit integers or
    /// floats).
    pub fn num_place(&mut self, v: &V) -> V {
        if v.kind == Vk::Place {
            return v.clone();
        }
        let ty = match (&v.ty, v.kind) {
            (Ty::Float(_), _) => Ty::Float(8),
            (t, Vk::Reg) if t.is_integer() => Ty::Int(20),
            (t, _) => t.clone(),
        };
        let t = self.tmp(&ty);
        let s = self.dec_operand(v);
        self.b.ins(format!("dec.mov {}, {s}", t.op));
        t
    }

    fn dec_bif(&mut self, name: &str, args: &[Expr]) -> Option<V> {
        if !self.bif_arity(name, args, 1, 3) {
            return None;
        }
        let v = self.expr(&args[0], Want::default())?;
        if v.ty.is_datetime() {
            return self.dt_dec(&v, args.get(1));
        }
        let num = |e: Option<&Expr>| -> Option<u8> {
            match e? {
                Expr::Lit(Lit::Num(n), _) => n.parse().ok(),
                _ => None,
            }
        };
        let (d, s) = match (num(args.get(1)), num(args.get(2))) {
            (Some(d), s) => (d, s.unwrap_or(0)),
            _ => match &v.ty {
                Ty::Int(n) | Ty::Uns(n) => (*n, 0),
                Ty::Float(_) => (30, 9),
                t if t.is_numeric() => t.shape().unwrap_or((30, 0)),
                _ => (63, 15),
            },
        };
        if d == 0 || d > 63 {
            self.err(ids::DEC_DIGITS, 30, name.to_string());
            return None;
        }
        if s > d {
            self.err(ids::DEC_SCALE, 30, name.to_string());
            return None;
        }
        let t = self.tmp(&Ty::Packed(d, s));
        let half = if name == "%DECH" { " halfadj" } else { "" };
        if v.ty.is_string() {
            self.char_to_num(&t, &v, name == "%DECH");
        } else if v.ty.is_numeric() {
            let o = self.dec_operand(&v);
            self.b.ins(format!("dec.mov {}, {o}{half}", t.op));
        } else {
            self.err(ids::BIF_ARGS, 30, name.to_string());
            return None;
        }
        Some(t)
    }

    fn editc(&mut self, args: &[Expr]) -> Option<V> {
        if !self.bif_arity("%EDITC", args, 2, 3) {
            return None;
        }
        let v = self.expr(&args[0], Want::default())?;
        if !v.ty.is_numeric() {
            self.err(ids::BIF_ARGS, 30, "%EDITC");
            return None;
        }
        let code = match &args[1] {
            Expr::Lit(Lit::Char(c), _) if c.chars().count() == 1 => c.to_ascii_uppercase(),
            _ => {
                self.err(ids::BIF_ARGS, 30, "%EDITC");
                return None;
            }
        };
        // 0 none, 1 *ASTFILL, 2 *CURSYM, 3 a currency literal.
        let (fill, cur) = match args.get(2) {
            Some(Expr::Special { word, .. }) if word == "*ASTFILL" => (1, String::new()),
            Some(Expr::Special { word, .. }) if word == "*CURSYM" => (2, String::new()),
            Some(Expr::Lit(Lit::Char(c), _)) => (3, c.clone()),
            None => (0, String::new()),
            _ => {
                self.err(ids::BIF_ARGS, 30, "%EDITC");
                return None;
            }
        };
        let src = self.num_place(&v);
        let (d, s) = src.ty.shape().unwrap_or((30, 0));
        let shape = qdecimal::Shape::new(d.max(1), s).ok()?;
        let opts = qdecimal::EditOpts::default();
        let ch = code.chars().next()?;
        let width = qdecimal::code_width(shape, ch, &opts).unwrap_or(u32::from(d) as usize + 4) + 1;
        let t = self.tmp(&Ty::VarChar(u32::try_from(width).unwrap_or(40), 2));
        let desc = self.desc_of(&src);
        self.rt_proc(
            "_QRNX_EDITC",
            "($dst: varchar(1) *VARSIZE, $src: bytes(1) *VARSIZE, byval $desc: i64, byval $code: i32, byval $fill: i32, const $cur: char(1) *VARSIZE)",
        );
        let cur_lit = if cur.is_empty() { quote(" ") } else { quote(&cur) };
        self.b.ins(format!(
            "callb _QRNX_EDITC({}, {}, {desc}, {}, {fill}, {cur_lit})",
            t.op,
            src.op,
            u32::from(ch)
        ));
        Some(t)
    }

    fn editw(&mut self, args: &[Expr]) -> Option<V> {
        if !self.bif_arity("%EDITW", args, 2, 2) {
            return None;
        }
        let v = self.expr(&args[0], Want::default())?;
        // The edit word is a character literal.
        if !matches!(&args[1], Expr::Lit(Lit::Char(_), _)) && self.expr(&args[1], Want::default()).is_some_and(|w| !w.ty.is_string()) {
            self.err(ids::BAD_EDIT_WORD, 20, "%EDITW");
            return None;
        }
        let word = self.str_arg(&args[1])?;
        if !v.ty.is_numeric() {
            self.err(ids::BIF_ARGS, 30, "%EDITW");
            return None;
        }
        let src = self.num_place(&v);
        let n = word.ty.str_len().unwrap_or(1).max(1);
        let t = self.tmp(&Ty::Char(n));
        let desc = self.desc_of(&src);
        self.rt_proc(
            "_QRNX_EDITW",
            "($dst: char(1) *VARSIZE, byval $dlen: i32, $src: bytes(1) *VARSIZE, byval $desc: i64, const $word: char(1) *VARSIZE, byval $wlen: i32)",
        );
        let w = self.lit_storage(&word);
        self.b.ins(format!("callb _QRNX_EDITW({}, {n}, {}, {desc}, {w}, {n})", t.op, src.op));
        Some(t)
    }

    /// A built-in function as an assignment target (`%SUBST(x:2:3) =`).
    pub fn bif_target(&mut self, name: &str, args: &[Expr]) -> Option<V> {
        match name {
            "%SUBST" | "%SUBSTR" if args.len() >= 2 => {
                // Through a based view of the characters.
                let base = self.target(&args[0])?;
                let base = self.as_chars(&base);
                let len = base.ty.str_len().unwrap_or(1);
                let (start, n) = match (&args[1], args.get(2)) {
                    (Expr::Lit(Lit::Num(a), _), Some(Expr::Lit(Lit::Num(b), _))) => {
                        (a.parse::<u32>().unwrap_or(1), b.parse::<u32>().unwrap_or(1))
                    }
                    (Expr::Lit(Lit::Num(a), _), None) => {
                        let a = a.parse::<u32>().unwrap_or(1);
                        (a, len + 1 - a.min(len))
                    }
                    _ => {
                        self.err(ids::UNSUPPORTED, 30, "%SUBST");
                        return None;
                    }
                };
                if start == 0 || start + n - 1 > len {
                    self.err(ids::INDEX_RANGE, 30, "%SUBST");
                    return None;
                }
                let p = self.b.reg();
                self.b.ins(format!("{p} = addr {}", base.op));
                let off = if base.ty.is_varying() { start + 1 } else { start - 1 };
                Some(V::place(Ty::Char(n), format!("[{p} +{off}]:char({n})")))
            }
            _ => {
                self.err(ids::NOT_ASSIGNABLE, 30, name.to_string());
                None
            }
        }
    }

    /// A call in an expression, through a prototype: the return value.
    pub fn call_expr(&mut self, id: SymId, steps: &[Step]) -> Option<V> {
        let Some(Kind::Proto(pi)) = self.t.syms[id].kind.clone() else {
            return None;
        };
        let p = self.t.protos[pi].clone();
        // In an expression the procedure must return a value.
        if p.ret.is_none() {
            self.err(ids::CALL_NO_VALUE, 30, p.name.clone());
            return None;
        }
        let args: Vec<Expr> = match steps.first() {
            Some(Step::Args(a)) => a.clone(),
            _ => Vec::new(),
        };
        self.call(&p, &args, true)
    }

    /// A `CALLP` (or implicit call) statement.
    pub fn call_statement(&mut self, e: &Expr) -> Option<()> {
        let Expr::Ref { name, steps, .. } = e else {
            self.err(ids::NO_PROTOTYPE, 30, "");
            return None;
        };
        let Some(id) = self.t.find(self.scope, name) else {
            self.err(ids::NOT_DEFINED, 30, name.clone());
            self.err(ids::UNDEFINED_OPERAND, 30, "");
            return None;
        };
        self.t.refer(id, self.at.line, false);
        let Some(Kind::Proto(pi)) = self.t.syms[id].kind.clone() else {
            self.err(ids::NO_PROTOTYPE, 30, name.clone());
            return None;
        };
        let p = self.t.protos[pi].clone();
        let args: Vec<Expr> = match steps.first() {
            Some(Step::Args(a)) => a.clone(),
            _ => Vec::new(),
        };
        self.call(&p, &args, false).map(|_| ())
    }

    /// An argument for a parameter: its operand text.
    fn argument(&mut self, prm: &Parm, e: &Expr) -> Option<String> {
        if matches!(e, Expr::Special { word, .. } if word == "*OMIT") {
            if !prm.omit {
                self.err(ids::PARM_TYPE, 30, "*OMIT");
            }
            return Some("*OMIT".into());
        }
        let v = self.expr(e, Want::default())?;
        let v = if matches!(prm.ty, Ty::Ds(_)) { v } else { self.as_chars(&v) };
        // Structures LIKEDS the same one are the same type.
        let same_ds = match (&v.ty, &prm.ty) {
            (Ty::Ds(a), Ty::Ds(b)) => self.t.ds[*a].ir_type == self.t.ds[*b].ir_type,
            _ => false,
        };
        let same = (v.ty == prm.ty || same_ds) && v.dim == prm.dim;
        // OPTIONS(*STRING): text passes as the address of a copy ended
        // by a null byte.
        if prm.string && prm.ty == Ty::Ptr && v.ty.is_string() {
            let max = v.ty.str_len().unwrap_or(1) + 1;
            let t = self.tmp(&Ty::VarChar(max, 2));
            self.b.ins(format!("str.cat {}, {}, x\"00\"", t.op, v.op));
            let a = self.b.reg();
            self.b.ins(format!("{a} = addr {}", t.op));
            let r = self.b.reg();
            self.b.ins(format!("{r} = ptradd {a}, 2"));
            return Some(r);
        }
        if prm.value || prm.konst {
            // Converted into a temporary of the parameter's type, unless
            // it is already a place of that type.
            if same && v.kind == Vk::Place && !prm.value {
                return Some(v.op);
            }
            if prm.value && crate::codegen::stmt::is_reg_type(&prm.ty) {
                return match &prm.ty {
                    Ty::Ind => self.ind_reg(&v),
                    Ty::Ptr | Ty::ProcPtr => Some(self.ptr_reg(&v)),
                    Ty::Float(8) => Some(self.float_reg(&v)),
                    _ => {
                        let t = self.tmp(&prm.ty.clone());
                        self.assign(&t, &v, false);
                        let r = self.b.reg();
                        self.b.ins(format!("{r} = load {}", t.op));
                        Some(r)
                    }
                };
            }
            if prm.varsize && v.ty.is_string() && prm.ty.is_string() && v.kind == Vk::Place && !prm.value {
                return Some(v.op);
            }
            let t = self.tmp(&prm.ty.clone());
            self.assign(&t, &v, false);
            return Some(t.op);
        }
        // By reference: the argument must be a place of the type.
        if v.kind != Vk::Place {
            self.err(ids::PARM_TYPE, 30, "");
            return None;
        }
        let varsize_ok = prm.varsize && v.ty.is_string() == prm.ty.is_string();
        if !same && !varsize_ok && prm.ty != Ty::Unknown {
            self.err(ids::PARM_TYPE, 30, "");
            return None;
        }
        Some(v.op)
    }

    /// Generate a call; the return value when `want_value`.
    pub fn call(&mut self, p: &Proto, args: &[Expr], want_value: bool) -> Option<V> {
        let required = p.parms.iter().filter(|x| !x.nopass).count();
        if args.len() < required || args.len() > p.parms.len() {
            self.err(ids::PARM_COUNT, 30, p.name.clone());
            return None;
        }
        let mut ops = Vec::new();
        for (prm, a) in p.parms.iter().zip(args) {
            ops.push(self.argument(prm, a)?);
        }
        let list = ops.join(", ");
        let callee = match self.through_pointer(p) {
            Some(head) => head,
            None => format!("callb {}", self.declare_callee(p)),
        };
        if p.extpgm.is_some() {
            let name = self.declare_callee(p);
            self.b.ins(format!("call {name}({list})"));
            return Some(V::lit(Ty::Unknown, ""));
        }
        match &p.ret {
            Some(rt) if crate::codegen::stmt::is_reg_type(rt) => {
                let r = self.b.reg();
                self.b.ins(format!("{r} = {callee}({list})"));
                let r = if *rt == Ty::Float(4) {
                    let f = self.b.reg();
                    self.b.ins(format!("{f} = cvt.f64 {r}"));
                    return Some(V::reg(Ty::Float(8), f));
                } else {
                    r
                };
                if matches!(rt, Ty::Int(_) | Ty::Uns(_)) {
                    // Integers come back as their own width; keep them in a
                    // temporary of the type.
                    let t = self.tmp(rt);
                    self.b.ins(format!("store {}, {r}", t.op));
                    return Some(t);
                }
                Some(V::reg(rt.clone(), r))
            }
            Some(rt) => {
                let t = self.tmp(rt);
                self.b.ins(format!("{callee}({list}) -> {}", t.op));
                Some(t)
            }
            None => {
                self.b.ins(format!("{callee}({list})"));
                if want_value {
                    self.err(ids::BAD_EXPRESSION_TYPE, 30, p.name.clone());
                    return None;
                }
                Some(V::lit(Ty::Unknown, ""))
            }
        }
    }

    /// A call through the procedure pointer `EXTPROC` names: the
    /// `callpp %p as PROTO` head, the signature declared as a prototype.
    fn through_pointer(&mut self, p: &Proto) -> Option<String> {
        if !p.extproc_word {
            return None;
        }
        let id = self.t.find(self.scope, p.extproc.as_deref()?)?;
        if self.t.syms[id].ty != Ty::ProcPtr {
            return None;
        }
        let place = crate::codegen::place_of(self.t, id);
        let decl = self.declare_callee(&Proto { extproc: Some(format!("_QRNPP_{}", p.name)), extproc_word: false, ..p.clone() });
        if let Some(e) = self.externs.get_mut(&decl) {
            *e = e.replacen("extern proc ", "proto ", 1);
        }
        let r = self.b.reg();
        self.b.ins(format!("{r} = load {place}"));
        Some(format!("callpp {r} as {decl}"))
    }

    /// The IR name of a call's target, declaring an external one.
    fn declare_callee(&mut self, p: &Proto) -> String {
        let params: Vec<String> = p
            .parms
            .iter()
            .enumerate()
            .map(|(i, prm)| {
                let ty = self.t.ir_type(&prm.ty);
                let ty = if prm.dim > 0 { format!("array[{}] of {ty}", prm.dim) } else { ty };
                let mut s = String::new();
                if prm.value {
                    s.push_str("byval ");
                } else if prm.konst {
                    s.push_str("const ");
                }
                s.push_str(&format!("$p{}: {ty}", i + 1));
                if prm.nopass {
                    s.push_str(" *NOPASS");
                }
                if prm.omit {
                    s.push_str(" *OMIT");
                }
                if prm.varsize {
                    s.push_str(" *VARSIZE");
                }
                s
            })
            .collect();
        let ret = p.ret.as_ref().map(|t| format!(" -> {}", self.t.ir_type(t))).unwrap_or_default();
        if let Some(pgm) = &p.extpgm {
            let name = format!("pgm_{}", mangle(&p.name));
            let (lib, obj) = match pgm.split_once('/') {
                Some((l, o)) => (l.to_string(), o.to_string()),
                None => ("*LIBL".to_string(), pgm.clone()),
            };
            self.externs.entry(name.clone()).or_insert_with(|| {
                format!(
                    "extern pgm {name}({}) pgm({}) lib({})",
                    params.join(", "),
                    mangle(&obj.to_ascii_uppercase()),
                    if lib == "*LIBL" { lib.clone() } else { mangle(&lib) }
                )
            });
            return name;
        }
        let ext = p.extproc.clone().unwrap_or_else(|| p.name.clone());
        let ir = mangle(&ext.to_ascii_uppercase());
        if !self.local_procs.contains(&p.name) {
            self.externs
                .entry(ir.clone())
                .or_insert_with(|| format!("extern proc {ir}({}){ret}", params.join(", ")));
        }
        let _ = DatFmt::Iso;
        let _ = Fig::Blanks;
        ir
    }
}

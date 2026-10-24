// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The array, bit and further string built-in functions: the `%LOOKUP`
//! and `%TLOOKUP` families, `%OCCUR`, `%MAXARR`, `%MINARR`, `%BITAND`,
//! `%BITOR`, `%BITXOR`, `%BITNOT`, `%SCANRPL`, `%SCANR`, `%STR`,
//! `%CHARCOUNT`, `%LEFT` and `%RIGHT`.

use rpgfront::ast::{BinOp, Expr, Lit, Step};

use crate::codegen::Gen;
use crate::codegen::strops::Rt;
use crate::codegen::expr::{V, Vk, Want};
use crate::diag::ids;
use crate::types::Ty;

/// What each `%LOOKUP` and `%TLOOKUP` looks for (`qrpgrt::arrays`).
fn wants(name: &str) -> Option<i32> {
    Some(match name.trim_start_matches("%T").trim_start_matches('%').trim_start_matches("LOOKUP") {
        "" => 4,
        "LT" => 2,
        "LE" => 6,
        "GT" => 1,
        "GE" => 5,
        _ => return None,
    })
}

impl Gen<'_> {
    /// A built-in function of this module: `None` when `name` is not
    /// one, else what it gives.
    pub fn bif_ext(&mut self, name: &str, args: &[Expr], w: Want) -> Option<Option<V>> {
        let _ = w;
        Some(match name {
            "%LOOKUP" | "%LOOKUPLT" | "%LOOKUPLE" | "%LOOKUPGT" | "%LOOKUPGE" => self.lookup_bif(name, args),
            "%TLOOKUP" | "%TLOOKUPLT" | "%TLOOKUPLE" | "%TLOOKUPGT" | "%TLOOKUPGE" => self.tlookup_bif(name, args),
            "%OCCUR" => {
                let r = self.occur_bif(args);
                if r.is_none() {
                    self.err(ids::BIF_ARGS, 30, name.to_string());
                }
                r
            }
            "%BITAND" | "%BITOR" | "%BITXOR" | "%BITNOT" => self.bit_bif(name, args),
            "%CHARCOUNT" => {
                let v = self.expr(args.first()?, Want::default())?;
                let v = self.as_chars(&v);
                if v.kind == Vk::Lit {
                    return Some(Some(V::lit(Ty::Packed(10, 0), v.ty.str_len().unwrap_or(0).to_string())));
                }
                let r = self.b.reg();
                self.b.ins(format!("{r} = str.len {}", v.op));
                Some(V::reg(Ty::Int(10), r))
            }
            "%LEFT" | "%RIGHT" => self.left_right(name == "%LEFT", args),
            "%CONCAT" | "%CONCATARR" => self.concat_bif(name, args),
            "%SCANR" => self.scanr_bif(args),
            "%SCANRPL" => self.scanrpl_bif(args),
            "%MAXARR" | "%MINARR" => self.extreme_bif(name, args),
            "%STR" => self.str_bif(args),
            "%SPLIT" | "%LIST" | "%RANGE" => {
                self.err(ids::BAD_EXPRESSION_TYPE, 30, name.to_string());
                None
            }
            _ => return None,
        })
    }

    /// `%LOOKUPxx(arg : array [: start [: count]])`: the index, 0 when
    /// none.
    fn lookup_bif(&mut self, name: &str, args: &[Expr]) -> Option<V> {
        if args.len() < 2 || args.len() > 4 {
            self.err(ids::BIF_ARGS, 30, name.to_string());
            return None;
        }
        let arg = self.expr(&args[0], Want::default())?;
        if let Some(id) = args[1].name().and_then(|n| self.t.find(self.scope, n))
            && self.t.syms[id].dim == 0
        {
            self.err(ids::LOOKUP_ARRAY, 20, name.to_string());
            return None;
        }
        let mut r = self.range(&args[1])?;
        // Searching for other than equal needs a sequenced array.
        if matches!(name, "%LOOKUPLT" | "%LOOKUPLE" | "%LOOKUPGT" | "%LOOKUPGE")
            && self.t.syms[r.sym].seq == 0
            && !matches!(self.t.syms[r.sym].ty, Ty::Ds(_))
        {
            self.err(ids::LOOKUP_UNSEQUENCED, 20, name.to_string());
            return None;
        }
        if let Some(s) = args.get(2) {
            let s = self.expr(s, Want::default())?;
            let s = self.int_reg(&s)?;
            let z = self.b.reg();
            self.b.ins(format!("{z} = sub {s}, 1"));
            r.start.clone_from(&z);
            r.count = match args.get(3) {
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
        }
        let res = self.search(&arg, &r, wants(name)?)?;
        let idx = self.b.reg();
        self.b.ins(format!("{idx} = and {res}, 4294967295"));
        let i32r = self.b.reg();
        self.b.ins(format!("{i32r} = cvt.i32 {idx}"));
        Some(V::reg(Ty::Int(10), i32r))
    }

    /// `%TLOOKUPxx(arg : table [: alternate])`: whether found; the
    /// tables' current elements move to it.
    fn tlookup_bif(&mut self, name: &str, args: &[Expr]) -> Option<V> {
        if args.len() < 2 || args.len() > 3 {
            self.err(ids::BIF_ARGS, 30, name.to_string());
            return None;
        }
        let arg = self.expr(&args[0], Want::default())?;
        let r = self.range(&args[1])?;
        // Only %TLOOKUP searches a table in no order.
        if name != "%TLOOKUP" && self.t.syms[r.sym].seq == 0 {
            self.err(ids::TLOOKUP_SEQ, 20, name.to_string());
            return None;
        }
        let res = self.search(&arg, &r, wants(name)?)?;
        let found = self.b.reg();
        self.b.ins(format!("{found} = cmp.ne {res}, 0"));
        let set = self.b.label();
        let done = self.b.label();
        self.b.term(format!("brif {found}, {set}, {done}"));
        self.b.place(&set);
        let idx = self.b.reg();
        self.b.ins(format!("{idx} = and {res}, 4294967295"));
        let z = self.b.reg();
        self.b.ins(format!("{z} = sub {idx}, 1"));
        let mut tables = vec![r.sym];
        if let Some(a) = args.get(2)
            && let Some(alt) = self.range(a)
        {
            tables.push(alt.sym);
        }
        for id in tables {
            if self.t.syms[id].table {
                let ix = self.tix(id);
                self.b.ins(format!("store {ix}, {z}"));
            }
        }
        self.b.place(&done);
        Some(V::reg(Ty::Ind, found))
    }

    /// `%BITAND`, `%BITOR`, `%BITXOR` (two or more arguments) and
    /// `%BITNOT` (one), on integers or on character data byte by byte.
    fn bit_bif(&mut self, name: &str, args: &[Expr]) -> Option<V> {
        let not = name == "%BITNOT";
        if not && args.len() != 1 || !not && args.len() < 2 {
            self.err(ids::BIF_ARGS, 30, name.to_string());
            return None;
        }
        let mut vals = Vec::new();
        for a in args {
            let v = self.expr(a, Want::default())?;
            vals.push(self.as_chars(&v));
        }
        if vals.iter().all(|v| v.ty.is_char()) {
            return Some(self.bit_chars(name, &vals));
        }
        if !vals.iter().all(|v| v.ty.is_integer() || v.kind == Vk::Lit && v.ty.shape().is_some_and(|s| s.1 == 0)) {
            self.err(ids::BIF_ARGS, 30, name.to_string());
            return None;
        }
        // The result: unsigned when every typed argument is, as wide as
        // the widest.
        let typed: Vec<&V> = vals.iter().filter(|v| v.kind != Vk::Lit).collect();
        let uns = !typed.is_empty() && typed.iter().all(|v| matches!(v.ty, Ty::Uns(_)));
        let digits = typed.iter().map(|v| v.ty.shape().map_or(10, |s| s.0)).max().unwrap_or(10);
        let mut acc = self.int_reg(&vals[0])?;
        if not {
            let r = self.b.reg();
            self.b.ins(format!("{r} = not {acc}"));
            acc = r;
        }
        for v in &vals[1..] {
            let b = self.int_reg(v)?;
            let r = self.b.reg();
            let o = match name {
                "%BITAND" => "and",
                "%BITOR" => "or",
                _ => "xor",
            };
            self.b.ins(format!("{r} = {o} {acc}, {b}"));
            acc = r;
        }
        let ty = if uns { Ty::Uns(digits) } else { Ty::Int(digits) };
        let t = self.b.temp(&self.ir_ty(&ty));
        let n = self.b.reg();
        let bits = match digits {
            3 => 8,
            5 => 16,
            10 => 32,
            _ => 64,
        };
        let k = format!("{}{bits}", if uns { 'u' } else { 'i' });
        self.b.ins(format!("{n} = cvt.{k} {acc}"));
        self.b.ins(format!("store {t}, {n}"));
        Some(V::place(ty, t))
    }

    /// The bit functions on character data, byte by byte.
    fn bit_chars(&mut self, name: &str, vals: &[V]) -> V {
        let n = vals.iter().map(|v| v.ty.str_len().unwrap_or(1)).min().unwrap_or(1).max(1);
        let t = self.b.temp(&format!("char({n})"));
        let mut srcs = Vec::new();
        for v in vals {
            let r = self.rt_of(v, None);
            srcs.push(r.op);
        }
        for i in 0..n {
            let p = self.b.reg();
            self.b.ins(format!("{p} = addr {}", srcs[0]));
            let mut acc = self.b.reg();
            self.b.ins(format!("{acc} = load [{p} +{i}]:u8"));
            if name == "%BITNOT" {
                let r = self.b.reg();
                self.b.ins(format!("{r} = not {acc}"));
                acc = r;
            }
            for s in &srcs[1..] {
                let q = self.b.reg();
                self.b.ins(format!("{q} = addr {s}"));
                let b = self.b.reg();
                self.b.ins(format!("{b} = load [{q} +{i}]:u8"));
                let r = self.b.reg();
                let o = match name {
                    "%BITAND" => "and",
                    "%BITOR" => "or",
                    _ => "xor",
                };
                self.b.ins(format!("{r} = {o} {acc}, {b}"));
                acc = r;
            }
            let d = self.b.reg();
            self.b.ins(format!("{d} = addr {t}"));
            self.b.ins(format!("store [{d} +{i}]:u8, {acc}"));
        }
        V::place(Ty::Char(n), t)
    }

    /// `%LEFT(s : n)` and `%RIGHT(s : n)`.
    fn left_right(&mut self, left: bool, args: &[Expr]) -> Option<V> {
        if args.len() != 2 {
            self.err(ids::BIF_ARGS, 30, if left { "%LEFT" } else { "%RIGHT" });
            return None;
        }
        let s = self.expr(&args[0], Want::default())?;
        let s = self.as_chars(&s);
        let nv = self.expr(&args[1], Want::default())?;
        let n = self.int_reg(&nv)?;
        let len = s.ty.str_len().unwrap_or(1).max(1);
        let t = self.b.temp(&format!("varchar({len})"));
        let start = if left {
            "1".to_string()
        } else {
            let l = self.b.reg();
            self.b.ins(format!("{l} = str.len {}", s.op));
            let l64 = self.b.reg();
            self.b.ins(format!("{l64} = cvt.i64 {l}"));
            let a = self.b.reg();
            self.b.ins(format!("{a} = sub {l64}, {n}"));
            let b = self.b.reg();
            self.b.ins(format!("{b} = add {a}, 1"));
            b
        };
        let src = self.rt_of(&s, None);
        self.b.ins(format!("str.subst {t}, {}, {start}, {n}", src.op));
        Some(V::place(Ty::varchar(len), t))
    }

    /// `%CONCAT(sep : a : b ...)` and `%CONCATARR(sep : array)`: the
    /// values joined by the separator.
    fn concat_bif(&mut self, name: &str, args: &[Expr]) -> Option<V> {
        let at = self.at;
        let (sep, items): (Expr, Vec<Expr>) = if name == "%CONCAT" {
            if args.len() < 2 {
                self.err(ids::BIF_ARGS, 30, name.to_string());
                return None;
            }
            (args[0].clone(), args[1..].to_vec())
        } else {
            let (Some(sep), Some(Expr::Ref { name: an, steps, at: aat })) = (args.first(), args.get(1)) else {
                self.err(ids::BIF_ARGS, 30, name.to_string());
                return None;
            };
            let dim = self.t.find(self.scope, an).map_or(0, |id| self.t.syms[id].dim);
            if dim == 0 || !steps.is_empty() {
                self.err(ids::BIF_ARGS, 30, name.to_string());
                return None;
            }
            let items = (1..=dim)
                .map(|i| Expr::Ref {
                    name: an.clone(),
                    steps: vec![Step::Args(vec![Expr::Lit(Lit::Num(i.to_string()), *aat)])],
                    at: *aat,
                })
                .collect();
            (sep.clone(), items)
        };
        let mut e = items[0].clone();
        for it in &items[1..] {
            let with = Expr::Binary(BinOp::Add, Box::new(e), Box::new(sep.clone()), at);
            e = Expr::Binary(BinOp::Add, Box::new(with), Box::new(it.clone()), at);
        }
        let v = self.expr(&e, Want::default())?;
        let n = v.ty.str_len().unwrap_or(1).max(1);
        let t = self.b.temp(&format!("varchar({n})"));
        let tv = V::place(Ty::varchar(n), t);
        self.assign(&tv, &v, false);
        Some(tv)
    }

    /// A string argument as storage.
    fn str_rt(&mut self, e: &Expr) -> Option<(Rt, V)> {
        let v = self.expr(e, Want::default())?;
        let v = self.as_chars(&v);
        if !v.ty.is_char() && v.fig.is_none() {
            self.err(ids::BIF_ARGS, 30, "");
            return None;
        }
        Some((self.rt_of(&v, None), v))
    }

    /// An optional integer argument, or a default.
    fn opt_int(&mut self, e: Option<&Expr>, dft: i64) -> Option<String> {
        match e {
            Some(e) => {
                let v = self.expr(e, Want::default())?;
                self.int_reg(&v)
            }
            None => Some(dft.to_string()),
        }
    }

    /// `%SCANR(search : source [: start [: length]])`.
    fn scanr_bif(&mut self, args: &[Expr]) -> Option<V> {
        if args.len() < 2 || args.len() > 4 {
            self.err(ids::BIF_ARGS, 30, "%SCANR");
            return None;
        }
        let (n, _) = self.str_rt(&args[0])?;
        let (h, _) = self.str_rt(&args[1])?;
        let st = self.opt_int(args.get(2), 1)?;
        let ln = self.opt_int(args.get(3), -1)?;
        self.rt_proc(
            "_QRNX_SCANR",
            "($n: bytes(1) *VARSIZE, byval $nd: i64, $h: bytes(1) *VARSIZE, byval $hd: i64, byval $st: i64, byval $ln: i64) -> i32",
        );
        let r = self.b.reg();
        self.b.ins(format!("{r} = callb _QRNX_SCANR({}, {}, {}, {}, {st}, {ln})", n.op, n.desc, h.op, h.desc));
        Some(V::reg(Ty::Int(10), r))
    }

    /// `%SCANRPL(search : replacement : source [: start [: length]])`.
    fn scanrpl_bif(&mut self, args: &[Expr]) -> Option<V> {
        if args.len() < 3 || args.len() > 5 {
            self.err(ids::BIF_ARGS, 30, "%SCANRPL");
            return None;
        }
        let (s, sv) = self.str_rt(&args[0])?;
        let (r, rv) = self.str_rt(&args[1])?;
        let (src, srcv) = self.str_rt(&args[2])?;
        let st = self.opt_int(args.get(3), 1)?;
        let ln = self.opt_int(args.get(4), -1)?;
        let sl = sv.ty.str_len().unwrap_or(1).max(1);
        let rl = rv.ty.str_len().unwrap_or(0);
        let n = srcv.ty.str_len().unwrap_or(1).max(1);
        let cap = if rl > sl { n / sl * rl + n % sl + 1 } else { n }.clamp(1, 65_535);
        let t = self.b.temp(&format!("varchar({cap})"));
        self.rt_proc(
            "_QRNX_SCANRPL",
            "($d: bytes(1) *VARSIZE, byval $cap: i64, $s: bytes(1) *VARSIZE, byval $sd: i64, $r: bytes(1) *VARSIZE, byval $rd: i64, $src: bytes(1) *VARSIZE, byval $srcd: i64, byval $st: i64, byval $ln: i64)",
        );
        self.b.ins(format!(
            "callb _QRNX_SCANRPL({t}, {cap}, {}, {}, {}, {}, {}, {}, {st}, {ln})",
            s.op, s.desc, r.op, r.desc, src.op, src.desc
        ));
        Some(V::place(Ty::varchar(cap), t))
    }

    /// `%MAXARR(array [: start [: count]])` and `%MINARR`.
    fn extreme_bif(&mut self, name: &str, args: &[Expr]) -> Option<V> {
        if args.is_empty() || args.len() > 3 {
            self.err(ids::BIF_ARGS, 30, name.to_string());
            return None;
        }
        let mut r = self.range(&args[0])?;
        if let Some(s) = args.get(1) {
            let s = self.expr(s, Want::default())?;
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
        }
        let ed = self.desc_of(&V::place(r.ty.clone(), String::new()));
        self.rt_proc(
            "_QRNX_EXTREME",
            "($arr: bytes(1) *VARSIZE, byval $ed: i64, byval $st: i64, byval $n: i64, byval $stride: i64, byval $off: i64, byval $max: i32) -> i64",
        );
        let res = self.b.reg();
        self.b.ins(format!(
            "{res} = callb _QRNX_EXTREME({}, {ed}, {}, {}, {}, {}, {})",
            r.base,
            r.start,
            r.count,
            r.stride,
            r.keyoff,
            i32::from(name == "%MAXARR")
        ));
        let i = self.b.reg();
        self.b.ins(format!("{i} = cvt.i32 {res}"));
        Some(V::reg(Ty::Int(10), i))
    }

    /// `%STR(pointer [: maximum])`: the bytes up to the first null.
    fn str_bif(&mut self, args: &[Expr]) -> Option<V> {
        let p = self.expr(args.first()?, Want::default())?;
        if p.ty != Ty::Ptr {
            self.err(ids::BIF_ARGS, 30, "%STR");
            return None;
        }
        let cap: u32 = match args.get(1) {
            Some(Expr::Lit(Lit::Num(n), _)) => n.parse().unwrap_or(65_535),
            Some(_) => {
                self.err(ids::UNSUPPORTED, 30, "%STR");
                return None;
            }
            None => 65_535,
        };
        let r = self.ptr_reg(&p);
        let t = self.b.temp(&format!("varchar({cap})"));
        self.rt_proc("_QRNX_STRGET", "($d: bytes(1) *VARSIZE, byval $cap: i64, $s: bytes(1) *VARSIZE)");
        self.b.ins(format!("callb _QRNX_STRGET({t}, {cap}, [{r}]:char({cap}))"));
        Some(V::place(Ty::varchar(cap), t))
    }

    /// `%STR(pointer : maximum) = value`.
    pub fn str_assign(&mut self, args: &[Expr], v: &Expr) -> Option<()> {
        let p = self.expr(args.first()?, Want::default())?;
        let cap: u32 = match args.get(1) {
            Some(Expr::Lit(Lit::Num(n), _)) => n.parse().ok()?,
            _ => {
                self.err(ids::BIF_ARGS, 30, "%STR");
                return None;
            }
        };
        let (src, _) = self.str_rt(v)?;
        let r = self.ptr_reg(&p);
        self.rt_proc("_QRNX_STRSET", "($d: bytes(1) *VARSIZE, byval $cap: i64, $s: bytes(1) *VARSIZE, byval $sd: i64)");
        self.b.ins(format!("callb _QRNX_STRSET([{r}]:char({cap}), {cap}, {}, {})", src.op, src.desc));
        Some(())
    }

    /// `x IN %LIST(a : b ...)` and `x IN %RANGE(low : high)`.
    pub fn in_op(&mut self, a: &Expr, b: &Expr) -> Option<V> {
        let Expr::Bif { name, args, .. } = b else {
            self.err(ids::BAD_EXPRESSION_TYPE, 30, "IN");
            return None;
        };
        let x = self.expr(a, Want::default())?;
        let x = self.as_chars(&x);
        // Evaluated once.
        let x = if x.kind == Vk::Place {
            x
        } else {
            let ty = match &x.ty {
                Ty::Ind => Ty::Ind,
                t if t.is_numeric() => match x.kind {
                    Vk::Reg if t.is_integer() => Ty::Int(20),
                    _ => t.clone(),
                },
                t => Ty::Char(t.str_len().unwrap_or(1).max(1)),
            };
            let tmp = self.b.temp(&self.ir_ty(&ty));
            let tv = V::place(ty, tmp);
            self.assign(&tv, &x, false);
            tv
        };
        match name.as_str() {
            "%LIST" => {
                let t = self.b.temp("ind");
                self.b.ins(format!("store {t}, *OFF"));
                for e in args {
                    let v = self.expr(e, Want::default())?;
                    let c = self.compare(BinOp::Eq, &x, &v)?;
                    let cur = self.b.reg();
                    self.b.ins(format!("{cur} = load {t}"));
                    let o = self.b.reg();
                    self.b.ins(format!("{o} = select {cur}, {cur}, {}", c.op));
                    self.b.ins(format!("store {t}, {o}"));
                }
                let r = self.b.reg();
                self.b.ins(format!("{r} = load {t}"));
                Some(V::reg(Ty::Ind, r))
            }
            "%RANGE" if args.len() == 2 => {
                let lo = self.expr(&args[0], Want::default())?;
                let hi = self.expr(&args[1], Want::default())?;
                let c1 = self.compare(BinOp::Ge, &x, &lo)?;
                let c2 = self.compare(BinOp::Le, &x, &hi)?;
                let r = self.b.reg();
                self.b.ins(format!("{r} = select {}, {}, {}", c1.op, c2.op, c1.op));
                Some(V::reg(Ty::Ind, r))
            }
            _ => {
                self.err(ids::BAD_EXPRESSION_TYPE, 30, "IN");
                None
            }
        }
    }
}

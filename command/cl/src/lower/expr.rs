// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Expressions into IR: character and numeric values are places or
//! literals, logical values are indicator registers. Operators build
//! temporaries sized by the operand types; the built-in functions map
//! to the string and conversion instructions.

use std::fmt::Write as _;

use super::Gen;
use crate::ast::{Expr, Lit, Op, VarType};
use crate::sem::{Ty, Var, type_of};

/// A lowered value.
#[derive(Clone, Debug)]
pub enum Val {
    /// A place or a literal of a storage type.
    Storage(String, Ty),
    /// An indicator register.
    Ind(String),
}

/// A CL string as an IR string literal.
#[must_use]
pub fn qstr(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}

/// Bytes as an IR hex literal.
#[must_use]
pub fn qhex(b: &[u8]) -> String {
    let mut s = String::from("x\"");
    for x in b {
        let _ = write!(s, "{x:02X}");
    }
    s.push('"');
    s
}

impl Gen<'_, '_> {
    /// The type of an expression.
    pub(super) fn ty(&self, e: &Expr) -> Result<Ty, String> {
        type_of(self.t, e)
    }

    /// A numeric operand: a place or a literal.
    pub(super) fn num(&mut self, e: &Expr) -> Result<String, String> {
        match self.value(e)? {
            Val::Storage(s, Ty::Dec(..)) => Ok(s),
            Val::Storage(s, Ty::Char(_)) => {
                // Character data where a number is needed: converted.
                let t = self.temp(&Ty::Dec(15, 5));
                self.ins(&format!("cvt {t}, {s}"));
                Ok(t)
            }
            _ => Err("a number is needed".into()),
        }
    }

    /// A character operand: a place or a literal.
    pub(super) fn chr(&mut self, e: &Expr) -> Result<String, String> {
        match self.value(e)? {
            Val::Storage(s, Ty::Char(_)) => Ok(s),
            Val::Storage(s, Ty::Dec(d, sc)) => {
                // Exact length, as %CHAR gives it: no padding.
                let t = self.temp_varying(u32::from(d) + 2 + u32::from(sc > 0));
                self.ins(&format!("cvt {t}, {s}"));
                Ok(t)
            }
            Val::Ind(r) => {
                let t = self.temp(&Ty::Char(1));
                let p = self.temp(&Ty::Lgl);
                self.ins(&format!("store {p}, {r}"));
                self.ins(&format!("cvt {t}, {p}"));
                Ok(t)
            }
            Val::Storage(_, Ty::Ptr) => Err("a pointer is not character data".into()),
            Val::Storage(_, Ty::Lgl) => Err("a logical value is not character data".into()),
        }
    }

    /// A logical operand: an indicator register.
    pub(super) fn lgl(&mut self, e: &Expr) -> Result<String, String> {
        match self.value(e)? {
            Val::Ind(r) => Ok(r),
            Val::Storage(s, Ty::Lgl) => {
                let r = self.reg("b");
                self.ins(&format!("{r} = load {s}"));
                Ok(r)
            }
            Val::Storage(s, Ty::Char(1)) if s == "\"1\"" || s == "\"0\"" => {
                let r = self.reg("b");
                self.ins(&format!(
                    "{r} = const.ind {}",
                    if s == "\"1\"" { "*ON" } else { "*OFF" }
                ));
                Ok(r)
            }
            Val::Storage(..) => Err("a logical value is needed".into()),
        }
    }

    /// Lower an expression.
    pub(super) fn value(&mut self, e: &Expr) -> Result<Val, String> {
        Ok(match e {
            Expr::Lit(Lit::Str(s)) => Val::Storage(
                qstr(s),
                Ty::Char(u32::try_from(s.chars().count().max(1)).unwrap_or(1)),
            ),
            Expr::Lit(Lit::Hex(b)) => Val::Storage(
                qhex(b),
                Ty::Char(u32::try_from(b.len().max(1)).unwrap_or(1)),
            ),
            Expr::Lit(Lit::Num(n)) => {
                let d = qdecimal::Decimal::parse(n).map_err(|_| format!("{n} is not a number"))?;
                Val::Storage(d.to_string(), Ty::Dec(d.digits_needed().max(1), d.scale()))
            }
            Expr::Word(w) => Val::Storage(
                qstr(w),
                Ty::Char(u32::try_from(w.chars().count().max(1)).unwrap_or(1)),
            ),
            Expr::Var(v) => {
                let var = self
                    .t
                    .var(v)
                    .ok_or_else(|| format!("&{v} is not declared"))?;
                match var.ty {
                    VarType::Lgl => {
                        let r = self.reg("b");
                        self.ins(&format!("{r} = load {}", var.place()));
                        Val::Ind(r)
                    }
                    _ => Val::Storage(var.place(), var.ty()),
                }
            }
            Expr::Neg(a) => {
                let ty = self.ty(e)?;
                let src = self.num(a)?;
                let t = self.temp(&ty);
                self.ins(&format!("dec.neg {t}, {src}"));
                Val::Storage(t, ty)
            }
            Expr::Not(a) => {
                let r = self.lgl(a)?;
                let n = self.reg("n");
                self.ins(&format!("{n} = cmp.eq {r}, *OFF"));
                Val::Ind(n)
            }
            Expr::Bin(op, a, b) => self.binary(*op, a, b)?,
            Expr::Builtin(name, args) => self.builtin(name, args)?,
        })
    }

    fn binary(&mut self, op: Op, a: &Expr, b: &Expr) -> Result<Val, String> {
        let ty = self.ty(&Expr::Bin(op, Box::new(a.clone()), Box::new(b.clone())))?;
        match op {
            Op::Add | Op::Sub | Op::Mul | Op::Div => {
                let (l, r) = (self.num(a)?, self.num(b)?);
                let Ty::Dec(d, s) = ty else { unreachable!() };
                let t = self.temp(&ty);
                let name = match op {
                    Op::Add => "add",
                    Op::Sub => "sub",
                    Op::Mul => "mul",
                    _ => "div",
                };
                self.ins(&format!("dec.{name} {t}, {l}, {r} prec({d},{s})"));
                Ok(Val::Storage(t, ty))
            }
            Op::Cat | Op::Bcat | Op::Tcat => {
                let (l, r) = (self.chr(a)?, self.chr(b)?);
                // The result keeps the operands' lengths as they are:
                // a variable's padding, a literal's or a built-in's
                // exact text.
                let t = self.temp_varying(match ty {
                    Ty::Char(n) => n,
                    _ => 1,
                });
                if op == Op::Cat {
                    self.ins(&format!("str.cat {t}, {l}, {r}"));
                } else {
                    // The left operand without its trailing blanks, in a
                    // varying temp whose length the trim sets; *BCAT
                    // puts one blank after it.
                    let lt = match self.ty(a)? {
                        Ty::Char(n) => n,
                        _ => 1,
                    };
                    let v = self.temp_varying(lt + 1);
                    self.ins(&format!("str.trimr {v}, {l}"));
                    if op == Op::Bcat {
                        let w = self.temp_varying(lt + 1);
                        self.ins(&format!("str.cat {w}, {v}, \" \""));
                        self.ins(&format!("str.cat {t}, {w}, {r}"));
                    } else {
                        self.ins(&format!("str.cat {t}, {v}, {r}"));
                    }
                }
                Ok(Val::Storage(t, ty))
            }
            Op::Eq | Op::Ne | Op::Gt | Op::Lt | Op::Ge | Op::Le | Op::Ng | Op::Nl => {
                let cond = match op {
                    Op::Eq => "eq",
                    Op::Ne => "ne",
                    Op::Gt => "gt",
                    Op::Lt => "lt",
                    Op::Ge | Op::Nl => "ge",
                    _ => "le",
                };
                let r = self.reg("c");
                let (ta, tb) = (self.ty(a)?, self.ty(b)?);
                let ta = if tb == Ty::Lgl { Ty::Lgl } else { ta };
                match ta {
                    Ty::Char(_) => {
                        let (l, rr) = (self.chr(a)?, self.chr(b)?);
                        self.ins(&format!("{r} = str.cmp.{cond} {l}, {rr}"));
                    }
                    Ty::Dec(..) => {
                        let (l, rr) = (self.num(a)?, self.num(b)?);
                        self.ins(&format!("{r} = dec.cmp.{cond} {l}, {rr}"));
                    }
                    Ty::Lgl => {
                        let (l, rr) = (self.lgl(a)?, self.lgl(b)?);
                        self.ins(&format!("{r} = cmp.{cond} {l}, {rr}"));
                    }
                    Ty::Ptr => {
                        let (l, rr) = (self.ptr(a)?, self.ptr(b)?);
                        self.ins(&format!("{r} = cmp.{cond} {l}, {rr}"));
                    }
                }
                Ok(Val::Ind(r))
            }
            Op::And | Op::Or => {
                let l = self.lgl(a)?;
                let out = self.reg("l");
                let second = self.fresh("A");
                let yes = self.fresh("Y");
                let no = self.fresh("Z");
                let join = self.fresh("J");
                if op == Op::And {
                    self.term(&format!("brif {l}, {second}, {no}"));
                } else {
                    self.term(&format!("brif {l}, {yes}, {second}"));
                }
                self.label(&second);
                let r = self.lgl(b)?;
                self.term(&format!("brif {r}, {yes}, {no}"));
                self.label(&yes);
                self.ins(&format!("{out} = const.ind *ON"));
                self.term(&format!("br {join}"));
                self.label(&no);
                self.ins(&format!("{out} = const.ind *OFF"));
                self.term(&format!("br {join}"));
                self.label(&join);
                Ok(Val::Ind(out))
            }
        }
    }

    /// A pointer register.
    pub(super) fn ptr(&mut self, e: &Expr) -> Result<String, String> {
        match e {
            Expr::Var(v) => {
                let var = self
                    .t
                    .var(v)
                    .ok_or_else(|| format!("&{v} is not declared"))?;
                if var.ty != VarType::Ptr {
                    return Err(format!("&{v} is not a pointer"));
                }
                let r = self.reg("p");
                self.ins(&format!("{r} = load {}", var.place()));
                Ok(r)
            }
            Expr::Builtin(n, args) if n == "ADDRESS" => {
                let Some(Expr::Var(v)) = args.first() else {
                    return Err("%ADDRESS needs a variable".into());
                };
                let var = self
                    .t
                    .var(v)
                    .ok_or_else(|| format!("&{v} is not declared"))?;
                let r = self.reg("p");
                self.ins(&format!("{r} = addr {}", var.place()));
                Ok(r)
            }
            Expr::Word(w) if w == "*NULL" => {
                let r = self.reg("p");
                self.ins(&format!("{r} = const.ptr *NULL"));
                Ok(r)
            }
            _ => Err("a pointer is needed".into()),
        }
    }

    /// An integer register from a numeric expression.
    pub(super) fn int(&mut self, e: &Expr) -> Result<String, String> {
        if let Expr::Lit(Lit::Num(n)) = e
            && n.parse::<i64>().is_ok()
        {
            return Ok(n.clone());
        }
        let p = self.num(e)?;
        let r = self.reg("i");
        self.ins(&format!("{r} = cvt.i32 {p}"));
        Ok(r)
    }

    fn builtin(&mut self, name: &str, args: &[Expr]) -> Result<Val, String> {
        Ok(match name {
            "SST" | "SUBSTRING" => {
                let ty = self.ty(&Expr::Builtin(name.into(), args.to_vec()))?;
                let src = self.chr(&args[0])?;
                let start = self.int(&args[1])?;
                let len = self.int(&args[2])?;
                let t = self.temp_varying(match ty {
                    Ty::Char(n) => n,
                    _ => 1,
                });
                self.ins(&format!("str.subst {t}, {src}, {start}, {len}"));
                Val::Storage(t, ty)
            }
            "BIN" | "BINARY" => {
                let Expr::Var(v) = &args[0] else {
                    return Err("%BIN needs a variable".into());
                };
                let var = self
                    .t
                    .var(v)
                    .ok_or_else(|| format!("&{v} is not declared"))?
                    .clone();
                let (start, len) = if args.len() == 3 {
                    (
                        self.int(&args[1])?,
                        match &args[2] {
                            Expr::Lit(Lit::Num(n)) => n.clone(),
                            _ => return Err("%BIN takes a constant length of 2 or 4".into()),
                        },
                    )
                } else {
                    let whole = if var.len == 4 { "4" } else { "2" };
                    ("1".into(), whole.into())
                };
                let p = self.reg("p");
                self.ins(&format!("{p} = addr {}", var.place()));
                let q = self.reg("q");
                self.ins(&format!("{q} = ptradd {p}, {start}"));
                let q2 = self.reg("q");
                self.ins(&format!("{q2} = ptradd {q}, -1"));
                let ity = if len == "4" { "i32" } else { "i16" };
                let v = self.reg("v");
                self.ins(&format!("{v} = load [{q2}]:{ity}"));
                let t = self.temp(&Ty::Dec(10, 0));
                self.ins(&format!("dec.mov {t}, {v}"));
                Val::Storage(t, Ty::Dec(10, 0))
            }
            "CHAR" => {
                let s = self.chr(&args[0])?;
                let ty = self.ty(&Expr::Builtin(name.into(), args.to_vec()))?;
                Val::Storage(s, ty)
            }
            "DEC" => {
                let ty = self.ty(&Expr::Builtin(name.into(), args.to_vec()))?;
                let t = self.temp(&ty);
                match self.value(&args[0])? {
                    Val::Storage(s, Ty::Char(_)) => self.ins(&format!("cvt {t}, {s}")),
                    Val::Storage(s, Ty::Dec(..)) => self.ins(&format!("dec.mov {t}, {s}")),
                    _ => return Err("%DEC takes a number or character data".into()),
                }
                Val::Storage(t, ty)
            }
            "INT" | "UINT" => {
                let t = self.temp(&Ty::Dec(10, 0));
                match self.value(&args[0])? {
                    Val::Storage(s, Ty::Char(_)) => self.ins(&format!("cvt {t}, {s}")),
                    Val::Storage(s, Ty::Dec(..)) => self.ins(&format!("dec.mov {t}, {s}")),
                    _ => return Err("%INT takes a number or character data".into()),
                }
                Val::Storage(t, Ty::Dec(10, 0))
            }
            "LEN" => {
                let s = self.chr(&args[0])?;
                let r = self.reg("n");
                self.ins(&format!("{r} = str.len {s}"));
                let t = self.temp(&Ty::Dec(10, 0));
                self.ins(&format!("dec.mov {t}, {r}"));
                Val::Storage(t, Ty::Dec(10, 0))
            }
            "SIZE" => {
                let Expr::Var(v) = &args[0] else {
                    return Err("%SIZE needs a variable".into());
                };
                let var = self
                    .t
                    .var(v)
                    .ok_or_else(|| format!("&{v} is not declared"))?;
                let n = match var.ty {
                    VarType::Char => var.len,
                    VarType::Dec => var.len / 2 + 1,
                    VarType::Lgl => 1,
                    VarType::Int | VarType::Uint => var.len,
                    VarType::Ptr => 16,
                };
                Val::Storage(n.to_string(), Ty::Dec(10, 0))
            }
            "SCAN" => {
                let needle = self.chr(&args[0])?;
                let hay = self.chr(&args[1])?;
                let r = self.reg("n");
                if args.len() == 3 {
                    let st = self.int(&args[2])?;
                    self.ins(&format!("{r} = str.scan {needle}, {hay}, {st}"));
                } else {
                    self.ins(&format!("{r} = str.scan {needle}, {hay}"));
                }
                let t = self.temp(&Ty::Dec(10, 0));
                self.ins(&format!("dec.mov {t}, {r}"));
                Val::Storage(t, Ty::Dec(10, 0))
            }
            "CHECK" | "CHECKR" => {
                let set = self.chr(&args[0])?;
                let src = self.chr(&args[1])?;
                let r = self.reg("n");
                let f = if name == "CHECK" { "check" } else { "checkr" };
                if args.len() == 3 {
                    let st = self.int(&args[2])?;
                    self.ins(&format!("{r} = str.{f} {set}, {src}, {st}"));
                } else {
                    self.ins(&format!("{r} = str.{f} {set}, {src}"));
                }
                let t = self.temp(&Ty::Dec(10, 0));
                self.ins(&format!("dec.mov {t}, {r}"));
                Val::Storage(t, Ty::Dec(10, 0))
            }
            "TRIM" | "TRIML" | "TRIMR" => {
                let ty = self.ty(&args[0])?;
                let src = self.chr(&args[0])?;
                let t = self.temp_varying(match ty {
                    Ty::Char(n) => n,
                    _ => 1,
                });
                let f = match name {
                    "TRIM" => "trim",
                    "TRIML" => "triml",
                    _ => "trimr",
                };
                if args.len() == 2 {
                    let chars = self.chr(&args[1])?;
                    self.ins(&format!("str.{f} {t}, {src}, {chars}"));
                } else {
                    self.ins(&format!("str.{f} {t}, {src}"));
                }
                Val::Storage(t, ty)
            }
            "UPPER" | "LOWER" => {
                let ty = self.ty(&args[0])?;
                let src = self.chr(&args[0])?;
                let t = self.temp(&ty);
                let (from, to) = if name == "UPPER" {
                    ("abcdefghijklmnopqrstuvwxyz", "ABCDEFGHIJKLMNOPQRSTUVWXYZ")
                } else {
                    ("ABCDEFGHIJKLMNOPQRSTUVWXYZ", "abcdefghijklmnopqrstuvwxyz")
                };
                self.ins(&format!("str.xlate {t}, \"{from}\", \"{to}\", {src}"));
                Val::Storage(t, ty)
            }
            "PARMS" => {
                let r = self.reg("n");
                self.ins(&format!("{r} = parms"));
                let t = self.temp(&Ty::Dec(10, 0));
                self.ins(&format!("dec.mov {t}, {r}"));
                Val::Storage(t, Ty::Dec(10, 0))
            }
            "SWITCH" => {
                // The job switches, as `QCLSWS` reads them: a mask of
                // 0, 1 and X against the eight switches.
                let mask = match &args[0] {
                    Expr::Lit(Lit::Str(s) | Lit::Num(s)) | Expr::Word(s) => {
                        s.to_ascii_uppercase()
                    }
                    _ => return Err("%SWITCH takes a mask of 0, 1 and X".into()),
                };
                if mask.len() != 8 || !mask.chars().all(|c| matches!(c, '0' | '1' | 'X')) {
                    return Err("%SWITCH takes a mask of eight 0, 1 and X".into());
                }
                let m = self.temp(&Ty::Char(8));
                self.ins(&format!("mov {m}, {}", qstr(&mask)));
                let out = self.temp(&Ty::Lgl);
                self.ins(&format!("call QSYS/QCLSWS({m}, {out})"));
                let r = self.reg("b");
                self.ins(&format!("{r} = load {out}"));
                Val::Ind(r)
            }
            "ADDRESS" => {
                let r = self.ptr(&Expr::Builtin(name.into(), args.to_vec()))?;
                Val::Storage(r, Ty::Ptr)
            }
            "OFFSET" => return Err("%OFFSET is not available in this release".into()),
            other => return Err(format!("%{other} is not a built-in function")),
        })
    }

    /// Assign an expression to a whole variable.
    pub(super) fn assign(&mut self, v: &Var, e: &Expr) -> Result<(), String> {
        let place = v.place();
        match v.ty {
            VarType::Char => {
                let s = self.chr(e)?;
                self.ins(&format!("mov {place}, {s}"));
            }
            VarType::Dec | VarType::Int | VarType::Uint => {
                let s = self.num(e)?;
                self.ins(&format!("dec.mov {place}, {s}"));
            }
            VarType::Lgl => {
                let r = self.lgl(e)?;
                self.ins(&format!("store {place}, {r}"));
            }
            VarType::Ptr => {
                let r = self.ptr(e)?;
                self.ins(&format!("store {place}, {r}"));
            }
        }
        Ok(())
    }

    /// `CHGVAR %SST(&V s l) VALUE(...)`: a constant length is needed.
    pub(super) fn assign_sst(
        &mut self,
        v: &Var,
        start: &Expr,
        len: &Expr,
        e: &Expr,
    ) -> Result<(), String> {
        let Expr::Lit(Lit::Num(n)) = len else {
            return Err("%SST as a target needs a constant length".into());
        };
        let width: usize = n.parse().map_err(|_| "%SST needs a whole length".to_string())?;
        // A constant is cut or padded here; a value goes through a
        // field of the target's length, as CHGVAR pads or truncates.
        let t = if let Expr::Lit(Lit::Str(text)) = e {
            qstr(&format!("{:<width$}", text.chars().take(width).collect::<String>()))
        } else {
            let s = self.chr(e)?;
            let t = self.temp(&Ty::Char(u32::try_from(width).unwrap_or(1).max(1)));
            self.ins(&format!("mov {t}, {s}"));
            t
        };
        let st = self.int(start)?;
        let p = self.reg("p");
        self.ins(&format!("{p} = addr {}", v.place()));
        let q = self.reg("q");
        self.ins(&format!("{q} = ptradd {p}, {st}"));
        let q2 = self.reg("q");
        self.ins(&format!("{q2} = ptradd {q}, -1"));
        self.ins(&format!("mov [{q2}]:char({n}), {t}"));
        Ok(())
    }

    /// `CHGVAR %BIN(&V s l) VALUE(...)`.
    pub(super) fn assign_bin(
        &mut self,
        v: &Var,
        start: &Expr,
        len: &Expr,
        e: &Expr,
    ) -> Result<(), String> {
        let n = match len {
            Expr::Lit(Lit::Num(n)) if n == "2" || n == "4" => n.clone(),
            Expr::Lit(Lit::Num(n)) if n == "0" => if v.len == 4 { "4" } else { "2" }.to_string(),
            _ => return Err("%BIN takes a length of 2 or 4".into()),
        };
        let val = self.num(e)?;
        let st = self.int(start)?;
        let p = self.reg("p");
        self.ins(&format!("{p} = addr {}", v.place()));
        let q = self.reg("q");
        self.ins(&format!("{q} = ptradd {p}, {st}"));
        let q2 = self.reg("q");
        self.ins(&format!("{q2} = ptradd {q}, -1"));
        let ity = if n == "4" { "i32" } else { "i16" };
        // Through a wide decimal: a literal keeps no type of its own.
        let t = self.temp(&Ty::Dec(15, 0));
        self.ins(&format!("dec.mov {t}, {val}"));
        let r = self.reg("v");
        self.ins(&format!("{r} = cvt.{ity} {t}"));
        self.ins(&format!("store [{q2}]:{ity}, {r}"));
        Ok(())
    }

    /// A value into a place of a known type (for parameters and
    /// receivers).
    #[allow(clippy::wrong_self_convention)] // a place, not the generator, is filled
    pub(super) fn into_place(&mut self, place: &str, ty: &Ty, e: &Expr) -> Result<(), String> {
        match ty {
            Ty::Char(_) => {
                let s = self.chr(e)?;
                self.ins(&format!("mov {place}, {s}"));
            }
            Ty::Dec(..) => {
                let s = self.num(e)?;
                self.ins(&format!("dec.mov {place}, {s}"));
            }
            Ty::Lgl => {
                let r = self.lgl(e)?;
                self.ins(&format!("store {place}, {r}"));
            }
            Ty::Ptr => {
                let r = self.ptr(e)?;
                self.ins(&format!("store {place}, {r}"));
            }
        }
        Ok(())
    }

    /// The register value of a storage value, for by-value parameters.
    pub(super) fn by_value(&mut self, v: &Val) -> Result<String, String> {
        match v {
            Val::Ind(r) => Ok(r.clone()),
            Val::Storage(s, Ty::Dec(_, 0)) => {
                let r = self.reg("i");
                self.ins(&format!("{r} = cvt.i32 {s}"));
                Ok(r)
            }
            Val::Storage(s, Ty::Ptr) => Ok(s.clone()),
            Val::Storage(s, _) => Err(format!("{s} cannot be passed by value")),
        }
    }
}

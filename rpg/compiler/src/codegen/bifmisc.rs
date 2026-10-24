// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Built-in functions of parameters, procedures, null indicators,
//! type limits and float editing: `%PASSED`, `%OMITTED`, `%PARMNUM`,
//! `%PROC`, `%NULLIND`, `%HIVAL`, `%LOVAL`, `%EDITFLT`, `%THIS`, and the
//! built-ins only one operation takes (`%XML`, `%DATA`, ...), which
//! the front end reports elsewhere.

use rpgfront::ast::{Expr, Fig};

use super::Gen;
use super::expr::{V, Want};
use crate::diag::ids;
use crate::syms::Kind;
use crate::types::Ty;

impl Gen<'_> {
    /// One of this module's built-ins; `None` for other names.
    pub fn misc_bif(&mut self, name: &str, args: &[Expr]) -> Option<Option<V>> {
        Some(match name {
            "%PASSED" | "%OMITTED" | "%PARMNUM" => self.parm_bif(name, args),
            "%PROC" => self.proc_bif(args),
            "%NULLIND" => self.nullind(args),
            "%HIVAL" | "%LOVAL" => self.limit_bif(name == "%HIVAL", args),
            "%EDITFLT" => self.editflt(args),
            // A built-in out of its place (`rpgfront`): only the names
            // in what followed it are checked.
            "%DROPPED" => {
                let mut names = Vec::new();
                for a in args {
                    ref_names(a, &mut names);
                }
                for (n, at) in names {
                    if self.t.find(self.scope, &n).is_none() && self.undefined.insert(n.clone()) {
                        self.diags.push(rpgfront::Diag::new(ids::NOT_DEFINED, 30, at).arg(n));
                    }
                }
                None
            }
            "%THIS" => {
                self.err(ids::THIS_OUTSIDE, 30, name.to_string());
                None
            }
            _ => return None,
        })
    }

    /// The parameter of the current procedure a name gives: its number
    /// from 1.
    fn parm_number(&mut self, name: &str, args: &[Expr]) -> Option<usize> {
        let p = match args {
            [Expr::Ref { name: n, steps, .. }] if steps.is_empty() => n.clone(),
            _ => {
                self.err(ids::BIF_NOT_VALID, 30, name.to_string());
                return None;
            }
        };
        let proc = self.cur_proc.clone();
        let parms = self
            .t
            .find(0, &proc)
            .and_then(|id| match self.t.syms[id].kind.clone() {
                Some(Kind::Proto(pi)) => Some(self.t.protos[pi].parms.clone()),
                _ => None,
            })
            .unwrap_or_default();
        let k = parms.iter().position(|x| x.name.eq_ignore_ascii_case(&p));
        if k.is_none() {
            if name == "%PARMNUM" {
                // At the operand.
                let at = args.first().map_or(self.at, Expr::pos);
                self.diags.push(rpgfront::Diag::new(ids::PARMNUM_NOT_PARM, 20, at).arg(name.to_string()));
            } else {
                self.err(ids::BIF_NOT_VALID, 30, name.to_string());
            }
        }
        if let Some(id) = self.t.find(self.scope, &p) {
            self.t.refer(id, self.at.line, false);
        }
        // %OMITTED needs a parameter that can be omitted.
        if name == "%OMITTED" && k.is_some_and(|k| !parms[k].omit) {
            self.err(ids::BIF_NOT_VALID, 30, name.to_string());
            return None;
        }
        k.map(|k| k + 1)
    }

    /// `%PASSED(p)`, `%OMITTED(p)` and `%PARMNUM(p)`.
    fn parm_bif(&mut self, name: &str, args: &[Expr]) -> Option<V> {
        let k = self.parm_number(name, args)?;
        if name == "%PARMNUM" {
            return self.literal(&rpgfront::ast::Lit::Num(k.to_string()));
        }
        let Some(Expr::Ref { name: n, .. }) = args.first() else { return None };
        let p = self.t.find(self.scope, n).map(|id| self.t.syms[id].ir.clone())?;
        let _ = k;
        let r = self.b.reg();
        let op = if name == "%PASSED" { "passed" } else { "omitted" };
        self.b.ins(format!("{r} = {op} {p}"));
        Some(V::reg(Ty::Ind, r))
    }

    /// `%PROC()`: the procedure's external name; the main procedure's
    /// is the module's.
    fn proc_bif(&mut self, args: &[Expr]) -> Option<V> {
        if !args.is_empty() {
            self.err(ids::BIF_NOT_VALID, 30, "%PROC");
            return None;
        }
        let cur = self.cur_proc.clone();
        let text = if cur.is_empty() {
            self.opts.name.to_ascii_uppercase()
        } else {
            self.proc_external(&cur)
        };
        self.literal(&rpgfront::ast::Lit::Char(text))
    }

    /// `%NULLIND(field)`: the field's null indicator, a place.
    pub fn nullind(&mut self, args: &[Expr]) -> Option<V> {
        let v = self.expr(args.first()?, Want::default())?;
        let null = v.sym.filter(|s| self.t.syms[*s].nullind);
        match null {
            Some(s) => {
                let place = format!("{}__null", self.t.syms[s].ir.replace('.', "_"));
                self.externs.entry(place.clone()).or_insert_with(|| format!("static {place}: ind inz(*OFF)"));
                Some(V::place(Ty::Ind, place))
            }
            None => {
                self.err(ids::NOT_NULL_CAPABLE, 20, "%NULLIND");
                None
            }
        }
    }

    /// `%HIVAL(x)`, `%LOVAL(x)`: the largest or smallest value of x's
    /// type.
    fn limit_bif(&mut self, hi: bool, args: &[Expr]) -> Option<V> {
        let v = self.expr(args.first()?, Want::default())?;
        let nines = |d: u8, s: u8| {
            let mut t = "9".repeat(usize::from(d - s.min(d)));
            if s > 0 {
                t.push('.');
                t.push_str(&"9".repeat(usize::from(s)));
            }
            if t.starts_with('.') {
                t.insert(0, '0');
            }
            t
        };
        let text = match &v.ty {
            Ty::Packed(d, s) | Ty::Zoned(d, s) | Ty::Bin(d, s) => {
                let n = nines(*d, *s);
                if hi { n } else { format!("-{n}") }
            }
            Ty::Int(d) => {
                let bits = match d {
                    3 => 8,
                    5 => 16,
                    10 => 32,
                    _ => 64,
                };
                if hi { ((1i128 << (bits - 1)) - 1).to_string() } else { (-(1i128 << (bits - 1))).to_string() }
            }
            Ty::Uns(d) => {
                let bits = match d {
                    3 => 8,
                    5 => 16,
                    10 => 32,
                    _ => 64,
                };
                if hi { ((1u128 << bits) - 1).to_string() } else { "0".into() }
            }
            _ => {
                return Some(V {
                    fig: Some(if hi { Fig::Hival } else { Fig::Loval }),
                    ..V::lit(Ty::Unknown, if hi { "*HIVAL" } else { "*LOVAL" })
                });
            }
        };
        let t = self.tmp(&v.ty);
        self.b.ins(format!("dec.mov {}, {text}", t.op));
        Some(t)
    }

    /// `%EDITFLT(n)`: the number as a float in IBM's external form.
    fn editflt(&mut self, args: &[Expr]) -> Option<V> {
        let v = self.expr(args.first()?, Want::default())?;
        if !v.ty.is_numeric() {
            self.err(ids::EDITFLT_PARM, 30, "%EDITFLT");
            return None;
        }
        let single = v.ty == Ty::Float(4);
        let f = self.float_reg(&v);
        let n = if single { 14 } else { 23 };
        let t = self.tmp(&Ty::VarChar(n, 2));
        self.rt_proc("_QRNX_EDITFLT", "($dst: varchar(23) *VARSIZE, byval $f: f64, byval $single: i32)");
        self.b.ins(format!("callb _QRNX_EDITFLT({}, {f}, {})", t.op, i32::from(single)));
        Some(t)
    }

    /// A procedure's external name: `EXTPROC`'s, or its name upper case.
    fn proc_external(&self, name: &str) -> String {
        self.t
            .find(0, &name.to_ascii_uppercase())
            .and_then(|id| match self.t.syms[id].kind.clone() {
                Some(Kind::Proto(pi)) => self.t.protos[pi].extproc.clone(),
                _ => None,
            })
            .unwrap_or_else(|| name.to_ascii_uppercase())
    }
}

/// The names an expression references, with where.
fn ref_names(e: &Expr, out: &mut Vec<(String, rpgfront::ast::Pos)>) {
    match e {
        Expr::Ref { name, at, .. } => out.push((name.clone(), *at)),
        Expr::Bif { args, .. } | Expr::List(args, _) => args.iter().for_each(|a| ref_names(a, out)),
        Expr::Unary(_, a, _) => ref_names(a, out),
        Expr::Binary(_, a, b, _) | Expr::Assign(_, a, b, _) => {
            ref_names(a, out);
            ref_names(b, out);
        }
        _ => {}
    }
}

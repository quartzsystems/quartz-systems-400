// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Instructions, operands, places and literals.

use super::Parser;
use crate::error::Result;
use crate::inst::{
    Arg, BinOp, CallRet, CallTarget, Cond, Const, Debug, Index, Inst, Operand, Place, PlaceBase,
    Step, UnOp,
};
use crate::module::{Label, Linkage, Reg, index32};
use crate::text::lex::Tok;
use crate::verify::typing::def_type;

/// Rewrite the labels of an instruction through a map.
pub(crate) fn remap_labels(i: &mut Inst, map: &[Label]) {
    let f = |l: &mut Label| *l = map[l.0 as usize];
    match i {
        Inst::Br(l) => f(l),
        Inst::BrIf { t, f: fl, .. } => {
            f(t);
            f(fl);
        }
        Inst::Switch { cases, default, .. } => {
            for (_, l) in cases {
                f(l);
            }
            f(default);
        }
        Inst::Monitor { handlers, .. } => {
            for h in handlers {
                f(&mut h.target);
            }
        }
        _ => {}
    }
}

impl Parser {
    /// One instruction; the line end is left for the caller.
    pub(crate) fn inst(&mut self) -> Result<Inst> {
        let dst = if matches!(self.peek(), Tok::Reg(_)) && self.peek_at(1) == &Tok::Sym("=") {
            let r = self.reg_operand()?;
            self.bump();
            Some(r)
        } else {
            None
        };
        let word = self.ident()?;
        let mut parts = word.split('.');
        let op = parts.next().unwrap_or_default().to_string();
        let suffix: Vec<String> = parts.map(str::to_string).collect();
        let inst = match dst {
            Some(dst) => self.def_inst(dst, &op, &suffix)?,
            None => self.plain_inst(&op, &suffix)?,
        };
        Ok(inst)
    }

    fn no_suffix(&self, op: &str, suffix: &[String]) -> Result<()> {
        if suffix.is_empty() {
            Ok(())
        } else {
            Err(self.err(format!("{op} takes no suffix")))
        }
    }

    /// `%d = op...`.
    #[allow(clippy::too_many_lines)]
    fn def_inst(&mut self, dst: Reg, op: &str, suffix: &[String]) -> Result<Inst> {
        let inst = match op {
            "const" => {
                let ty_word = suffix
                    .first()
                    .ok_or_else(|| self.err("const needs a type: const.i32"))?
                    .clone();
                let ty = self.ty_from_word(&ty_word)?;
                self.type_reg(dst, ty)?;
                let value = self.literal()?;
                Inst::Const { dst, value }
            }
            "cvt" => {
                let ty_word = suffix
                    .first()
                    .ok_or_else(|| self.err("cvt needs a type: cvt.i64"))?
                    .clone();
                let ty = self.ty_from_word(&ty_word)?;
                let chk = match suffix.get(1).map(String::as_str) {
                    None => false,
                    Some("chk") => true,
                    Some(s) => {
                        return Err(self.err(format!("cvt.{ty_word}.{s} is not an instruction")));
                    }
                };
                self.type_reg(dst, ty)?;
                let src = self.operand()?;
                Inst::Cvt { dst, src, chk }
            }
            "load" => {
                self.no_suffix(op, suffix)?;
                let src = self.place()?;
                Inst::Load { dst, src }
            }
            "cmp" => {
                let c = suffix.first().and_then(|s| Cond::from_name(s));
                let cond = c.ok_or_else(|| self.err("cmp needs a condition: cmp.eq, cmp.lt..."))?;
                let a = self.operand()?;
                self.expect_sym(",")?;
                let b = self.operand()?;
                Inst::Cmp { dst, cond, a, b }
            }
            "select" => {
                self.no_suffix(op, suffix)?;
                let c = self.operand()?;
                self.expect_sym(",")?;
                let a = self.operand()?;
                self.expect_sym(",")?;
                let b = self.operand()?;
                Inst::Select { dst, c, a, b }
            }
            "parms" => {
                self.no_suffix(op, suffix)?;
                Inst::Parms { dst }
            }
            "passed" | "omitted" => {
                self.no_suffix(op, suffix)?;
                let name = self.var_name()?;
                let param = self
                    .cur_proc()
                    .params
                    .iter()
                    .position(|p| p.name == name)
                    .ok_or_else(|| self.err(format!("${name} is not a parameter")))?;
                let param = index32(param);
                if op == "passed" {
                    Inst::Passed { dst, param }
                } else {
                    Inst::Omitted { dst, param }
                }
            }
            "callb" | "call" | "callp" | "callpp" | "sysapi" => {
                self.no_suffix(op, suffix)?;
                self.call(op, Some(dst))?
            }
            _ => {
                if let Some(i) = self.ext_def(dst, op, suffix)? {
                    i
                } else if let Some(bin) = BinOp::from_name(op) {
                    let chk = match suffix.first().map(String::as_str) {
                        None => false,
                        Some("chk") => true,
                        Some(s) => return Err(self.err(format!("{op}.{s} is not an instruction"))),
                    };
                    let a = self.operand()?;
                    self.expect_sym(",")?;
                    let b = self.operand()?;
                    Inst::Bin {
                        dst,
                        op: bin,
                        a,
                        b,
                        chk,
                    }
                } else if let Some(un) = UnOp::from_name(op) {
                    self.no_suffix(op, suffix)?;
                    let a = self.operand()?;
                    Inst::Un { dst, op: un, a }
                } else {
                    return Err(self.err(format!("{op} does not define a register")));
                }
            }
        };
        // Type the destination from the operands.
        if let Some(t) = def_type(&self.m, self.cur_proc(), &inst).map_err(|e| self.err(e))? {
            self.type_reg(dst, t)?;
        }
        Ok(inst)
    }

    /// An instruction without a defined register.
    fn plain_inst(&mut self, op: &str, suffix: &[String]) -> Result<Inst> {
        Ok(match op {
            "store" | "mov" => {
                self.no_suffix(op, suffix)?;
                let dst = self.place()?;
                self.expect_sym(",")?;
                let src = self.operand()?;
                if op == "store" {
                    Inst::Store { dst, src }
                } else {
                    Inst::Mov { dst, src }
                }
            }
            "br" => {
                self.no_suffix(op, suffix)?;
                Inst::Br(self.label_operand()?)
            }
            "brif" => {
                self.no_suffix(op, suffix)?;
                let c = self.operand()?;
                self.expect_sym(",")?;
                let t = self.label_operand()?;
                self.expect_sym(",")?;
                let f = self.label_operand()?;
                Inst::BrIf { c, t, f }
            }
            "switch" => {
                self.no_suffix(op, suffix)?;
                let v = self.operand()?;
                self.expect_sym(",")?;
                self.expect_sym("[")?;
                let mut cases = Vec::new();
                while !self.eat_sym("]") {
                    let k = self.int()?;
                    let k = i64::try_from(k).map_err(|_| self.err("case value is out of range"))?;
                    self.expect_sym(":")?;
                    let l = self.label_operand()?;
                    cases.push((k, l));
                    if !self.eat_sym(",") {
                        self.expect_sym("]")?;
                        break;
                    }
                }
                self.expect_sym(",")?;
                self.expect_word("default")?;
                let default = self.label_operand()?;
                Inst::Switch { v, cases, default }
            }
            "ret" => {
                self.no_suffix(op, suffix)?;
                if matches!(self.peek(), Tok::Newline | Tok::End) {
                    Inst::Ret(None)
                } else {
                    Inst::Ret(Some(self.operand()?))
                }
            }
            "callb" | "call" | "callp" | "callpp" | "sysapi" => {
                self.no_suffix(op, suffix)?;
                self.call(op, None)?
            }
            "debug" => match suffix.first().map(String::as_str) {
                Some("stmt") => Inst::Debug(Debug::Stmt(self.u32()?)),
                Some("var") => {
                    let name = self.string()?;
                    self.expect_sym(",")?;
                    let place = self.place()?;
                    Inst::Debug(Debug::Var { name, place })
                }
                Some("scope") => {
                    if self.eat_word("begin") {
                        Inst::Debug(Debug::ScopeBegin)
                    } else if self.eat_word("end") {
                        Inst::Debug(Debug::ScopeEnd)
                    } else {
                        return Err(self.err("debug.scope begin or end"));
                    }
                }
                _ => return Err(self.err("debug.stmt, debug.var or debug.scope")),
            },
            "unreachable" => {
                self.no_suffix(op, suffix)?;
                Inst::Unreachable
            }
            _ => match self.ext_plain(op, suffix)? {
                Some(i) => i,
                None => {
                    return Err(self.err(format!(
                        "{op} is not an instruction, or it defines a register"
                    )));
                }
            },
        })
    }

    /// `op TARGET(args) [-> place]`.
    fn call(&mut self, op: &str, dst: Option<Reg>) -> Result<Inst> {
        let target = match op {
            "callb" => {
                let name = self.ident()?;
                if let Some((r, _)) = self.m.proc_named(&name) {
                    CallTarget::Local(r)
                } else if let Some((r, p)) = self.m.proto_named(&name) {
                    // An extern proc, or a bare signature imported by name.
                    if !matches!(p.linkage, Linkage::Import | Linkage::Signature) {
                        return Err(
                            self.err(format!("{name} is not a procedure; use call or sysapi"))
                        );
                    }
                    CallTarget::Proto(r)
                } else {
                    return Err(self.err(format!(
                        "{name} is not a procedure of this module or an extern proc"
                    )));
                }
            }
            "sysapi" => {
                let name = self.ident()?;
                match self.m.proto_named(&name) {
                    Some((r, p)) if p.linkage == Linkage::SysApi => CallTarget::Proto(r),
                    _ => return Err(self.err(format!("{name} is not an extern sysapi"))),
                }
            }
            "call" => match self.peek().clone() {
                Tok::Var(_) | Tok::Sym("[") => CallTarget::PgmVar(self.place()?),
                Tok::Ident(name) => {
                    self.bump();
                    if self.eat_sym("/") {
                        let pgm = self.ident()?;
                        CallTarget::PgmName {
                            name: pgm,
                            lib: Some(name),
                        }
                    } else if let Some((r, p)) = self.m.proto_named(&name) {
                        if !matches!(p.linkage, Linkage::Pgm { .. }) {
                            return Err(self.err(format!("{name} is not an extern pgm")));
                        }
                        CallTarget::Proto(r)
                    } else {
                        CallTarget::PgmName { name, lib: None }
                    }
                }
                _ => return Err(self.unexpected("a program name or a place")),
            },
            "callp" => CallTarget::SysPtr(self.reg_operand()?),
            "callpp" => {
                let ptr = self.reg_operand()?;
                self.expect_word("as")?;
                let name = self.ident()?;
                let (proto, _) = self
                    .m
                    .proto_named(&name)
                    .ok_or_else(|| self.err(format!("{name} is not a prototype")))?;
                CallTarget::ProcPtr { ptr, proto }
            }
            _ => unreachable!(),
        };
        self.expect_sym("(")?;
        let mut args = Vec::new();
        while !self.eat_sym(")") {
            if matches!(self.peek(), Tok::Special(s) if s == "OMIT") {
                self.bump();
                args.push(Arg::Omit);
            } else {
                args.push(Arg::Value(self.operand()?));
            }
            if !self.eat_sym(",") {
                self.expect_sym(")")?;
                break;
            }
        }
        let ret = match dst {
            Some(r) => Some(CallRet::Reg(r)),
            None if self.eat_sym("->") => Some(CallRet::Place(self.place()?)),
            None => None,
        };
        Ok(Inst::Call { target, args, ret })
    }

    /// A register, a place or a literal.
    pub(crate) fn operand(&mut self) -> Result<Operand> {
        Ok(match self.peek() {
            Tok::Reg(_) => Operand::Reg(self.reg_operand()?),
            Tok::Var(_) | Tok::Sym("[") => Operand::Place(self.place()?),
            _ => Operand::Const(self.literal()?),
        })
    }

    /// `$var`, `[%p [+ n]]:T`, each followed by `.field` and `[index]` steps.
    pub(crate) fn place(&mut self) -> Result<Place> {
        let mut path = Vec::new();
        let base = if self.eat_sym("[") {
            let ptr = self.reg_operand()?;
            let offset = if self.eat_sym("+") || matches!(self.peek(), Tok::Int(v) if *v < 0) {
                self.int()?
            } else {
                0
            };
            let offset = i64::try_from(offset).map_err(|_| self.err("offset is out of range"))?;
            self.expect_sym("]")?;
            self.expect_sym(":")?;
            let (ty, fields) = self.ty_and_fields()?;
            path.extend(fields.into_iter().map(Step::Field));
            PlaceBase::Deref { ptr, offset, ty }
        } else {
            PlaceBase::Var(self.var_ref()?)
        };
        loop {
            if self.eat_sym(".") {
                let word = self.ident()?;
                path.extend(word.split('.').map(|s| Step::Field(s.to_string())));
            } else if self.is_sym("[") {
                self.bump();
                let ix = match self.peek() {
                    Tok::Reg(_) => Index::Reg(self.reg_operand()?),
                    _ => Index::Const(self.u32()?),
                };
                self.expect_sym("]")?;
                path.push(Step::Index(ix));
            } else {
                break;
            }
        }
        Ok(Place { base, path })
    }

    /// A literal.
    pub(crate) fn literal(&mut self) -> Result<Const> {
        Ok(match self.bump() {
            Tok::Int(v) => Const::Int(v),
            Tok::Float(f) => Const::Float(f),
            Tok::Dec(s) => Const::Dec(s),
            Tok::Str(s) => Const::Str(s),
            Tok::Hex(h) => Const::Hex(h),
            Tok::Special(s) => match s.as_str() {
                "BLANKS" | "BLANK" => Const::Blanks,
                "ZEROS" | "ZERO" => Const::Zeros,
                "HIVAL" => Const::HiVal,
                "LOVAL" => Const::LoVal,
                "NULL" => Const::Null,
                "ON" => Const::On,
                "OFF" => Const::Off,
                _ => {
                    self.at -= 1;
                    return Err(self.err(format!("*{s} is not a literal")));
                }
            },
            _ => {
                self.at -= 1;
                return Err(self.unexpected("a literal"));
            }
        })
    }
}

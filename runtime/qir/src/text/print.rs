// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The printer: a module as canonical `.qir` text.

use std::fmt::Write as _;

use super::lex::hex;
use crate::inst::{
    Arg, CallRet, CallTarget, Const, Debug, Index, Inst, Operand, Place, PlaceBase, Step,
};
use crate::module::{Linkage, Module, Param, Proc, Proto, StorageClass, Var};
use crate::types::Type;

/// Print a module.
#[must_use]
pub fn print(m: &Module) -> String {
    let mut p = Printer {
        m,
        out: String::new(),
    };
    p.module();
    p.out
}

/// Print a type, using the module's named types where one matches.
#[must_use]
pub fn print_type(m: &Module, ty: &Type) -> String {
    Printer {
        m,
        out: String::new(),
    }
    .ty(ty)
}

pub(super) struct Printer<'a> {
    pub(super) m: &'a Module,
    pub(super) out: String,
}

impl Printer<'_> {
    fn module(&mut self) {
        let m = self.m;
        let _ = write!(
            self.out,
            "module {} lang({}) ccsid({}) actgrp({})",
            m.name,
            m.lang.name(),
            m.ccsid,
            m.actgrp
        );
        if m.dftactgrp {
            self.out.push_str(" dftactgrp");
        }
        if m.nomain {
            self.out.push_str(" nomain");
        }
        if m.options.srcstmt {
            self.out.push_str(" srcstmt");
        }
        if m.options.nodebugio {
            self.out.push_str(" nodebugio");
        }
        let _ = writeln!(self.out, " dbgview({})", m.options.dbgview.name());

        if !m.types.is_empty() {
            self.out.push('\n');
            for (i, t) in m.types.iter().enumerate() {
                // Earlier names only, so a type prints with the names it was built from.
                let shown = Printer {
                    m: &Module {
                        types: m.types[..i].to_vec(),
                        ..Module::new("")
                    },
                    out: String::new(),
                }
                .ty(&t.ty);
                let _ = writeln!(self.out, "type {} = {shown}", t.name);
            }
        }
        if !m.vars.is_empty() {
            self.out.push('\n');
            for v in &m.vars {
                let line = self.var(v, None);
                let _ = writeln!(self.out, "{line}");
            }
        }
        if !m.files.is_empty() {
            self.out.push('\n');
            for f in &m.files {
                let _ = write!(
                    self.out,
                    "file {} file({}) lib({}) usage({})",
                    f.name,
                    f.file,
                    f.lib,
                    f.usage.name()
                );
                if f.dev != crate::module::FileDev::Db {
                    let _ = write!(self.out, " dev({})", f.dev.name());
                }
                if f.keyed {
                    self.out.push_str(" keyed");
                }
                if !f.lvlchk {
                    self.out.push_str(" nolvlchk");
                }
                if f.form.0 != 0 {
                    let _ = write!(self.out, " formlen({})", f.form.0);
                }
                if f.form.1 != 0 {
                    let _ = write!(self.out, " formofl({})", f.form.1);
                }
                for r in &f.formats {
                    match &r.level {
                        Some(l) => {
                            let _ = write!(self.out, " fmt({}:\"{l}\")", r.name);
                        }
                        None => {
                            let _ = write!(self.out, " fmt({})", r.name);
                        }
                    }
                }
                self.out.push('\n');
            }
        }
        if !m.protos.is_empty() {
            self.out.push('\n');
            for p in &m.protos {
                let line = self.proto(p);
                let _ = writeln!(self.out, "{line}");
            }
        }
        for p in &m.procs {
            self.out.push('\n');
            self.proc(p);
        }
    }

    fn ty(&self, ty: &Type) -> String {
        if let Some(t) = self.m.types.iter().find(|t| t.ty == *ty) {
            return t.name.clone();
        }
        match ty {
            Type::Struct(s) => {
                let mut out = format!("struct({}) {{", s.size);
                for (i, f) in s.fields.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    let _ = write!(out, " {}: {} @{}", f.name, self.ty(&f.ty), f.offset);
                }
                out.push_str(" }");
                out
            }
            Type::Array { len, elem } => format!("array[{len}] of {}", self.ty(elem)),
            _ => ty.to_string(),
        }
    }

    fn var(&self, v: &Var, proc: Option<&Proc>) -> String {
        let mut out = String::new();
        match &v.class {
            StorageClass::Based(on) => {
                let name = self.m.var(*on, proc).map_or("?", |b| b.name.as_str());
                let _ = write!(out, "based ${}: {} on ${name}", v.name, self.ty(&v.ty));
            }
            StorageClass::Const => {
                let _ = write!(out, "const ${}: {}", v.name, self.ty(&v.ty));
            }
            c => {
                let _ = write!(out, "{} ${}: {}", c.keyword(), v.name, self.ty(&v.ty));
            }
        }
        if let Some(b) = &v.inz {
            if v.class == StorageClass::Const {
                let _ = write!(out, " = x\"{}\"", hex(b));
            } else {
                let _ = write!(out, " inz(x\"{}\")", hex(b));
            }
        }
        if v.export {
            out.push_str(" export");
        }
        out
    }

    fn params(&self, params: &[Param]) -> String {
        let mut out = String::from("(");
        for (i, p) in params.iter().enumerate() {
            if i > 0 {
                out.push_str(", ");
            }
            if p.by_value {
                out.push_str("byval ");
            }
            if p.const_ {
                out.push_str("const ");
            }
            let _ = write!(out, "${}: {}", p.name, self.ty(&p.ty));
            if p.nopass {
                out.push_str(" *NOPASS");
            }
            if p.omit {
                out.push_str(" *OMIT");
            }
            if p.varsize {
                out.push_str(" *VARSIZE");
            }
        }
        out.push(')');
        out
    }

    fn proto(&self, p: &Proto) -> String {
        let mut out = match &p.linkage {
            Linkage::Import => "extern proc ".to_string(),
            Linkage::Pgm { .. } => "extern pgm ".to_string(),
            Linkage::SysApi => "extern sysapi ".to_string(),
            Linkage::Signature => "proto ".to_string(),
        };
        out.push_str(&p.name);
        out.push_str(&self.params(&p.params));
        if let Some(r) = &p.ret {
            let _ = write!(out, " -> {}", self.ty(r));
        }
        if p.opdesc {
            out.push_str(" opdesc");
        }
        if let Linkage::Pgm { name, lib } = &p.linkage {
            let _ = write!(out, " pgm({name}) lib({lib})");
        }
        out
    }

    fn proc(&mut self, p: &Proc) {
        let _ = write!(self.out, "proc {}{}", p.name, self.params(&p.params));
        if let Some(r) = &p.ret {
            let _ = write!(self.out, " -> {}", self.ty(r));
        }
        if p.export {
            self.out.push_str(" export");
        }
        if p.entry {
            self.out.push_str(" entry");
        }
        if p.opdesc {
            self.out.push_str(" opdesc");
        }
        self.out.push_str(" {\n");
        for v in &p.vars {
            if matches!(v.class, StorageClass::Param(_)) {
                continue;
            }
            let line = self.var(v, Some(p));
            let _ = writeln!(self.out, "  {line}");
        }
        for b in &p.blocks {
            let _ = writeln!(self.out, "{}:", b.label);
            for i in &b.insts {
                let line = self.inst(p, i);
                let _ = writeln!(self.out, "  {line}");
            }
        }
        self.out.push_str("}\n");
    }

    pub(super) fn reg(p: &Proc, r: crate::module::Reg) -> String {
        format!("%{}", p.reg_name(r))
    }

    pub(super) fn place(&self, p: &Proc, pl: &Place) -> String {
        let mut out = match &pl.base {
            PlaceBase::Var(r) => {
                format!(
                    "${}",
                    self.m.var(*r, Some(p)).map_or("?", |v| v.name.as_str())
                )
            }
            PlaceBase::Deref { ptr, offset, ty } => {
                let off = match *offset {
                    0 => String::new(),
                    o if o < 0 => format!(" {o}"),
                    o => format!(" +{o}"),
                };
                format!("[{}{off}]:{}", Self::reg(p, *ptr), self.ty(ty))
            }
        };
        for s in &pl.path {
            match s {
                Step::Field(f) => {
                    out.push('.');
                    out.push_str(f);
                }
                Step::Index(Index::Reg(r)) => {
                    let _ = write!(out, "[{}]", Self::reg(p, *r));
                }
                Step::Index(Index::Const(i)) => {
                    let _ = write!(out, "[{i}]");
                }
            }
        }
        out
    }

    fn konst(c: &Const) -> String {
        match c {
            Const::Str(s) => format!("\"{}\"", s.replace('"', "\"\"")),
            Const::Float(f) => {
                let s = format!("{f:e}");
                if s.contains('e') { s } else { format!("{s}e0") }
            }
            _ => crate::verify::typing::describe_const(c),
        }
    }

    pub(super) fn operand(&self, p: &Proc, o: &Operand) -> String {
        match o {
            Operand::Reg(r) => Self::reg(p, *r),
            Operand::Place(pl) => self.place(p, pl),
            Operand::Const(c) => Self::konst(c),
        }
    }

    fn label(p: &Proc, l: crate::module::Label) -> String {
        p.label_name(l).to_string()
    }

    #[allow(clippy::many_single_char_names)]
    fn inst(&self, p: &Proc, i: &Inst) -> String {
        let r = |r| Self::reg(p, r);
        let o = |x| self.operand(p, x);
        match i {
            Inst::Const { dst, value } => {
                let ty = p.reg_type(*dst).map_or("?".into(), |t| self.ty(t));
                format!("{} = const.{ty} {}", r(*dst), Self::konst(value))
            }
            Inst::Load { dst, src } => format!("{} = load {}", r(*dst), self.place(p, src)),
            Inst::Store { dst, src } => format!("store {}, {}", self.place(p, dst), o(src)),
            Inst::Mov { dst, src } => format!("mov {}, {}", self.place(p, dst), o(src)),
            Inst::Bin { dst, op, a, b, chk } => format!(
                "{} = {}{} {}, {}",
                r(*dst),
                op.name(),
                if *chk { ".chk" } else { "" },
                o(a),
                o(b)
            ),
            Inst::Un { dst, op, a } => format!("{} = {} {}", r(*dst), op.name(), o(a)),
            Inst::Cmp { dst, cond, a, b } => {
                format!("{} = cmp.{} {}, {}", r(*dst), cond.name(), o(a), o(b))
            }
            Inst::Select { dst, c, a, b } => {
                format!("{} = select {}, {}, {}", r(*dst), o(c), o(a), o(b))
            }
            Inst::Cvt { dst, src, chk } => {
                let ty = p.reg_type(*dst).map_or("?".into(), |t| self.ty(t));
                format!(
                    "{} = cvt.{ty}{} {}",
                    r(*dst),
                    if *chk { ".chk" } else { "" },
                    o(src)
                )
            }
            Inst::Br(l) => format!("br {}", Self::label(p, *l)),
            Inst::BrIf { c, t, f } => {
                format!(
                    "brif {}, {}, {}",
                    o(c),
                    Self::label(p, *t),
                    Self::label(p, *f)
                )
            }
            Inst::Switch { v, cases, default } => {
                let cs: Vec<String> = cases
                    .iter()
                    .map(|(k, l)| format!("{k}: {}", Self::label(p, *l)))
                    .collect();
                format!(
                    "switch {}, [{}], default {}",
                    o(v),
                    cs.join(", "),
                    Self::label(p, *default)
                )
            }
            Inst::Ret(None) => "ret".into(),
            Inst::Ret(Some(v)) => format!("ret {}", o(v)),
            Inst::Call { target, args, ret } => self.call(p, target, args, ret.as_ref()),
            Inst::Parms { dst } => format!("{} = parms", r(*dst)),
            Inst::Passed { dst, param } => {
                format!("{} = passed ${}", r(*dst), Self::param_name(p, *param))
            }
            Inst::Omitted { dst, param } => {
                format!("{} = omitted ${}", r(*dst), Self::param_name(p, *param))
            }
            Inst::Debug(Debug::Stmt(n)) => format!("debug.stmt {n}"),
            Inst::Debug(Debug::Var { name, place }) => {
                format!("debug.var \"{name}\", {}", self.place(p, place))
            }
            Inst::Debug(Debug::ScopeBegin) => "debug.scope begin".into(),
            Inst::Debug(Debug::ScopeEnd) => "debug.scope end".into(),
            Inst::Unreachable => "unreachable".into(),
            _ => self.ext_inst(p, i),
        }
    }

    fn param_name(p: &Proc, i: u32) -> String {
        p.params
            .get(i as usize)
            .map_or_else(|| i.to_string(), |x| x.name.clone())
    }

    fn call(&self, p: &Proc, target: &CallTarget, args: &[Arg], ret: Option<&CallRet>) -> String {
        let mut out = String::new();
        if let Some(CallRet::Reg(r)) = ret {
            let _ = write!(out, "{} = ", Self::reg(p, *r));
        }
        match target {
            CallTarget::Local(pr) => {
                let name = self
                    .m
                    .procs
                    .get(pr.0 as usize)
                    .map_or("?", |x| x.name.as_str());
                let _ = write!(out, "callb {name}");
            }
            CallTarget::Proto(pr) => {
                let proto = self.m.protos.get(pr.0 as usize);
                let op = match proto.map(|x| &x.linkage) {
                    Some(Linkage::Pgm { .. }) => "call",
                    Some(Linkage::SysApi) => "sysapi",
                    _ => "callb",
                };
                let _ = write!(out, "{op} {}", proto.map_or("?", |x| x.name.as_str()));
            }
            CallTarget::PgmName { name, lib } => match lib {
                Some(l) => {
                    let _ = write!(out, "call {l}/{name}");
                }
                None => {
                    let _ = write!(out, "call {name}");
                }
            },
            CallTarget::PgmVar(pl) => {
                let _ = write!(out, "call {}", self.place(p, pl));
            }
            CallTarget::ProcPtr { ptr, proto } => {
                let name = self
                    .m
                    .protos
                    .get(proto.0 as usize)
                    .map_or("?", |x| x.name.as_str());
                let _ = write!(out, "callpp {} as {name}", Self::reg(p, *ptr));
            }
            CallTarget::SysPtr(r) => {
                let _ = write!(out, "callp {}", Self::reg(p, *r));
            }
        }
        out.push('(');
        for (i, a) in args.iter().enumerate() {
            if i > 0 {
                out.push_str(", ");
            }
            match a {
                Arg::Omit => out.push_str("*OMIT"),
                Arg::Value(v) => out.push_str(&self.operand(p, v)),
            }
        }
        out.push(')');
        if let Some(CallRet::Place(pl)) = ret {
            let _ = write!(out, " -> {}", self.place(p, pl));
        }
        out
    }
}

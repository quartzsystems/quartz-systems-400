// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Writing a module. `read.rs` mirrors this file field for field.

use super::MAGIC;
use crate::inst::{
    Arg, CallRet, CallTarget, Callee, Const, CvtOpts, Debug, DtInst, DtaqInst, Index, Inst, IoInst,
    MsgInst, MsgQueue, Operand, Place, PlaceBase, PtrInst, Selector, Step, StrInst, TrimSide,
};
use crate::module::{
    ActGrp, DbgView, FORMAT_VERSION, FileDev, FileRef, FileUsage, Lang, Linkage, Module, Param,
    Proc, Proto, StorageClass, Var, VarRef,
};
use crate::types::{DateFmt, TimeFmt, Type};

/// Write a module.
#[must_use]
pub fn write(m: &Module) -> Vec<u8> {
    let mut w = W { out: Vec::new() };
    w.out.extend_from_slice(MAGIC);
    w.u16(FORMAT_VERSION);
    w.u16(0);
    w.module(m);
    w.out
}

/// Index of a date format, shared with the reader.
pub(super) const DATE_FMTS: [DateFmt; 8] = [
    DateFmt::Iso,
    DateFmt::Usa,
    DateFmt::Eur,
    DateFmt::Jis,
    DateFmt::Mdy,
    DateFmt::Dmy,
    DateFmt::Ymd,
    DateFmt::Jul,
];

/// Index of a time format, shared with the reader.
pub(super) const TIME_FMTS: [TimeFmt; 5] = [
    TimeFmt::Hms,
    TimeFmt::Iso,
    TimeFmt::Usa,
    TimeFmt::Eur,
    TimeFmt::Jis,
];

/// The tag of a value in its table.
fn index_of<T: PartialEq>(table: &[T], v: &T) -> u8 {
    let i = table.iter().position(|x| x == v).expect("known value");
    u8::try_from(i).expect("small table")
}

struct W {
    out: Vec<u8>,
}

#[allow(clippy::cast_possible_truncation)]
fn len32(n: usize) -> u32 {
    u32::try_from(n).expect("fewer than 2^32 items")
}

impl W {
    fn u8(&mut self, v: u8) {
        self.out.push(v);
    }
    fn u16(&mut self, v: u16) {
        self.out.extend_from_slice(&v.to_le_bytes());
    }
    fn u32(&mut self, v: u32) {
        self.out.extend_from_slice(&v.to_le_bytes());
    }
    fn i64(&mut self, v: i64) {
        self.out.extend_from_slice(&v.to_le_bytes());
    }
    fn bool(&mut self, v: bool) {
        self.u8(u8::from(v));
    }
    fn bytes(&mut self, b: &[u8]) {
        self.u32(len32(b.len()));
        self.out.extend_from_slice(b);
    }
    fn str(&mut self, s: &str) {
        self.bytes(s.as_bytes());
    }
    fn opt<T>(&mut self, v: Option<&T>, f: impl FnOnce(&mut Self, &T)) {
        match v {
            Some(x) => {
                self.u8(1);
                f(self, x);
            }
            None => self.u8(0),
        }
    }
    fn list<T>(&mut self, v: &[T], mut f: impl FnMut(&mut Self, &T)) {
        self.u32(len32(v.len()));
        for x in v {
            f(self, x);
        }
    }

    fn module(&mut self, m: &Module) {
        self.str(&m.name);
        self.u8(match m.lang {
            Lang::Qir => 0,
            Lang::Rpgle => 1,
            Lang::Clle => 2,
        });
        self.u16(m.ccsid);
        match &m.actgrp {
            ActGrp::New => self.u8(0),
            ActGrp::Caller => self.u8(1),
            ActGrp::Named(n) => {
                self.u8(2);
                self.str(n);
            }
        }
        self.bool(m.dftactgrp);
        self.bool(m.nomain);
        self.bool(m.options.srcstmt);
        self.bool(m.options.nodebugio);
        self.u8(match m.options.dbgview {
            DbgView::None => 0,
            DbgView::Stmt => 1,
            DbgView::Source => 2,
            DbgView::List => 3,
            DbgView::All => 4,
        });
        self.list(&m.types, |w, t| {
            w.str(&t.name);
            w.ty(&t.ty);
        });
        self.list(&m.vars, Self::var);
        self.list(&m.files, Self::file);
        self.list(&m.protos, Self::proto);
        self.list(&m.procs, Self::proc);
    }

    #[allow(clippy::too_many_lines)]
    fn ty(&mut self, t: &Type) {
        match t {
            Type::I8 => self.u8(0),
            Type::I16 => self.u8(1),
            Type::I32 => self.u8(2),
            Type::I64 => self.u8(3),
            Type::U8 => self.u8(4),
            Type::U16 => self.u8(5),
            Type::U32 => self.u8(6),
            Type::U64 => self.u8(7),
            Type::F32 => self.u8(8),
            Type::F64 => self.u8(9),
            Type::Packed(s) => {
                self.u8(10);
                self.u8(s.digits);
                self.u8(s.scale);
            }
            Type::Zoned(s) => {
                self.u8(11);
                self.u8(s.digits);
                self.u8(s.scale);
            }
            Type::Bin(s) => {
                self.u8(26);
                self.u8(s.digits);
                self.u8(s.scale);
            }
            Type::Char(n) => {
                self.u8(12);
                self.u32(*n);
            }
            Type::VarChar(n, 2) => {
                self.u8(13);
                self.u32(*n);
            }
            Type::VarChar(n, p) => {
                self.u8(27);
                self.u32(*n);
                self.u8(*p);
            }
            Type::Ucs2(n) => {
                self.u8(14);
                self.u32(*n);
            }
            Type::VarUcs2(n, 2) => {
                self.u8(15);
                self.u32(*n);
            }
            Type::VarUcs2(n, p) => {
                self.u8(28);
                self.u32(*n);
                self.u8(*p);
            }
            Type::VarGraphic(n, p) => {
                self.u8(29);
                self.u32(*n);
                self.u8(*p);
            }
            Type::Graphic(n) => {
                self.u8(16);
                self.u32(*n);
            }
            Type::Date(d) => {
                self.u8(17);
                self.u8(index_of(&DATE_FMTS, d));
            }
            Type::Time(t) => {
                self.u8(18);
                self.u8(index_of(&TIME_FMTS, t));
            }
            Type::Timestamp(p) => {
                self.u8(19);
                self.u8(*p);
            }
            Type::Ind => self.u8(20),
            Type::Ptr => self.u8(21),
            Type::ProcPtr => self.u8(22),
            Type::Struct(s) => {
                self.u8(23);
                self.u32(s.size);
                self.list(&s.fields, |w, f| {
                    w.str(&f.name);
                    w.u32(f.offset);
                    w.ty(&f.ty);
                });
            }
            Type::Array { len, elem, stride: None } => {
                self.u8(24);
                self.u32(*len);
                self.ty(elem);
            }
            Type::Array { len, elem, stride: Some(s) } => {
                self.u8(30);
                self.u32(*len);
                self.u32(*s);
                self.ty(elem);
            }
            Type::Bytes(n) => {
                self.u8(25);
                self.u32(*n);
            }
        }
    }

    fn var_ref(&mut self, r: VarRef) {
        self.u8(match r.scope {
            crate::module::Scope::Module => 0,
            crate::module::Scope::Proc => 1,
        });
        self.u32(r.index);
    }

    fn var(&mut self, v: &Var) {
        self.str(&v.name);
        self.ty(&v.ty);
        match &v.class {
            StorageClass::Static => self.u8(0),
            StorageClass::Auto => self.u8(1),
            StorageClass::Based(r) => {
                self.u8(2);
                self.var_ref(*r);
            }
            StorageClass::Import => self.u8(3),
            StorageClass::Const => self.u8(4),
            StorageClass::Param(i) => {
                self.u8(5);
                self.u32(*i);
            }
        }
        self.bool(v.export);
        self.opt(v.inz.as_ref(), |w, b| w.bytes(b));
    }

    fn file(&mut self, f: &FileRef) {
        self.str(&f.name);
        self.str(&f.file);
        self.str(&f.lib);
        self.u8(match f.dev {
            FileDev::Db => 0,
            FileDev::Display => 1,
            FileDev::Printer => 2,
        });
        self.u8(match f.usage {
            FileUsage::Input => 0,
            FileUsage::Output => 1,
            FileUsage::Update => 2,
            FileUsage::All => 3,
        });
        self.bool(f.keyed);
        self.bool(f.lvlchk);
        self.list(&f.formats, |w, r| {
            w.str(&r.name);
            w.opt(r.level.as_ref(), |w, l| w.str(l));
        });
        self.u16(f.form.0);
        self.u16(f.form.1);
    }

    fn param(&mut self, p: &Param) {
        self.str(&p.name);
        self.ty(&p.ty);
        self.bool(p.by_value);
        self.bool(p.nopass);
        self.bool(p.omit);
        self.bool(p.varsize);
        self.bool(p.const_);
    }

    fn proto(&mut self, p: &Proto) {
        self.str(&p.name);
        self.list(&p.params, Self::param);
        self.opt(p.ret.as_ref(), Self::ty);
        self.bool(p.opdesc);
        match &p.linkage {
            Linkage::Import => self.u8(0),
            Linkage::Pgm { name, lib } => {
                self.u8(1);
                self.str(name);
                self.str(lib);
            }
            Linkage::SysApi => self.u8(2),
            Linkage::Signature => self.u8(3),
        }
    }

    fn proc(&mut self, p: &Proc) {
        self.str(&p.name);
        self.list(&p.params, Self::param);
        self.opt(p.ret.as_ref(), Self::ty);
        self.bool(p.export);
        self.bool(p.entry);
        self.bool(p.opdesc);
        self.list(&p.vars, Self::var);
        self.list(&p.regs, |w, r| {
            w.str(&r.name);
            w.ty(&r.ty);
        });
        self.list(&p.blocks, |w, b| {
            w.str(&b.label);
            w.list(&b.insts, Self::inst);
        });
    }

    fn konst(&mut self, c: &Const) {
        match c {
            Const::Int(v) => {
                self.u8(0);
                self.out.extend_from_slice(&v.to_le_bytes());
            }
            Const::Float(f) => {
                self.u8(1);
                self.out.extend_from_slice(&f.to_bits().to_le_bytes());
            }
            Const::Dec(s) => {
                self.u8(2);
                self.str(s);
            }
            Const::Str(s) => {
                self.u8(3);
                self.str(s);
            }
            Const::Hex(h) => {
                self.u8(4);
                self.bytes(h);
            }
            Const::Blanks => self.u8(5),
            Const::Zeros => self.u8(6),
            Const::HiVal => self.u8(7),
            Const::LoVal => self.u8(8),
            Const::Null => self.u8(9),
            Const::On => self.u8(10),
            Const::Off => self.u8(11),
        }
    }

    fn place(&mut self, p: &Place) {
        match &p.base {
            PlaceBase::Var(r) => {
                self.u8(0);
                self.var_ref(*r);
            }
            PlaceBase::Deref { ptr, offset, ty } => {
                self.u8(1);
                self.u32(ptr.0);
                self.i64(*offset);
                self.ty(ty);
            }
        }
        self.list(&p.path, |w, s| match s {
            Step::Field(f) => {
                w.u8(0);
                w.str(f);
            }
            Step::Index(Index::Reg(r)) => {
                w.u8(1);
                w.u32(r.0);
            }
            Step::Index(Index::Const(i)) => {
                w.u8(2);
                w.u32(*i);
            }
        });
    }

    fn operand(&mut self, o: &Operand) {
        match o {
            Operand::Reg(r) => {
                self.u8(0);
                self.u32(r.0);
            }
            Operand::Place(p) => {
                self.u8(1);
                self.place(p);
            }
            Operand::Const(c) => {
                self.u8(2);
                self.konst(c);
            }
        }
    }

    fn opt_operand(&mut self, o: Option<&Operand>) {
        self.opt(o, Self::operand);
    }

    fn opt_place(&mut self, p: Option<&Place>) {
        self.opt(p, Self::place);
    }

    fn label(&mut self, l: crate::module::Label) {
        self.u32(l.0);
    }

    fn reg(&mut self, r: crate::module::Reg) {
        self.u32(r.0);
    }

    fn date_fmt(&mut self, d: Option<DateFmt>) {
        self.opt(d.as_ref(), |w, d| {
            w.u8(index_of(&DATE_FMTS, d));
        });
    }

    fn time_fmt(&mut self, t: Option<TimeFmt>) {
        self.opt(t.as_ref(), |w, t| {
            w.u8(index_of(&TIME_FMTS, t));
        });
    }

    fn queue(&mut self, q: &MsgQueue) {
        match q {
            MsgQueue::Same => self.u8(0),
            MsgQueue::Prv => self.u8(1),
            MsgQueue::CtlBdy => self.u8(2),
            MsgQueue::Ext => self.u8(3),
            MsgQueue::SysOpr => self.u8(4),
            MsgQueue::Up(n) => {
                self.u8(5);
                self.operand(n);
            }
        }
    }

    #[allow(clippy::too_many_lines)]
    fn inst(&mut self, i: &Inst) {
        match i {
            Inst::Const { dst, value } => {
                self.u8(0);
                self.reg(*dst);
                self.konst(value);
            }
            Inst::Load { dst, src } => {
                self.u8(1);
                self.reg(*dst);
                self.place(src);
            }
            Inst::Store { dst, src } => {
                self.u8(2);
                self.place(dst);
                self.operand(src);
            }
            Inst::Mov { dst, src } => {
                self.u8(3);
                self.place(dst);
                self.operand(src);
            }
            Inst::Bin { dst, op, a, b, chk } => {
                self.u8(4);
                self.reg(*dst);
                self.u8(*op as u8);
                self.operand(a);
                self.operand(b);
                self.bool(*chk);
            }
            Inst::Un { dst, op, a } => {
                self.u8(5);
                self.reg(*dst);
                self.u8(*op as u8);
                self.operand(a);
            }
            Inst::Cmp { dst, cond, a, b } => {
                self.u8(6);
                self.reg(*dst);
                self.u8(*cond as u8);
                self.operand(a);
                self.operand(b);
            }
            Inst::Select { dst, c, a, b } => {
                self.u8(7);
                self.reg(*dst);
                self.operand(c);
                self.operand(a);
                self.operand(b);
            }
            Inst::Cvt { dst, src, chk } => {
                self.u8(8);
                self.reg(*dst);
                self.operand(src);
                self.bool(*chk);
            }
            Inst::Br(l) => {
                self.u8(9);
                self.label(*l);
            }
            Inst::BrIf { c, t, f } => {
                self.u8(10);
                self.operand(c);
                self.label(*t);
                self.label(*f);
            }
            Inst::Switch { v, cases, default } => {
                self.u8(11);
                self.operand(v);
                self.list(cases, |w, (k, l)| {
                    w.i64(*k);
                    w.label(*l);
                });
                self.label(*default);
            }
            Inst::Ret(v) => {
                self.u8(12);
                self.opt_operand(v.as_ref());
            }
            Inst::Call { target, args, ret } => {
                self.u8(13);
                match target {
                    CallTarget::Local(p) => {
                        self.u8(0);
                        self.u32(p.0);
                    }
                    CallTarget::Proto(p) => {
                        self.u8(1);
                        self.u32(p.0);
                    }
                    CallTarget::PgmName { name, lib } => {
                        self.u8(2);
                        self.str(name);
                        self.opt(lib.as_ref(), |w, l| w.str(l));
                    }
                    CallTarget::PgmVar(p) => {
                        self.u8(3);
                        self.place(p);
                    }
                    CallTarget::ProcPtr { ptr, proto } => {
                        self.u8(4);
                        self.reg(*ptr);
                        self.u32(proto.0);
                    }
                    CallTarget::SysPtr(r) => {
                        self.u8(5);
                        self.reg(*r);
                    }
                }
                self.list(args, |w, a| match a {
                    Arg::Omit => w.u8(0),
                    Arg::Value(v) => {
                        w.u8(1);
                        w.operand(v);
                    }
                });
                match ret {
                    None => self.u8(0),
                    Some(CallRet::Reg(r)) => {
                        self.u8(1);
                        self.reg(*r);
                    }
                    Some(CallRet::Place(p)) => {
                        self.u8(2);
                        self.place(p);
                    }
                }
            }
            Inst::Parms { dst } => {
                self.u8(14);
                self.reg(*dst);
            }
            Inst::Passed { dst, param } => {
                self.u8(15);
                self.reg(*dst);
                self.u32(*param);
            }
            Inst::Omitted { dst, param } => {
                self.u8(16);
                self.reg(*dst);
                self.u32(*param);
            }
            Inst::Debug(d) => {
                self.u8(17);
                match d {
                    Debug::Stmt(n) => {
                        self.u8(0);
                        self.u32(*n);
                    }
                    Debug::Var { name, place } => {
                        self.u8(1);
                        self.str(name);
                        self.place(place);
                    }
                    Debug::ScopeBegin => self.u8(2),
                    Debug::ScopeEnd => self.u8(3),
                }
            }
            Inst::Unreachable => self.u8(18),
            Inst::DecBin {
                dst,
                op,
                a,
                b,
                prec,
                halfadj,
                resdecpos,
            } => {
                self.u8(19);
                self.place(dst);
                self.u8(*op as u8);
                self.operand(a);
                self.operand(b);
                self.u8(prec.digits);
                self.u8(prec.scale);
                self.bool(*halfadj);
                self.bool(*resdecpos);
            }
            Inst::DecUn {
                dst,
                op,
                src,
                halfadj,
                trunc,
            } => {
                self.u8(20);
                self.place(dst);
                self.u8(*op as u8);
                self.operand(src);
                self.bool(*halfadj);
                self.bool(*trunc);
            }
            Inst::DecCmp { dst, cond, a, b } => {
                self.u8(21);
                self.reg(*dst);
                self.u8(*cond as u8);
                self.operand(a);
                self.operand(b);
            }
            Inst::Convert { dst, src, opts } => {
                self.u8(22);
                self.place(dst);
                self.operand(src);
                self.cvt_opts(opts);
            }
            Inst::Str(s) => {
                self.u8(23);
                self.str_inst(s);
            }
            Inst::Dt(d) => {
                self.u8(24);
                self.dt_inst(d);
            }
            Inst::Ptr(p) => {
                self.u8(25);
                self.ptr_inst(p);
            }
            Inst::Monitor { name, handlers } => {
                self.u8(26);
                self.str(name);
                self.list(handlers, |w, h| {
                    match &h.selector {
                        Selector::All => w.u8(0),
                        Selector::Program => w.u8(1),
                        Selector::File => w.u8(2),
                        Selector::Status(a, b) => {
                            w.u8(3);
                            w.u16(*a);
                            w.u16(*b);
                        }
                        Selector::MsgId(id) => {
                            w.u8(4);
                            w.str(id);
                        }
                    }
                    w.label(h.target);
                });
            }
            Inst::EndMon(name) => {
                self.u8(27);
                self.str(name);
            }
            Inst::Io(io) => {
                self.u8(28);
                self.io_inst(io);
            }
            Inst::Msg(m) => {
                self.u8(29);
                self.msg_inst(m);
            }
            Inst::Dsply { src, reply } => {
                self.u8(30);
                self.operand(src);
                self.opt_place(reply.as_ref());
            }
            Inst::Dtaara {
                op,
                name,
                lib,
                data,
            } => {
                self.u8(31);
                self.u8(*op as u8);
                self.operand(name);
                self.operand(lib);
                self.opt_place(data.as_ref());
            }
            Inst::Alloc { dst, n } => {
                self.u8(34);
                self.reg(*dst);
                self.operand(n);
            }
            Inst::Realloc { dst, ptr, n } => {
                self.u8(35);
                self.reg(*dst);
                self.operand(ptr);
                self.operand(n);
            }
            Inst::Dealloc { ptr } => {
                self.u8(36);
                self.operand(ptr);
            }
            Inst::Dtaq(DtaqInst::Snd {
                name,
                lib,
                data,
                len,
            }) => {
                self.u8(32);
                self.operand(name);
                self.operand(lib);
                self.operand(data);
                self.operand(len);
            }
            Inst::Dtaq(DtaqInst::Rcv {
                dst,
                name,
                lib,
                data,
                len,
                wait,
            }) => {
                self.u8(33);
                self.reg(*dst);
                self.operand(name);
                self.operand(lib);
                self.place(data);
                self.operand(len);
                self.operand(wait);
            }
        }
    }

    fn cvt_opts(&mut self, o: &CvtOpts) {
        self.bool(o.halfadj);
        self.date_fmt(o.datfmt);
        self.time_fmt(o.timfmt);
        self.opt(o.ccsid.as_ref(), |w, c| w.u16(*c));
    }

    #[allow(clippy::too_many_lines)]
    fn str_inst(&mut self, s: &StrInst) {
        match s {
            StrInst::Cat { dst, a, b } => {
                self.u8(0);
                self.place(dst);
                self.operand(a);
                self.operand(b);
            }
            StrInst::Subst {
                dst,
                src,
                start,
                len,
            } => {
                self.u8(1);
                self.place(dst);
                self.operand(src);
                self.operand(start);
                self.operand(len);
            }
            StrInst::Scan {
                dst,
                needle,
                hay,
                start,
            } => {
                self.u8(2);
                self.reg(*dst);
                self.operand(needle);
                self.operand(hay);
                self.opt_operand(start.as_ref());
            }
            StrInst::Check {
                dst,
                set,
                src,
                start,
                reverse,
            } => {
                self.u8(3);
                self.reg(*dst);
                self.operand(set);
                self.operand(src);
                self.opt_operand(start.as_ref());
                self.bool(*reverse);
            }
            StrInst::Xlate {
                dst,
                from,
                to,
                src,
                start,
            } => {
                self.u8(4);
                self.place(dst);
                self.operand(from);
                self.operand(to);
                self.operand(src);
                self.opt_operand(start.as_ref());
            }
            StrInst::Trim {
                dst,
                src,
                chars,
                side,
            } => {
                self.u8(5);
                self.place(dst);
                self.operand(src);
                self.opt_operand(chars.as_ref());
                self.u8(match side {
                    TrimSide::Both => 0,
                    TrimSide::Left => 1,
                    TrimSide::Right => 2,
                });
            }
            StrInst::Cmp { dst, cond, a, b } => {
                self.u8(6);
                self.reg(*dst);
                self.u8(*cond as u8);
                self.operand(a);
                self.operand(b);
            }
            StrInst::Len { dst, src } => {
                self.u8(7);
                self.reg(*dst);
                self.operand(src);
            }
            StrInst::SetLen { dst, n } => {
                self.u8(8);
                self.place(dst);
                self.operand(n);
            }
            StrInst::Replace {
                dst,
                src,
                repl,
                start,
                len,
            } => {
                self.u8(9);
                self.place(dst);
                self.operand(src);
                self.operand(repl);
                self.operand(start);
                self.opt_operand(len.as_ref());
            }
        }
    }

    fn dt_inst(&mut self, d: &DtInst) {
        match d {
            DtInst::Add { dst, src, n, unit } => {
                self.u8(0);
                self.place(dst);
                self.operand(src);
                self.operand(n);
                self.u8(*unit as u8);
            }
            DtInst::Diff { dst, a, b, unit } => {
                self.u8(1);
                self.reg(*dst);
                self.operand(a);
                self.operand(b);
                self.u8(*unit as u8);
            }
            DtInst::Extract { dst, src, unit } => {
                self.u8(2);
                self.reg(*dst);
                self.operand(src);
                self.u8(*unit as u8);
            }
            DtInst::Test {
                dst,
                src,
                datfmt,
                timfmt,
            } => {
                self.u8(3);
                self.reg(*dst);
                self.operand(src);
                self.date_fmt(*datfmt);
                self.time_fmt(*timfmt);
            }
            DtInst::Now { dst } => {
                self.u8(4);
                self.place(dst);
            }
        }
    }

    fn ptr_inst(&mut self, p: &PtrInst) {
        match p {
            PtrInst::Addr { dst, place } => {
                self.u8(0);
                self.reg(*dst);
                self.place(place);
            }
            PtrInst::PtrAdd { dst, ptr, n } => {
                self.u8(1);
                self.reg(*dst);
                self.operand(ptr);
                self.operand(n);
            }
            PtrInst::PtrDiff { dst, p, q } => {
                self.u8(2);
                self.reg(*dst);
                self.operand(p);
                self.operand(q);
            }
            PtrInst::ProcAddr { dst, callee } => {
                self.u8(3);
                self.reg(*dst);
                match callee {
                    Callee::Local(r) => {
                        self.u8(0);
                        self.u32(r.0);
                    }
                    Callee::Import(r) => {
                        self.u8(1);
                        self.u32(r.0);
                    }
                }
            }
            PtrInst::Rslvsp {
                dst,
                obj_type,
                name,
                lib,
            } => {
                self.u8(4);
                self.reg(*dst);
                self.str(obj_type);
                self.operand(name);
                self.operand(lib);
            }
        }
    }

    fn io_inst(&mut self, io: &IoInst) {
        match io {
            IoInst::DbOpen { dst, file, infds } => {
                self.u8(0);
                self.reg(*dst);
                self.u32(*file);
                self.opt_place(infds.as_ref());
            }
            IoInst::DbClose { dst, file } => {
                self.u8(1);
                self.reg(*dst);
                self.u32(*file);
            }
            IoInst::DbFb { dst, file } => {
                self.u8(4);
                self.reg(*dst);
                self.u32(*file);
            }
            IoInst::DbIo {
                dst,
                file,
                op,
                rec,
                key,
                rrn,
                fmt,
                nolock,
                nulls,
            } => {
                self.u8(2);
                self.reg(*dst);
                self.u32(*file);
                self.u8(*op as u8);
                self.opt_place(rec.as_ref());
                self.opt_operand(key.as_ref());
                self.opt_operand(rrn.as_ref());
                self.opt(fmt.as_ref(), |w, f| w.str(f));
                self.bool(*nolock);
                self.opt_place(nulls.as_ref());
            }
            IoInst::DevIo {
                dst,
                file,
                op,
                fmt,
                rec,
                ctl,
                rrn,
                inds,
            } => {
                self.u8(3);
                self.reg(*dst);
                self.u32(*file);
                self.u8(*op as u8);
                self.str(fmt);
                self.opt_place(rec.as_ref());
                self.opt(ctl.as_ref(), |w, c| {
                    for o in c.iter() {
                        w.operand(o);
                    }
                });
                self.opt_operand(rrn.as_ref());
                self.opt_place(inds.as_ref());
            }
            IoInst::FileRef { dst, file } => {
                self.u8(5);
                self.reg(*dst);
                self.u32(*file);
            }
            IoInst::FileBind { file, handle } => {
                self.u8(6);
                self.u32(*file);
                self.operand(handle);
            }
        }
    }

    fn msg_inst(&mut self, m: &MsgInst) {
        match m {
            MsgInst::Snd {
                msgid,
                msgf,
                msgf_lib,
                data,
                msg_type,
                to,
                key,
            } => {
                self.u8(0);
                self.operand(msgid);
                self.operand(msgf);
                self.opt_operand(msgf_lib.as_ref());
                self.opt_operand(data.as_ref());
                self.u8(*msg_type as u8);
                self.queue(to);
                self.opt_place(key.as_ref());
            }
            MsgInst::Rcv {
                dst,
                pgmq,
                msg_type,
                key,
                rmv,
                msg,
                msgid,
                data,
                keyout,
                sev,
            } => {
                self.u8(1);
                self.reg(*dst);
                self.queue(pgmq);
                self.opt(msg_type.as_ref(), |w, t| w.u8(*t as u8));
                self.opt_operand(key.as_ref());
                self.bool(*rmv);
                self.opt_place(msg.as_ref());
                self.opt_place(msgid.as_ref());
                self.opt_place(data.as_ref());
                self.opt_place(keyout.as_ref());
                self.opt_place(sev.as_ref());
            }
            MsgInst::Rmv { pgmq, key, clear } => {
                self.u8(2);
                self.queue(pgmq);
                self.opt_operand(key.as_ref());
                self.str(clear);
            }
        }
    }
}

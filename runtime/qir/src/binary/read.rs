// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Reading a module. Mirrors `write.rs` field for field.

use super::MAGIC;
use super::write::{DATE_FMTS, TIME_FMTS};
use crate::error::{QirError, Result};
use crate::inst::{
    Arg, BinOp, CallRet, CallTarget, Callee, Cond, Const, CvtOpts, DbOp, Debug, DecOp, DecUn,
    DevOp, DtInst, DtUnit, DtaaraOp, DtaqInst, Handler, Index, Inst, IoInst, MsgInst, MsgQueue,
    MsgType, Operand, Place, PlaceBase, PtrInst, Selector, Step, StrInst, TrimSide, UnOp,
};
use crate::module::{
    ActGrp, Block, DbgView, FORMAT_VERSION, FileDev, FileRef, FileUsage, FormatRef, Label, Lang,
    Linkage, Module, Options, Param, Proc, ProcRef, Proto, ProtoRef, Reg, RegInfo, Scope,
    StorageClass, TypeDef, Var, VarRef,
};
use crate::types::{Field, Shape, StructType, Type};

/// Read a module.
pub fn read(bytes: &[u8]) -> Result<Module> {
    let mut r = R { b: bytes, at: 0 };
    if r.take(4)? != MAGIC {
        return Err(bad("wrong magic"));
    }
    let version = r.u16()?;
    if version == 0 || version > FORMAT_VERSION {
        return Err(bad(format!(
            "format version {version} is not readable; this release reads up to {FORMAT_VERSION}"
        )));
    }
    r.u16()?;
    let m = r.module()?;
    if r.at != bytes.len() {
        return Err(bad("trailing bytes"));
    }
    Ok(m)
}

fn bad(msg: impl Into<String>) -> QirError {
    QirError::Binary { msg: msg.into() }
}

const BIN_OPS: [BinOp; 10] = [
    BinOp::Add,
    BinOp::Sub,
    BinOp::Mul,
    BinOp::Div,
    BinOp::Rem,
    BinOp::And,
    BinOp::Or,
    BinOp::Xor,
    BinOp::Shl,
    BinOp::Shr,
];
const UN_OPS: [UnOp; 3] = [UnOp::Neg, UnOp::Not, UnOp::Abs];
const CONDS: [Cond; 6] = [Cond::Eq, Cond::Ne, Cond::Lt, Cond::Le, Cond::Gt, Cond::Ge];
const DEC_OPS: [DecOp; 5] = [DecOp::Add, DecOp::Sub, DecOp::Mul, DecOp::Div, DecOp::Rem];
const DEC_UNS: [DecUn; 3] = [DecUn::Mov, DecUn::Neg, DecUn::Abs];
const DT_UNITS: [DtUnit; 7] = [
    DtUnit::Years,
    DtUnit::Months,
    DtUnit::Days,
    DtUnit::Hours,
    DtUnit::Minutes,
    DtUnit::Seconds,
    DtUnit::MSeconds,
];
const DB_OPS: [DbOp; 12] = [
    DbOp::Read,
    DbOp::ReadP,
    DbOp::ReadE,
    DbOp::ReadPE,
    DbOp::Chain,
    DbOp::SetLL,
    DbOp::SetGT,
    DbOp::Write,
    DbOp::Update,
    DbOp::Delete,
    DbOp::Unlock,
    DbOp::Feod,
];
const DEV_OPS: [DevOp; 3] = [DevOp::Write, DevOp::Read, DevOp::ExFmt];
const MSG_TYPES: [MsgType; 11] = [
    MsgType::Info,
    MsgType::Comp,
    MsgType::Diag,
    MsgType::Escape,
    MsgType::Status,
    MsgType::Notify,
    MsgType::Rqs,
    MsgType::Inq,
    MsgType::Rpy,
    MsgType::Scope,
    MsgType::Any,
];
const DTAARA_OPS: [DtaaraOp; 4] = [
    DtaaraOp::Lock,
    DtaaraOp::Read,
    DtaaraOp::Write,
    DtaaraOp::Unlock,
];

struct R<'a> {
    b: &'a [u8],
    at: usize,
}

impl R<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8]> {
        let end = self
            .at
            .checked_add(n)
            .ok_or_else(|| bad("length overflow"))?;
        if end > self.b.len() {
            return Err(bad(format!("truncated at byte {}", self.at)));
        }
        let s = &self.b[self.at..end];
        self.at = end;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }
    fn u32(&mut self) -> Result<u32> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn i64(&mut self) -> Result<i64> {
        let b = self.take(8)?;
        Ok(i64::from_le_bytes(b.try_into().expect("8 bytes")))
    }
    fn bool(&mut self) -> Result<bool> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            v => Err(bad(format!("{v} is not a boolean"))),
        }
    }
    fn bytes(&mut self) -> Result<Vec<u8>> {
        let n = self.u32()? as usize;
        Ok(self.take(n)?.to_vec())
    }
    fn str(&mut self) -> Result<String> {
        String::from_utf8(self.bytes()?).map_err(|_| bad("string is not UTF-8"))
    }
    fn opt<T>(&mut self, f: impl FnOnce(&mut Self) -> Result<T>) -> Result<Option<T>> {
        match self.u8()? {
            0 => Ok(None),
            1 => f(self).map(Some),
            v => Err(bad(format!("{v} is not a presence flag"))),
        }
    }
    fn list<T>(&mut self, mut f: impl FnMut(&mut Self) -> Result<T>) -> Result<Vec<T>> {
        let n = self.u32()? as usize;
        // A count beyond the remaining bytes is corrupt, not a request for memory.
        if n > self.b.len() - self.at {
            return Err(bad(format!("list of {n} items at byte {}", self.at)));
        }
        let mut out = Vec::with_capacity(n);
        for _ in 0..n {
            out.push(f(self)?);
        }
        Ok(out)
    }
    fn tag<T: Copy>(&mut self, table: &[T], what: &str) -> Result<T> {
        let v = self.u8()?;
        table
            .get(v as usize)
            .copied()
            .ok_or_else(|| bad(format!("{v} is not a {what}")))
    }

    fn module(&mut self) -> Result<Module> {
        let name = self.str()?;
        let lang = self.tag(&[Lang::Qir, Lang::Rpgle, Lang::Clle], "language")?;
        let ccsid = self.u16()?;
        let actgrp = match self.u8()? {
            0 => ActGrp::New,
            1 => ActGrp::Caller,
            2 => ActGrp::Named(self.str()?),
            v => return Err(bad(format!("{v} is not an activation group"))),
        };
        let dftactgrp = self.bool()?;
        let nomain = self.bool()?;
        let srcstmt = self.bool()?;
        let nodebugio = self.bool()?;
        let dbgview = self.tag(
            &[
                DbgView::None,
                DbgView::Stmt,
                DbgView::Source,
                DbgView::List,
                DbgView::All,
            ],
            "debug view",
        )?;
        let types = self.list(|r| {
            Ok(TypeDef {
                name: r.str()?,
                ty: r.ty()?,
            })
        })?;
        let vars = self.list(Self::var)?;
        let files = self.list(Self::file)?;
        let protos = self.list(Self::proto)?;
        let procs = self.list(Self::proc)?;
        Ok(Module {
            name,
            lang,
            ccsid,
            actgrp,
            dftactgrp,
            nomain,
            options: Options {
                srcstmt,
                nodebugio,
                dbgview,
            },
            types,
            vars,
            files,
            protos,
            procs,
        })
    }

    fn shape(&mut self) -> Result<Shape> {
        let d = self.u8()?;
        let s = self.u8()?;
        Shape::new(d, s).map_err(|e| bad(e.to_string()))
    }

    fn ty(&mut self) -> Result<Type> {
        Ok(match self.u8()? {
            0 => Type::I8,
            1 => Type::I16,
            2 => Type::I32,
            3 => Type::I64,
            4 => Type::U8,
            5 => Type::U16,
            6 => Type::U32,
            7 => Type::U64,
            8 => Type::F32,
            9 => Type::F64,
            10 => Type::Packed(self.shape()?),
            11 => Type::Zoned(self.shape()?),
            12 => Type::Char(self.u32()?),
            13 => Type::VarChar(self.u32()?),
            14 => Type::Ucs2(self.u32()?),
            15 => Type::VarUcs2(self.u32()?),
            16 => Type::Graphic(self.u32()?),
            17 => Type::Date(self.tag(&DATE_FMTS, "date format")?),
            18 => Type::Time(self.tag(&TIME_FMTS, "time format")?),
            19 => Type::Timestamp(self.u8()?),
            20 => Type::Ind,
            21 => Type::Ptr,
            22 => Type::ProcPtr,
            23 => {
                let size = self.u32()?;
                let fields = self.list(|r| {
                    Ok(Field {
                        name: r.str()?,
                        offset: r.u32()?,
                        ty: r.ty()?,
                    })
                })?;
                Type::Struct(Box::new(StructType { size, fields }))
            }
            24 => {
                let len = self.u32()?;
                Type::Array {
                    len,
                    elem: Box::new(self.ty()?),
                }
            }
            25 => Type::Bytes(self.u32()?),
            v => return Err(bad(format!("{v} is not a type"))),
        })
    }

    fn var_ref(&mut self) -> Result<VarRef> {
        let scope = self.tag(&[Scope::Module, Scope::Proc], "scope")?;
        Ok(VarRef {
            scope,
            index: self.u32()?,
        })
    }

    fn var(&mut self) -> Result<Var> {
        let name = self.str()?;
        let ty = self.ty()?;
        let class = match self.u8()? {
            0 => StorageClass::Static,
            1 => StorageClass::Auto,
            2 => StorageClass::Based(self.var_ref()?),
            3 => StorageClass::Import,
            4 => StorageClass::Const,
            5 => StorageClass::Param(self.u32()?),
            v => return Err(bad(format!("{v} is not a storage class"))),
        };
        let export = self.bool()?;
        let inz = self.opt(Self::bytes)?;
        Ok(Var {
            name,
            ty,
            class,
            export,
            inz,
        })
    }

    fn file(&mut self) -> Result<FileRef> {
        Ok(FileRef {
            name: self.str()?,
            file: self.str()?,
            lib: self.str()?,
            dev: self.tag(&[FileDev::Db, FileDev::Display, FileDev::Printer], "device")?,
            usage: self.tag(
                &[
                    FileUsage::Input,
                    FileUsage::Output,
                    FileUsage::Update,
                    FileUsage::All,
                ],
                "usage",
            )?,
            keyed: self.bool()?,
            lvlchk: self.bool()?,
            formats: self.list(|r| {
                Ok(FormatRef {
                    name: r.str()?,
                    level: r.opt(Self::str)?,
                })
            })?,
        })
    }

    fn param(&mut self) -> Result<Param> {
        Ok(Param {
            name: self.str()?,
            ty: self.ty()?,
            by_value: self.bool()?,
            nopass: self.bool()?,
            omit: self.bool()?,
            varsize: self.bool()?,
            const_: self.bool()?,
        })
    }

    fn proto(&mut self) -> Result<Proto> {
        let name = self.str()?;
        let params = self.list(Self::param)?;
        let ret = self.opt(Self::ty)?;
        let opdesc = self.bool()?;
        let linkage = match self.u8()? {
            0 => Linkage::Import,
            1 => Linkage::Pgm {
                name: self.str()?,
                lib: self.str()?,
            },
            2 => Linkage::SysApi,
            3 => Linkage::Signature,
            v => return Err(bad(format!("{v} is not a linkage"))),
        };
        Ok(Proto {
            name,
            params,
            ret,
            opdesc,
            linkage,
        })
    }

    fn proc(&mut self) -> Result<Proc> {
        Ok(Proc {
            name: self.str()?,
            params: self.list(Self::param)?,
            ret: self.opt(Self::ty)?,
            export: self.bool()?,
            entry: self.bool()?,
            opdesc: self.bool()?,
            vars: self.list(Self::var)?,
            regs: self.list(|r| {
                Ok(RegInfo {
                    name: r.str()?,
                    ty: r.ty()?,
                })
            })?,
            blocks: self.list(|r| {
                Ok(Block {
                    label: r.str()?,
                    insts: r.list(Self::inst)?,
                })
            })?,
        })
    }

    fn konst(&mut self) -> Result<Const> {
        Ok(match self.u8()? {
            0 => Const::Int(i128::from_le_bytes(
                self.take(16)?.try_into().expect("16 bytes"),
            )),
            1 => Const::Float(f64::from_bits(u64::from_le_bytes(
                self.take(8)?.try_into().expect("8 bytes"),
            ))),
            2 => Const::Dec(self.str()?),
            3 => Const::Str(self.str()?),
            4 => Const::Hex(self.bytes()?),
            5 => Const::Blanks,
            6 => Const::Zeros,
            7 => Const::HiVal,
            8 => Const::LoVal,
            9 => Const::Null,
            10 => Const::On,
            11 => Const::Off,
            v => return Err(bad(format!("{v} is not a literal"))),
        })
    }

    fn reg(&mut self) -> Result<Reg> {
        Ok(Reg(self.u32()?))
    }

    fn label(&mut self) -> Result<Label> {
        Ok(Label(self.u32()?))
    }

    fn place(&mut self) -> Result<Place> {
        let base = match self.u8()? {
            0 => PlaceBase::Var(self.var_ref()?),
            1 => PlaceBase::Deref {
                ptr: self.reg()?,
                offset: self.i64()?,
                ty: self.ty()?,
            },
            v => return Err(bad(format!("{v} is not a place base"))),
        };
        let path = self.list(|r| {
            Ok(match r.u8()? {
                0 => Step::Field(r.str()?),
                1 => Step::Index(Index::Reg(r.reg()?)),
                2 => Step::Index(Index::Const(r.u32()?)),
                v => return Err(bad(format!("{v} is not a place step"))),
            })
        })?;
        Ok(Place { base, path })
    }

    fn operand(&mut self) -> Result<Operand> {
        Ok(match self.u8()? {
            0 => Operand::Reg(self.reg()?),
            1 => Operand::Place(self.place()?),
            2 => Operand::Const(self.konst()?),
            v => return Err(bad(format!("{v} is not an operand"))),
        })
    }

    fn opt_operand(&mut self) -> Result<Option<Operand>> {
        self.opt(Self::operand)
    }

    fn opt_place(&mut self) -> Result<Option<Place>> {
        self.opt(Self::place)
    }

    fn queue(&mut self) -> Result<MsgQueue> {
        Ok(match self.u8()? {
            0 => MsgQueue::Same,
            1 => MsgQueue::Prv,
            2 => MsgQueue::CtlBdy,
            3 => MsgQueue::Ext,
            4 => MsgQueue::SysOpr,
            5 => MsgQueue::Up(self.operand()?),
            v => return Err(bad(format!("{v} is not a message queue"))),
        })
    }

    #[allow(clippy::too_many_lines)]
    fn inst(&mut self) -> Result<Inst> {
        Ok(match self.u8()? {
            0 => Inst::Const {
                dst: self.reg()?,
                value: self.konst()?,
            },
            1 => Inst::Load {
                dst: self.reg()?,
                src: self.place()?,
            },
            2 => Inst::Store {
                dst: self.place()?,
                src: self.operand()?,
            },
            3 => Inst::Mov {
                dst: self.place()?,
                src: self.operand()?,
            },
            4 => Inst::Bin {
                dst: self.reg()?,
                op: self.tag(&BIN_OPS, "binary operator")?,
                a: self.operand()?,
                b: self.operand()?,
                chk: self.bool()?,
            },
            5 => Inst::Un {
                dst: self.reg()?,
                op: self.tag(&UN_OPS, "unary operator")?,
                a: self.operand()?,
            },
            6 => Inst::Cmp {
                dst: self.reg()?,
                cond: self.tag(&CONDS, "condition")?,
                a: self.operand()?,
                b: self.operand()?,
            },
            7 => Inst::Select {
                dst: self.reg()?,
                c: self.operand()?,
                a: self.operand()?,
                b: self.operand()?,
            },
            8 => Inst::Cvt {
                dst: self.reg()?,
                src: self.operand()?,
                chk: self.bool()?,
            },
            9 => Inst::Br(self.label()?),
            10 => Inst::BrIf {
                c: self.operand()?,
                t: self.label()?,
                f: self.label()?,
            },
            11 => Inst::Switch {
                v: self.operand()?,
                cases: self.list(|r| Ok((r.i64()?, r.label()?)))?,
                default: self.label()?,
            },
            12 => Inst::Ret(self.opt_operand()?),
            13 => {
                let target = match self.u8()? {
                    0 => CallTarget::Local(ProcRef(self.u32()?)),
                    1 => CallTarget::Proto(ProtoRef(self.u32()?)),
                    2 => CallTarget::PgmName {
                        name: self.str()?,
                        lib: self.opt(Self::str)?,
                    },
                    3 => CallTarget::PgmVar(self.place()?),
                    4 => CallTarget::ProcPtr {
                        ptr: self.reg()?,
                        proto: ProtoRef(self.u32()?),
                    },
                    5 => CallTarget::SysPtr(self.reg()?),
                    v => return Err(bad(format!("{v} is not a call target"))),
                };
                let args = self.list(|r| {
                    Ok(match r.u8()? {
                        0 => Arg::Omit,
                        1 => Arg::Value(r.operand()?),
                        v => return Err(bad(format!("{v} is not an argument"))),
                    })
                })?;
                let ret = match self.u8()? {
                    0 => None,
                    1 => Some(CallRet::Reg(self.reg()?)),
                    2 => Some(CallRet::Place(self.place()?)),
                    v => return Err(bad(format!("{v} is not a call return"))),
                };
                Inst::Call { target, args, ret }
            }
            14 => Inst::Parms { dst: self.reg()? },
            15 => Inst::Passed {
                dst: self.reg()?,
                param: self.u32()?,
            },
            16 => Inst::Omitted {
                dst: self.reg()?,
                param: self.u32()?,
            },
            17 => Inst::Debug(match self.u8()? {
                0 => Debug::Stmt(self.u32()?),
                1 => Debug::Var {
                    name: self.str()?,
                    place: self.place()?,
                },
                2 => Debug::ScopeBegin,
                3 => Debug::ScopeEnd,
                v => return Err(bad(format!("{v} is not a debug instruction"))),
            }),
            18 => Inst::Unreachable,
            19 => Inst::DecBin {
                dst: self.place()?,
                op: self.tag(&DEC_OPS, "decimal operator")?,
                a: self.operand()?,
                b: self.operand()?,
                prec: self.shape()?,
                halfadj: self.bool()?,
                resdecpos: self.bool()?,
            },
            20 => Inst::DecUn {
                dst: self.place()?,
                op: self.tag(&DEC_UNS, "decimal assignment")?,
                src: self.operand()?,
                halfadj: self.bool()?,
            },
            21 => Inst::DecCmp {
                dst: self.reg()?,
                cond: self.tag(&CONDS, "condition")?,
                a: self.operand()?,
                b: self.operand()?,
            },
            22 => Inst::Convert {
                dst: self.place()?,
                src: self.operand()?,
                opts: self.cvt_opts()?,
            },
            23 => Inst::Str(self.str_inst()?),
            24 => Inst::Dt(self.dt_inst()?),
            25 => Inst::Ptr(self.ptr_inst()?),
            26 => Inst::Monitor {
                name: self.str()?,
                handlers: self.list(|r| {
                    let selector = match r.u8()? {
                        0 => Selector::All,
                        1 => Selector::Program,
                        2 => Selector::File,
                        3 => Selector::Status(r.u16()?, r.u16()?),
                        4 => Selector::MsgId(r.str()?),
                        v => return Err(bad(format!("{v} is not a selector"))),
                    };
                    Ok(Handler {
                        selector,
                        target: r.label()?,
                    })
                })?,
            },
            27 => Inst::EndMon(self.str()?),
            28 => Inst::Io(self.io_inst()?),
            29 => Inst::Msg(self.msg_inst()?),
            30 => Inst::Dsply {
                src: self.operand()?,
                reply: self.opt_place()?,
            },
            31 => Inst::Dtaara {
                op: self.tag(&DTAARA_OPS, "data area operation")?,
                name: self.operand()?,
                lib: self.operand()?,
                data: self.opt_place()?,
            },
            34 => Inst::Alloc {
                dst: self.reg()?,
                n: self.operand()?,
            },
            35 => Inst::Realloc {
                dst: self.reg()?,
                ptr: self.operand()?,
                n: self.operand()?,
            },
            36 => Inst::Dealloc {
                ptr: self.operand()?,
            },
            32 => Inst::Dtaq(DtaqInst::Snd {
                name: self.operand()?,
                lib: self.operand()?,
                data: self.operand()?,
                len: self.operand()?,
            }),
            33 => Inst::Dtaq(DtaqInst::Rcv {
                dst: self.reg()?,
                name: self.operand()?,
                lib: self.operand()?,
                data: self.place()?,
                len: self.operand()?,
                wait: self.operand()?,
            }),
            v => return Err(bad(format!("{v} is not an instruction"))),
        })
    }

    fn cvt_opts(&mut self) -> Result<CvtOpts> {
        Ok(CvtOpts {
            halfadj: self.bool()?,
            datfmt: self.opt(|r| r.tag(&DATE_FMTS, "date format"))?,
            timfmt: self.opt(|r| r.tag(&TIME_FMTS, "time format"))?,
            ccsid: self.opt(Self::u16)?,
        })
    }

    fn str_inst(&mut self) -> Result<StrInst> {
        Ok(match self.u8()? {
            0 => StrInst::Cat {
                dst: self.place()?,
                a: self.operand()?,
                b: self.operand()?,
            },
            1 => StrInst::Subst {
                dst: self.place()?,
                src: self.operand()?,
                start: self.operand()?,
                len: self.operand()?,
            },
            2 => StrInst::Scan {
                dst: self.reg()?,
                needle: self.operand()?,
                hay: self.operand()?,
                start: self.opt_operand()?,
            },
            3 => StrInst::Check {
                dst: self.reg()?,
                set: self.operand()?,
                src: self.operand()?,
                start: self.opt_operand()?,
                reverse: self.bool()?,
            },
            4 => StrInst::Xlate {
                dst: self.place()?,
                from: self.operand()?,
                to: self.operand()?,
                src: self.operand()?,
                start: self.opt_operand()?,
            },
            5 => StrInst::Trim {
                dst: self.place()?,
                src: self.operand()?,
                chars: self.opt_operand()?,
                side: self.tag(
                    &[TrimSide::Both, TrimSide::Left, TrimSide::Right],
                    "trim side",
                )?,
            },
            6 => StrInst::Cmp {
                dst: self.reg()?,
                cond: self.tag(&CONDS, "condition")?,
                a: self.operand()?,
                b: self.operand()?,
            },
            7 => StrInst::Len {
                dst: self.reg()?,
                src: self.operand()?,
            },
            8 => StrInst::SetLen {
                dst: self.place()?,
                n: self.operand()?,
            },
            9 => StrInst::Replace {
                dst: self.place()?,
                src: self.operand()?,
                repl: self.operand()?,
                start: self.operand()?,
                len: self.opt_operand()?,
            },
            v => return Err(bad(format!("{v} is not a string instruction"))),
        })
    }

    fn dt_inst(&mut self) -> Result<DtInst> {
        Ok(match self.u8()? {
            0 => DtInst::Add {
                dst: self.place()?,
                src: self.operand()?,
                n: self.operand()?,
                unit: self.tag(&DT_UNITS, "date unit")?,
            },
            1 => DtInst::Diff {
                dst: self.reg()?,
                a: self.operand()?,
                b: self.operand()?,
                unit: self.tag(&DT_UNITS, "date unit")?,
            },
            2 => DtInst::Extract {
                dst: self.reg()?,
                src: self.operand()?,
                unit: self.tag(&DT_UNITS, "date unit")?,
            },
            3 => DtInst::Test {
                dst: self.reg()?,
                src: self.operand()?,
                datfmt: self.opt(|r| r.tag(&DATE_FMTS, "date format"))?,
                timfmt: self.opt(|r| r.tag(&TIME_FMTS, "time format"))?,
            },
            4 => DtInst::Now { dst: self.place()? },
            v => return Err(bad(format!("{v} is not a date instruction"))),
        })
    }

    fn ptr_inst(&mut self) -> Result<PtrInst> {
        Ok(match self.u8()? {
            0 => PtrInst::Addr {
                dst: self.reg()?,
                place: self.place()?,
            },
            1 => PtrInst::PtrAdd {
                dst: self.reg()?,
                ptr: self.operand()?,
                n: self.operand()?,
            },
            2 => PtrInst::PtrDiff {
                dst: self.reg()?,
                p: self.operand()?,
                q: self.operand()?,
            },
            3 => PtrInst::ProcAddr {
                dst: self.reg()?,
                callee: match self.u8()? {
                    0 => Callee::Local(ProcRef(self.u32()?)),
                    1 => Callee::Import(ProtoRef(self.u32()?)),
                    v => return Err(bad(format!("{v} is not a callee"))),
                },
            },
            4 => PtrInst::Rslvsp {
                dst: self.reg()?,
                obj_type: self.str()?,
                name: self.operand()?,
                lib: self.operand()?,
            },
            v => return Err(bad(format!("{v} is not a pointer instruction"))),
        })
    }

    fn io_inst(&mut self) -> Result<IoInst> {
        Ok(match self.u8()? {
            0 => IoInst::DbOpen {
                dst: self.reg()?,
                file: self.u32()?,
                infds: self.opt_place()?,
            },
            1 => IoInst::DbClose {
                dst: self.reg()?,
                file: self.u32()?,
            },
            2 => IoInst::DbIo {
                dst: self.reg()?,
                file: self.u32()?,
                op: self.tag(&DB_OPS, "database operation")?,
                rec: self.opt_place()?,
                key: self.opt_operand()?,
                rrn: self.opt_operand()?,
                fmt: self.opt(Self::str)?,
                nolock: self.bool()?,
            },
            3 => IoInst::DevIo {
                dst: self.reg()?,
                file: self.u32()?,
                op: self.tag(&DEV_OPS, "device operation")?,
                fmt: self.str()?,
                rec: self.opt_place()?,
            },
            v => return Err(bad(format!("{v} is not an I/O instruction"))),
        })
    }

    fn msg_inst(&mut self) -> Result<MsgInst> {
        Ok(match self.u8()? {
            0 => MsgInst::Snd {
                msgid: self.operand()?,
                msgf: self.operand()?,
                msgf_lib: self.opt_operand()?,
                data: self.opt_operand()?,
                msg_type: self.tag(&MSG_TYPES, "message type")?,
                to: self.queue()?,
                key: self.opt_place()?,
            },
            1 => MsgInst::Rcv {
                dst: self.reg()?,
                pgmq: self.queue()?,
                msg_type: self.opt(|r| r.tag(&MSG_TYPES, "message type"))?,
                key: self.opt_operand()?,
                rmv: self.bool()?,
                msg: self.opt_place()?,
                msgid: self.opt_place()?,
                data: self.opt_place()?,
                keyout: self.opt_place()?,
                sev: self.opt_place()?,
            },
            2 => MsgInst::Rmv {
                pgmq: self.queue()?,
                key: self.opt_operand()?,
                clear: self.str()?,
            },
            v => return Err(bad(format!("{v} is not a message instruction"))),
        })
    }
}

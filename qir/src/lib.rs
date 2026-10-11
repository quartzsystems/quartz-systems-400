// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The QS/400 intermediate representation: the types every compiled
//! program's data has, with IBM's byte layouts; the instructions the
//! RPG and CL compilers lower to; `*MODULE` units with their static
//! frames, imports, exports and file references; a text form (`.qir`)
//! with a parser and printer; and a verifier. `docs/QIR.md` is the
//! specification.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod error;

pub mod inst;
pub mod module;
pub mod text;
pub mod types;
pub mod verify;

pub use error::{QirError, Result};
pub use inst::{
    Arg, CallRet, CallTarget, Callee, Const, CvtOpts, DbOp, DecOp, DecUn, DevOp, DtInst, DtUnit,
    DtaaraOp, DtaqInst, Handler, Inst, IoInst, MsgInst, MsgQueue, MsgType, Operand, Place,
    PlaceBase, PtrInst, Selector, Step, StrInst, TrimSide,
};
pub use module::{
    ActGrp, Block, DbgView, FORMAT_VERSION, FileDev, FileRef, FileUsage, FormatRef, Label, Lang,
    Linkage, Module, Options, Param, Proc, ProcRef, Proto, ProtoRef, Reg, RegInfo, Scope,
    StorageClass, TypeDef, Var, VarRef,
};
pub use text::{parse, print};
pub use types::{DateFmt, Field, Shape, StructType, TimeFmt, Type, TypeClass};
pub use verify::verify;

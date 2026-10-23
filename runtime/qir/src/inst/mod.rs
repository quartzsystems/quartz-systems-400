// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The instruction set. Scalars live in typed registers; everything
//! with an IBM byte layout lives in storage and is named by a place.
//! Instructions that work on decimals, strings, dates and pointers take
//! places and lower to runtime calls.

mod ext;
pub mod literal;
mod ops;

pub use ext::{
    Callee, CvtOpts, DbOp, DecOp, DecUn, DevOp, DtInst, DtUnit, DtaaraOp, DtaqInst, Handler,
    IoInst, MsgInst, MsgQueue, MsgType, PtrInst, Selector, StrInst, TrimSide,
};
pub use ops::{BinOp, Cond, UnOp};

use crate::module::{Label, ProcRef, ProtoRef, Reg, VarRef};
use crate::types::{Shape, Type};

/// A literal.
#[derive(Clone, Debug, PartialEq)]
pub enum Const {
    /// An integer; the type comes from the context.
    Int(i128),
    /// A float.
    Float(f64),
    /// A decimal literal, as written (`123.45`).
    Dec(String),
    /// A character literal in Unicode; CP037 in storage.
    Str(String),
    /// Hexadecimal bytes.
    Hex(Vec<u8>),
    /// `*BLANKS`.
    Blanks,
    /// `*ZEROS`.
    Zeros,
    /// `*HIVAL`.
    HiVal,
    /// `*LOVAL`.
    LoVal,
    /// `*NULL`.
    Null,
    /// `*ON`.
    On,
    /// `*OFF`.
    Off,
}

/// An index into an array.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Index {
    /// A register holding an integer; zero-based.
    Reg(Reg),
    /// A constant, zero-based.
    Const(u32),
}

/// A step from a place into a part of it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    /// A field of a structure.
    Field(String),
    /// An element of an array.
    Index(Index),
}

/// Where a place starts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlaceBase {
    /// A variable.
    Var(VarRef),
    /// Storage a pointer register addresses, at a byte offset, viewed
    /// as a type.
    Deref {
        /// The pointer.
        ptr: Reg,
        /// Byte offset added to the pointer.
        offset: i64,
        /// The type seen there.
        ty: Type,
    },
}

/// A storage location with a type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Place {
    /// The start.
    pub base: PlaceBase,
    /// Fields and indexes applied in order.
    pub path: Vec<Step>,
}

impl Place {
    /// A whole variable.
    #[must_use]
    pub fn var(r: VarRef) -> Self {
        Self {
            base: PlaceBase::Var(r),
            path: Vec::new(),
        }
    }
}

/// An instruction operand.
#[derive(Clone, Debug, PartialEq)]
pub enum Operand {
    /// A register.
    Reg(Reg),
    /// A place.
    Place(Place),
    /// A literal.
    Const(Const),
}

/// A call argument.
#[derive(Clone, Debug, PartialEq)]
pub enum Arg {
    /// `*OMIT`.
    Omit,
    /// A value or place.
    Value(Operand),
}

/// Where a call's return value goes.
#[derive(Clone, Debug, PartialEq)]
pub enum CallRet {
    /// A register, for register types.
    Reg(Reg),
    /// A place, for storage types.
    Place(Place),
}

/// What a call reaches.
#[derive(Clone, Debug, PartialEq)]
pub enum CallTarget {
    /// A procedure of this module (`callb`).
    Local(ProcRef),
    /// A prototype: an import (`callb`), a program (`call`) or a system
    /// API (`sysapi`).
    Proto(ProtoRef),
    /// A program by name, untyped (`call`).
    PgmName {
        /// Program name.
        name: String,
        /// Library; `None` is `*LIBL`.
        lib: Option<String>,
    },
    /// A program whose name a place holds: `char(10)`, or `char(20)`
    /// with the library in the second half (`call`).
    PgmVar(Place),
    /// A procedure pointer with the signature of a prototype (`callpp`).
    ProcPtr {
        /// The pointer.
        ptr: Reg,
        /// The signature.
        proto: ProtoRef,
    },
    /// A system pointer to a program (`callp`).
    SysPtr(Reg),
}

/// A debug pseudo-instruction.
#[derive(Clone, Debug, PartialEq)]
pub enum Debug {
    /// Statement `n` starts here.
    Stmt(u32),
    /// The source name of a place, for `EVAL`.
    Var {
        /// Source name.
        name: String,
        /// Where it lives.
        place: Place,
    },
    /// A nested scope opens.
    ScopeBegin,
    /// A nested scope closes.
    ScopeEnd,
}

/// An instruction.
#[derive(Clone, Debug, PartialEq)]
pub enum Inst {
    /// `%d = const.T value`.
    Const {
        /// Destination.
        dst: Reg,
        /// Value.
        value: Const,
    },
    /// `%d = load place`: a scalar from storage.
    Load {
        /// Destination.
        dst: Reg,
        /// Source.
        src: Place,
    },
    /// `store place, value`: a scalar to storage.
    Store {
        /// Destination.
        dst: Place,
        /// Value.
        src: Operand,
    },
    /// `mov place, src`: copy between places of the same class, text
    /// padded or truncated, or a literal into storage.
    Mov {
        /// Destination.
        dst: Place,
        /// Source.
        src: Operand,
    },
    /// `%d = op[.chk] a, b`.
    Bin {
        /// Destination.
        dst: Reg,
        /// Operator.
        op: BinOp,
        /// Left operand.
        a: Operand,
        /// Right operand.
        b: Operand,
        /// Raise `MCH1210` on overflow instead of wrapping.
        chk: bool,
    },
    /// `%d = op a`.
    Un {
        /// Destination.
        dst: Reg,
        /// Operator.
        op: UnOp,
        /// Operand.
        a: Operand,
    },
    /// `%d = cmp.cond a, b`: an `i8` of 0 or 1.
    Cmp {
        /// Destination.
        dst: Reg,
        /// Condition.
        cond: Cond,
        /// Left operand.
        a: Operand,
        /// Right operand.
        b: Operand,
    },
    /// `%d = select c, a, b`.
    Select {
        /// Destination.
        dst: Reg,
        /// Condition, nonzero picks `a`.
        c: Operand,
        /// Value when true.
        a: Operand,
        /// Value when false.
        b: Operand,
    },
    /// `%d = cvt.T a`: between register types; integer narrowing wraps
    /// unless `.chk`, float to integer truncates toward zero.
    Cvt {
        /// Destination; its type is the target.
        dst: Reg,
        /// Source.
        src: Operand,
        /// Raise `MCH1210` when the value does not fit.
        chk: bool,
    },
    /// `br L`.
    Br(Label),
    /// `brif c, Lt, Lf`.
    BrIf {
        /// Condition, nonzero is true.
        c: Operand,
        /// Target when true.
        t: Label,
        /// Target when false.
        f: Label,
    },
    /// `switch v, [k: L, ...], default L`.
    Switch {
        /// Value.
        v: Operand,
        /// Cases.
        cases: Vec<(i64, Label)>,
        /// Default target.
        default: Label,
    },
    /// `ret [value]`.
    Ret(Option<Operand>),
    /// A call.
    Call {
        /// Target.
        target: CallTarget,
        /// Arguments.
        args: Vec<Arg>,
        /// Return destination.
        ret: Option<CallRet>,
    },
    /// `%d = parms`: the number of parameters passed, an `i32`.
    Parms {
        /// Destination.
        dst: Reg,
    },
    /// `%d = passed $p`: 1 when the parameter was passed and not
    /// omitted, an `i8`.
    Passed {
        /// Destination.
        dst: Reg,
        /// Parameter index.
        param: u32,
    },
    /// `%d = omitted $p`: 1 when the parameter was passed as `*OMIT`.
    Omitted {
        /// Destination.
        dst: Reg,
        /// Parameter index.
        param: u32,
    },
    /// `debug.*`.
    Debug(Debug),
    /// `unreachable`: a trap; control never arrives here.
    Unreachable,
    /// `dec.op dst, a, b prec(d,s) [halfadj] [resdecpos]`: the exact
    /// result truncated to the stated precision (`MCH1210` when its
    /// integer part does not fit), then assigned to `dst`.
    DecBin {
        /// Destination, numeric.
        dst: Place,
        /// Operator.
        op: DecOp,
        /// Left operand.
        a: Operand,
        /// Right operand.
        b: Operand,
        /// Intermediate precision.
        prec: Shape,
        /// Half-adjust on assignment.
        halfadj: bool,
        /// `*RESDECPOS`: the intermediate keeps at least `dst`'s decimals.
        resdecpos: bool,
    },
    /// `dec.mov|neg|abs dst, src [halfadj] [trunc]`: numeric assignment
    /// between any numeric types, a register or a literal.
    DecUn {
        /// Destination, numeric.
        dst: Place,
        /// Variant.
        op: DecUn,
        /// Source.
        src: Operand,
        /// Half-adjust.
        halfadj: bool,
        /// Drop high-order digits that do not fit instead of `MCH1210`
        /// (fixed-form RPG arithmetic); format 2.
        trunc: bool,
    },
    /// `%d = dec.cmp.cond a, b`: numeric comparison by value.
    DecCmp {
        /// Destination, an `ind`.
        dst: Reg,
        /// Condition.
        cond: Cond,
        /// Left.
        a: Operand,
        /// Right.
        b: Operand,
    },
    /// `cvt dst, src [halfadj] [datfmt(*X)] [timfmt(*X)] [ccsid(n)]`:
    /// conversion between storage types of different classes.
    Convert {
        /// Destination.
        dst: Place,
        /// Source.
        src: Operand,
        /// Options.
        opts: CvtOpts,
    },
    /// `str.*`.
    Str(StrInst),
    /// `dt.*`.
    Dt(DtInst),
    /// Pointer instructions.
    Ptr(PtrInst),
    /// `monitor NAME [*SEL -> L]...`: opens a region that `endmon NAME`
    /// closes; an exception raised inside goes to the first matching
    /// handler.
    Monitor {
        /// Region name.
        name: String,
        /// Handlers in order.
        handlers: Vec<Handler>,
    },
    /// `endmon NAME`.
    EndMon(String),
    /// Database, display and printer I/O.
    Io(IoInst),
    /// Program messages.
    Msg(MsgInst),
    /// `dsply src [reply(place)]`: console echo, a reply when asked.
    Dsply {
        /// What to show.
        src: Operand,
        /// Receives the reply.
        reply: Option<Place>,
    },
    /// `dtaara.op name, lib [, place]`.
    Dtaara {
        /// Operation.
        op: DtaaraOp,
        /// Data area name, `char(10)`.
        name: Operand,
        /// Library, `char(10)`.
        lib: Operand,
        /// The data read or written.
        data: Option<Place>,
    },
    /// Data queues.
    Dtaq(DtaqInst),
    /// `%d = alloc n`: `n` bytes from the activation group's heap.
    Alloc {
        /// Destination, a `ptr`.
        dst: Reg,
        /// Bytes.
        n: Operand,
    },
    /// `%d = realloc p, n`: the storage `p` addresses resized to `n`
    /// bytes, its contents kept; the old pointer is dead afterwards.
    Realloc {
        /// Destination, a `ptr`.
        dst: Reg,
        /// Storage from `alloc`.
        ptr: Operand,
        /// Bytes.
        n: Operand,
    },
    /// `dealloc p`: free storage from `alloc`; a pointer that is not
    /// from the heap is `MCH3601`.
    Dealloc {
        /// Storage from `alloc`.
        ptr: Operand,
    },
}

impl Inst {
    /// Whether the instruction ends a block.
    #[must_use]
    pub fn is_terminator(&self) -> bool {
        matches!(
            self,
            Self::Br(_)
                | Self::BrIf { .. }
                | Self::Switch { .. }
                | Self::Ret(_)
                | Self::Unreachable
        )
    }

    /// The register the instruction defines, if any.
    #[must_use]
    pub fn def(&self) -> Option<Reg> {
        match self {
            Self::Const { dst, .. }
            | Self::Load { dst, .. }
            | Self::Bin { dst, .. }
            | Self::Un { dst, .. }
            | Self::Cmp { dst, .. }
            | Self::Select { dst, .. }
            | Self::Cvt { dst, .. }
            | Self::Parms { dst }
            | Self::Passed { dst, .. }
            | Self::Omitted { dst, .. }
            | Self::DecCmp { dst, .. }
            | Self::Str(
                StrInst::Scan { dst, .. }
                | StrInst::Check { dst, .. }
                | StrInst::Cmp { dst, .. }
                | StrInst::Len { dst, .. },
            )
            | Self::Dt(
                DtInst::Diff { dst, .. } | DtInst::Extract { dst, .. } | DtInst::Test { dst, .. },
            )
            | Self::Ptr(
                PtrInst::Addr { dst, .. }
                | PtrInst::PtrAdd { dst, .. }
                | PtrInst::PtrDiff { dst, .. }
                | PtrInst::ProcAddr { dst, .. }
                | PtrInst::Rslvsp { dst, .. },
            )
            | Self::Io(
                IoInst::DbOpen { dst, .. }
                | IoInst::DbClose { dst, .. }
                | IoInst::DbIo { dst, .. }
                | IoInst::DevIo { dst, .. },
            )
            | Self::Msg(MsgInst::Rcv { dst, .. })
            | Self::Dtaq(DtaqInst::Rcv { dst, .. })
            | Self::Alloc { dst, .. }
            | Self::Realloc { dst, .. } => Some(*dst),
            Self::Call {
                ret: Some(CallRet::Reg(r)),
                ..
            } => Some(*r),
            _ => None,
        }
    }

    /// The labels a `monitor` hands control to.
    #[must_use]
    pub fn handler_targets(&self) -> Vec<Label> {
        match self {
            Self::Monitor { handlers, .. } => handlers.iter().map(|h| h.target).collect(),
            _ => Vec::new(),
        }
    }

    /// The labels the instruction branches to.
    #[must_use]
    pub fn targets(&self) -> Vec<Label> {
        match self {
            Self::Br(l) => vec![*l],
            Self::BrIf { t, f, .. } => vec![*t, *f],
            Self::Switch { cases, default, .. } => {
                let mut v: Vec<Label> = cases.iter().map(|(_, l)| *l).collect();
                v.push(*default);
                v
            }
            _ => Vec::new(),
        }
    }
}

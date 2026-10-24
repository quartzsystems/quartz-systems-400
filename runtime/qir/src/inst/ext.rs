// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The storage instructions: decimal arithmetic, conversions, strings,
//! pointers, dates, monitors, I/O, messages, data areas and queues.

use super::{Cond, Operand, Place};
use crate::module::{Label, ProcRef, ProtoRef, Reg};
use crate::types::{DateFmt, TimeFmt};

/// A decimal arithmetic operator.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DecOp {
    /// `dec.add`.
    Add,
    /// `dec.sub`.
    Sub,
    /// `dec.mul`.
    Mul,
    /// `dec.div`; `MCH1211` on a zero divisor.
    Div,
    /// `dec.rem`: the remainder of `dec.div` truncated to an integer.
    Rem,
}

impl DecOp {
    /// The opcode suffix.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Add => "add",
            Self::Sub => "sub",
            Self::Mul => "mul",
            Self::Div => "div",
            Self::Rem => "rem",
        }
    }

    /// From the suffix.
    #[must_use]
    pub fn from_name(s: &str) -> Option<Self> {
        Some(match s {
            "add" => Self::Add,
            "sub" => Self::Sub,
            "mul" => Self::Mul,
            "div" => Self::Div,
            "rem" => Self::Rem,
            _ => return None,
        })
    }
}

/// A decimal assignment variant.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DecUn {
    /// `dec.mov`: assign.
    Mov,
    /// `dec.neg`: assign the negation.
    Neg,
    /// `dec.abs`: assign the absolute value.
    Abs,
}

impl DecUn {
    /// The opcode suffix.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Mov => "mov",
            Self::Neg => "neg",
            Self::Abs => "abs",
        }
    }
}

/// Options of a storage conversion.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CvtOpts {
    /// Half-adjust when a numeric result loses decimal positions.
    pub halfadj: bool,
    /// The date format of the character or numeric side.
    pub datfmt: Option<DateFmt>,
    /// The time format of the character or numeric side.
    pub timfmt: Option<TimeFmt>,
    /// The CCSID of the character side when it is not the module's.
    pub ccsid: Option<u16>,
}

/// Which side `str.trim` trims.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TrimSide {
    /// Both ends.
    Both,
    /// Leading characters.
    Left,
    /// Trailing characters.
    Right,
}

/// A string instruction.
#[derive(Clone, Debug, PartialEq)]
pub enum StrInst {
    /// `str.cat dst, a, b`: `a` then `b`, padded or truncated to `dst`.
    Cat {
        /// Destination.
        dst: Place,
        /// First part.
        a: Operand,
        /// Second part.
        b: Operand,
    },
    /// `str.subst dst, src, start, len`: `len` characters from the
    /// 1-based `start`; `MCH0603` when outside the source.
    Subst {
        /// Destination.
        dst: Place,
        /// Source.
        src: Operand,
        /// Start, from 1.
        start: Operand,
        /// Length.
        len: Operand,
    },
    /// `%d = str.scan needle, hay [, start]`: the 1-based position of
    /// `needle` in `hay`, 0 when absent.
    Scan {
        /// Destination, an `i32`.
        dst: Reg,
        /// What to find.
        needle: Operand,
        /// Where to look.
        hay: Operand,
        /// Start, from 1.
        start: Option<Operand>,
    },
    /// `%d = str.check set, src [, start]` and `str.checkr`: the 1-based
    /// position of the first (last) character of `src` not in `set`.
    Check {
        /// Destination, an `i32`.
        dst: Reg,
        /// The allowed characters.
        set: Operand,
        /// What to check.
        src: Operand,
        /// Start, from 1.
        start: Option<Operand>,
        /// From the right.
        reverse: bool,
    },
    /// `str.xlate dst, from, to, src [, start]`.
    Xlate {
        /// Destination.
        dst: Place,
        /// Characters to translate.
        from: Operand,
        /// What they become.
        to: Operand,
        /// Source.
        src: Operand,
        /// Start, from 1.
        start: Option<Operand>,
    },
    /// `str.trim|triml|trimr dst, src [, chars]`.
    Trim {
        /// Destination.
        dst: Place,
        /// Source.
        src: Operand,
        /// Characters to trim; blanks by default.
        chars: Option<Operand>,
        /// Which side.
        side: TrimSide,
    },
    /// `%d = str.cmp.cond a, b`: blank-padded EBCDIC comparison.
    Cmp {
        /// Destination, an `ind`.
        dst: Reg,
        /// Condition.
        cond: Cond,
        /// Left.
        a: Operand,
        /// Right.
        b: Operand,
    },
    /// `%d = str.len src`: the current length of a varying string or
    /// the declared length of a fixed one.
    Len {
        /// Destination, an `i32`.
        dst: Reg,
        /// Source.
        src: Operand,
    },
    /// `str.setlen dst, n`: the current length of a varying string.
    SetLen {
        /// Destination, varying.
        dst: Place,
        /// New length.
        n: Operand,
    },
    /// `str.replace dst, src, repl, start [, len]`: `src` with `len`
    /// characters at `start` replaced by `repl`.
    Replace {
        /// Destination.
        dst: Place,
        /// Source.
        src: Operand,
        /// Replacement.
        repl: Operand,
        /// Start, from 1.
        start: Operand,
        /// Characters replaced; the replacement's length by default.
        len: Option<Operand>,
    },
}

/// A date unit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DtUnit {
    /// `*YEARS`.
    Years,
    /// `*MONTHS`.
    Months,
    /// `*DAYS`.
    Days,
    /// `*HOURS`.
    Hours,
    /// `*MINUTES`.
    Minutes,
    /// `*SECONDS`.
    Seconds,
    /// `*MSECONDS`.
    MSeconds,
}

impl DtUnit {
    /// The special value.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Years => "*YEARS",
            Self::Months => "*MONTHS",
            Self::Days => "*DAYS",
            Self::Hours => "*HOURS",
            Self::Minutes => "*MINUTES",
            Self::Seconds => "*SECONDS",
            Self::MSeconds => "*MSECONDS",
        }
    }

    /// From the special value.
    #[must_use]
    pub fn from_name(s: &str) -> Option<Self> {
        Some(match s {
            "*YEARS" | "*Y" => Self::Years,
            "*MONTHS" | "*M" => Self::Months,
            "*DAYS" | "*D" => Self::Days,
            "*HOURS" | "*H" => Self::Hours,
            "*MINUTES" | "*MN" => Self::Minutes,
            "*SECONDS" | "*S" => Self::Seconds,
            "*MSECONDS" | "*MS" => Self::MSeconds,
            _ => return None,
        })
    }

    /// A date unit, as opposed to a time unit.
    #[must_use]
    pub fn is_date(self) -> bool {
        matches!(self, Self::Years | Self::Months | Self::Days)
    }
}

/// A date, time and timestamp instruction.
#[derive(Clone, Debug, PartialEq)]
pub enum DtInst {
    /// `dt.add dst, src, n, unit`: `src` plus `n` units; `n` may be negative.
    Add {
        /// Destination, the type of `src`.
        dst: Place,
        /// Source.
        src: Operand,
        /// Count.
        n: Operand,
        /// Unit.
        unit: DtUnit,
    },
    /// `%d = dt.diff a, b, unit`: `a - b` in whole units.
    Diff {
        /// Destination, an `i64`.
        dst: Reg,
        /// Left.
        a: Operand,
        /// Right.
        b: Operand,
        /// Unit.
        unit: DtUnit,
    },
    /// `%d = dt.extract src, unit`: one part of the value.
    Extract {
        /// Destination, an `i32`.
        dst: Reg,
        /// Source.
        src: Operand,
        /// Which part.
        unit: DtUnit,
    },
    /// `%d = dt.test src [datfmt(*X)] [timfmt(*X)]`: whether character
    /// or numeric data is a valid date, time or timestamp.
    Test {
        /// Destination, an `ind`.
        dst: Reg,
        /// Source.
        src: Operand,
        /// Date format of the source.
        datfmt: Option<DateFmt>,
        /// Time format of the source.
        timfmt: Option<TimeFmt>,
    },
    /// `dt.now dst`: the current date, time or timestamp.
    Now {
        /// Destination.
        dst: Place,
    },
}

/// A bound procedure a pointer can name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Callee {
    /// A procedure of this module.
    Local(ProcRef),
    /// An `extern proc`.
    Import(ProtoRef),
}

/// A pointer instruction.
#[derive(Clone, Debug, PartialEq)]
pub enum PtrInst {
    /// `%d = addr place`.
    Addr {
        /// Destination, a `ptr`.
        dst: Reg,
        /// What it points to.
        place: Place,
    },
    /// `%d = ptradd p, n`: `n` bytes on; `MCH0601` outside the region.
    PtrAdd {
        /// Destination, a `ptr`.
        dst: Reg,
        /// Base.
        ptr: Operand,
        /// Byte count.
        n: Operand,
    },
    /// `%d = ptrdiff p, q`: `p - q` in bytes; `MCH0601` across regions.
    PtrDiff {
        /// Destination, an `i64`.
        dst: Reg,
        /// Left.
        p: Operand,
        /// Right.
        q: Operand,
    },
    /// `%d = procaddr NAME`.
    ProcAddr {
        /// Destination, a `procptr`.
        dst: Reg,
        /// The procedure.
        callee: Callee,
    },
    /// `%d = rslvsp *TYPE, name, lib`: a system pointer to an object;
    /// `MCH3401` when it does not exist.
    Rslvsp {
        /// Destination, a `ptr`.
        dst: Reg,
        /// Object type, such as `*PGM`.
        obj_type: String,
        /// Object name, `char(10)`.
        name: Operand,
        /// Library, `char(10)`.
        lib: Operand,
    },
}

/// What a `monitor` handler catches.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Selector {
    /// `*ALL`.
    All,
    /// `*PROGRAM`: everything but file errors.
    Program,
    /// `*FILE`: file errors.
    File,
    /// A range of RPG status codes.
    Status(u16, u16),
    /// A message identifier, or a prefix ending in `*`.
    MsgId(String),
}

/// A `dbio` operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DbOp {
    /// `READ`.
    Read,
    /// `READP`.
    ReadP,
    /// `READE`.
    ReadE,
    /// `READPE`.
    ReadPE,
    /// `CHAIN`.
    Chain,
    /// `SETLL`.
    SetLL,
    /// `SETGT`.
    SetGT,
    /// `WRITE`.
    Write,
    /// `UPDATE`.
    Update,
    /// `DELETE`.
    Delete,
    /// `UNLOCK`.
    Unlock,
    /// `FEOD`.
    Feod,
}

impl DbOp {
    /// The name as written.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Read => "READ",
            Self::ReadP => "READP",
            Self::ReadE => "READE",
            Self::ReadPE => "READPE",
            Self::Chain => "CHAIN",
            Self::SetLL => "SETLL",
            Self::SetGT => "SETGT",
            Self::Write => "WRITE",
            Self::Update => "UPDATE",
            Self::Delete => "DELETE",
            Self::Unlock => "UNLOCK",
            Self::Feod => "FEOD",
        }
    }

    /// From the name.
    #[must_use]
    pub fn from_name(s: &str) -> Option<Self> {
        Some(match s {
            "READ" => Self::Read,
            "READP" => Self::ReadP,
            "READE" => Self::ReadE,
            "READPE" => Self::ReadPE,
            "CHAIN" => Self::Chain,
            "SETLL" => Self::SetLL,
            "SETGT" => Self::SetGT,
            "WRITE" => Self::Write,
            "UPDATE" => Self::Update,
            "DELETE" => Self::Delete,
            "UNLOCK" => Self::Unlock,
            "FEOD" => Self::Feod,
            _ => return None,
        })
    }

    /// Whether the operation takes a key or relative record number.
    #[must_use]
    pub fn positions(self) -> bool {
        matches!(
            self,
            Self::Chain | Self::SetLL | Self::SetGT | Self::ReadE | Self::ReadPE
        )
    }

    /// Whether the operation reads a record into the buffer.
    #[must_use]
    pub fn reads(self) -> bool {
        matches!(
            self,
            Self::Read | Self::ReadP | Self::ReadE | Self::ReadPE | Self::Chain
        )
    }

    /// Whether the operation writes the buffer.
    #[must_use]
    pub fn writes(self) -> bool {
        matches!(self, Self::Write | Self::Update)
    }
}

/// A display or printer operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DevOp {
    /// `WRITE` a format.
    Write,
    /// `READ` a format.
    Read,
    /// `EXFMT`: write then read.
    ExFmt,
    /// `CHAIN` a subfile record by relative record number (format 3).
    Chain,
    /// `UPDATE` the subfile record last read (format 3).
    Update,
    /// `READC`: the next changed subfile record (format 3).
    ReadC,
}

impl DevOp {
    /// The name as written.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Write => "WRITE",
            Self::Read => "READ",
            Self::ExFmt => "EXFMT",
            Self::Chain => "CHAIN",
            Self::Update => "UPDATE",
            Self::ReadC => "READC",
        }
    }

    /// From the name.
    #[must_use]
    pub fn from_name(s: &str) -> Option<Self> {
        Some(match s {
            "WRITE" => Self::Write,
            "READ" => Self::Read,
            "EXFMT" => Self::ExFmt,
            "CHAIN" => Self::Chain,
            "UPDATE" => Self::Update,
            "READC" => Self::ReadC,
            _ => return None,
        })
    }
}

/// An I/O instruction. Every operation yields the status code of the
/// `IoFeedback`: 0, or the five-digit RPG status.
#[derive(Clone, Debug, PartialEq)]
pub enum IoInst {
    /// `%d = dbopen FILE [infds(place)]`.
    DbOpen {
        /// Destination, an `i32`.
        dst: Reg,
        /// File index.
        file: u32,
        /// The information data structure the runtime keeps current.
        infds: Option<Place>,
    },
    /// `%d = dbclose FILE`.
    DbClose {
        /// Destination, an `i32`.
        dst: Reg,
        /// File index.
        file: u32,
    },
    /// `%d = dbfb FILE`: the feedback of the file's last operation as
    /// bits: 1 end of file, 2 found, 4 equal, 8 open.
    DbFb {
        /// Destination, an `i32`.
        dst: Reg,
        /// File index.
        file: u32,
    },
    /// `%d = dbio FILE OP [rec(place)] [key(opnd)] [rrn(opnd)] [fmt(NAME)] [nolock]`.
    DbIo {
        /// Destination, an `i32`.
        dst: Reg,
        /// File index.
        file: u32,
        /// Operation.
        op: DbOp,
        /// Record buffer.
        rec: Option<Place>,
        /// Key for keyed positioning.
        key: Option<Operand>,
        /// Relative record number for positioning by number.
        rrn: Option<Operand>,
        /// Record format for a multi-format file.
        fmt: Option<String>,
        /// Read without a lock on an update file.
        nolock: bool,
    },
    /// `%d = dspio|prtio FILE OP fmt(NAME) [rec(place)] [ctl(sb, sa, kb, ka)]
    /// [rrn(x)] [inds(place)]`.
    DevIo {
        /// Destination, an `i32`.
        dst: Reg,
        /// File index.
        file: u32,
        /// Operation.
        op: DevOp,
        /// Record format.
        fmt: String,
        /// Record buffer.
        rec: Option<Place>,
        /// A printer line's carriage control: space before, space
        /// after, skip before, skip after (0 for none); format 2.
        ctl: Option<Box<[Operand; 4]>>,
        /// A subfile record's relative record number (`WRITE`,
        /// `CHAIN`); format 3.
        rrn: Option<Operand>,
        /// The separate indicator area (`INDARA`), 99 indicators;
        /// format 3.
        inds: Option<Place>,
    },
}

/// A message type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MsgType {
    /// `*INFO`.
    Info,
    /// `*COMP`.
    Comp,
    /// `*DIAG`.
    Diag,
    /// `*ESCAPE`.
    Escape,
    /// `*STATUS`.
    Status,
    /// `*NOTIFY`.
    Notify,
    /// `*RQS`.
    Rqs,
    /// `*INQ`.
    Inq,
    /// `*RPY`.
    Rpy,
    /// `*SCOPE`.
    Scope,
    /// `*ANY`, receive only.
    Any,
}

impl MsgType {
    /// The special value.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Info => "*INFO",
            Self::Comp => "*COMP",
            Self::Diag => "*DIAG",
            Self::Escape => "*ESCAPE",
            Self::Status => "*STATUS",
            Self::Notify => "*NOTIFY",
            Self::Rqs => "*RQS",
            Self::Inq => "*INQ",
            Self::Rpy => "*RPY",
            Self::Scope => "*SCOPE",
            Self::Any => "*ANY",
        }
    }

    /// From the special value.
    #[must_use]
    pub fn from_name(s: &str) -> Option<Self> {
        Some(match s {
            "*INFO" => Self::Info,
            "*COMP" => Self::Comp,
            "*DIAG" => Self::Diag,
            "*ESCAPE" => Self::Escape,
            "*STATUS" => Self::Status,
            "*NOTIFY" => Self::Notify,
            "*RQS" => Self::Rqs,
            "*INQ" => Self::Inq,
            "*RPY" => Self::Rpy,
            "*SCOPE" => Self::Scope,
            "*ANY" => Self::Any,
            _ => return None,
        })
    }
}

/// A call-stack entry a message goes to or comes from.
#[derive(Clone, Debug, PartialEq)]
pub enum MsgQueue {
    /// `*SAME`: this entry.
    Same,
    /// `*PRV`: the caller.
    Prv,
    /// `*CTLBDY`: the nearest control boundary.
    CtlBdy,
    /// `*EXT`: the job's external queue.
    Ext,
    /// `*SYSOPR`.
    SysOpr,
    /// A number of entries up, from a register or literal.
    Up(Operand),
}

/// A message instruction.
#[derive(Clone, Debug, PartialEq)]
pub enum MsgInst {
    /// `sndpgmmsg msgid(x) msgf(x [, lib]) [data(x)] type(*X) to(*X) [key(place)]`.
    Snd {
        /// Message identifier, `char(7)`.
        msgid: Operand,
        /// Message file, `char(10)`.
        msgf: Operand,
        /// Message file library, `char(10)`; `*LIBL` when absent.
        msgf_lib: Option<Operand>,
        /// Message data.
        data: Option<Operand>,
        /// Type.
        msg_type: MsgType,
        /// Where it goes.
        to: MsgQueue,
        /// Receives the message key, `char(4)`.
        key: Option<Place>,
    },
    /// `%d = rcvmsg pgmq(*X) [type(*X)] [key(x)] [rmv] [msg(place)] [msgid(place)]
    /// [data(place)] [keyout(place)] [sev(place)]`: 1 when a message was received.
    Rcv {
        /// Destination, an `ind`.
        dst: Reg,
        /// Which queue.
        pgmq: MsgQueue,
        /// Type wanted; `*ANY` when absent.
        msg_type: Option<MsgType>,
        /// Key of the message wanted.
        key: Option<Operand>,
        /// Remove it from the queue.
        rmv: bool,
        /// Receives the first-level text.
        msg: Option<Place>,
        /// Receives the identifier, `char(7)`.
        msgid: Option<Place>,
        /// Receives the message data.
        data: Option<Place>,
        /// Receives the key, `char(4)`.
        keyout: Option<Place>,
        /// Receives the severity, `i32` or a numeric place.
        sev: Option<Place>,
    },
    /// `rmvmsg pgmq(*X) [key(x)] [clear(*ALL|*OLD|*NEW|*KEEPUNANS)]`.
    Rmv {
        /// Which queue.
        pgmq: MsgQueue,
        /// The message to remove; `clear` otherwise.
        key: Option<Operand>,
        /// Which messages when no key is given.
        clear: String,
    },
}

/// A data area operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DtaaraOp {
    /// `lock`: allocate for update.
    Lock,
    /// `read`: into the place.
    Read,
    /// `write`: from the place.
    Write,
    /// `unlock`.
    Unlock,
}

impl DtaaraOp {
    /// The name as written.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Lock => "lock",
            Self::Read => "read",
            Self::Write => "write",
            Self::Unlock => "unlock",
        }
    }

    /// From the name.
    #[must_use]
    pub fn from_name(s: &str) -> Option<Self> {
        Some(match s {
            "lock" => Self::Lock,
            "read" => Self::Read,
            "write" => Self::Write,
            "unlock" => Self::Unlock,
            _ => return None,
        })
    }
}

/// A data queue instruction.
#[derive(Clone, Debug, PartialEq)]
pub enum DtaqInst {
    /// `dtaq.snd name, lib, data, len`.
    Snd {
        /// Queue name, `char(10)`.
        name: Operand,
        /// Library, `char(10)`.
        lib: Operand,
        /// Entry data.
        data: Operand,
        /// Entry length.
        len: Operand,
    },
    /// `%d = dtaq.rcv name, lib, data, len, wait`: the length received,
    /// 0 when none within `wait` seconds (negative waits forever).
    Rcv {
        /// Destination, an `i32`.
        dst: Reg,
        /// Queue name, `char(10)`.
        name: Operand,
        /// Library, `char(10)`.
        lib: Operand,
        /// Receives the entry.
        data: Place,
        /// Capacity of `data`.
        len: Operand,
        /// Seconds to wait.
        wait: Operand,
    },
}

/// A `monitor` handler.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Handler {
    /// What it catches.
    pub selector: Selector,
    /// Where control goes.
    pub target: Label,
}

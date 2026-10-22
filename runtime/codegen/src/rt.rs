// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The runtime call table: the functions generated code calls, by
//! index. The index is what a relocation names; the runtime fills a
//! table of addresses in the same order. Every function takes the
//! environment first and uses the System V convention. Storage
//! operands travel as a native address and a type descriptor
//! ([`crate::abi::TypeDesc`]); an absent optional operand is 0.

use cranelift_codegen::ir::{AbiParam, Signature, types};
use cranelift_codegen::isa::CallConv;

/// A runtime function. The discriminant is its table index.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u32)]
pub enum RtFn {
    /// `raise(env, cond)`: a machine condition occurred at the caller.
    /// Returns when the condition was handled and execution resumes;
    /// otherwise sets the job's pending flag and returns.
    Raise = 0,
    /// `enter(env, code)`: a procedure at code address `code` begins.
    Enter = 1,
    /// `leave(env)`: the procedure ends, normally or by percolation.
    Leave = 2,
    /// `deref(env, lo, hi) -> addr`: the native address a fat pointer
    /// (its two halves) addresses, or a raised condition and 0.
    Deref = 3,
    /// `copy(dst, src, len)`: bytes between non-overlapping storage.
    Copy = 4,
    /// `mov_str(env, dst, dty, src, sty)`: text assignment, padded or
    /// truncated, varying lengths kept.
    MovStr = 5,
    /// `dec_bin(env, op, dst, dty, a, aty, b, bty, prec, flags)`.
    DecBin = 6,
    /// `dec_un(env, op, dst, dty, src, sty, halfadj)`.
    DecUn = 7,
    /// `dec_cmp(env, a, aty, b, bty) -> -1|0|1`.
    DecCmp = 8,
    /// `dec_to_int(env, src, sty, bits, signed, chk) -> i64`.
    DecToInt = 9,
    /// `dec_to_float(env, src, sty) -> f64`.
    DecToFloat = 10,
    /// `convert(env, dst, dty, src, sty, opts)`.
    Convert = 11,
    /// `str_cat(env, dst, dty, a, aty, b, bty)`.
    StrCat = 12,
    /// `str_subst(env, dst, dty, src, sty, start, len)`.
    StrSubst = 13,
    /// `str_scan(env, needle, nty, hay, hty, start) -> i32`.
    StrScan = 14,
    /// `str_check(env, set, sty, src, srcty, start, reverse) -> i32`.
    StrCheck = 15,
    /// `str_xlate(env, dst, dty, from, fty, to, tty, src, sty, start)`.
    StrXlate = 16,
    /// `str_trim(env, dst, dty, src, sty, chars, cty, side)`.
    StrTrim = 17,
    /// `str_cmp(env, a, aty, b, bty) -> -1|0|1`.
    StrCmp = 18,
    /// `str_len(env, src, sty) -> i32`.
    StrLen = 19,
    /// `str_setlen(env, dst, dty, n)`.
    StrSetLen = 20,
    /// `str_replace(env, dst, dty, src, sty, repl, rty, start, len)`.
    StrReplace = 21,
    /// `dt_add(env, dst, dty, src, sty, n, unit)`.
    DtAdd = 22,
    /// `dt_diff(env, a, aty, b, bty, unit) -> i64`.
    DtDiff = 23,
    /// `dt_extract(env, src, sty, unit) -> i32`.
    DtExtract = 24,
    /// `dt_test(env, src, sty, datfmt, timfmt) -> 0|1`.
    DtTest = 25,
    /// `dt_now(env, dst, dty)`.
    DtNow = 26,
    /// `ptr_addr(env, out, native, len)`: a space pointer to storage,
    /// written to the 16 bytes at `out`.
    PtrAddr = 27,
    /// `ptr_add(env, out, ptr, n)`: `n` bytes on, written to `out`.
    PtrAdd = 28,
    /// `ptr_diff(env, p, q) -> i64`.
    PtrDiff = 29,
    /// `proc_addr(env, out, code)`: a procedure pointer, written to `out`.
    ProcAddr = 30,
    /// `dsply(env, src, sty, reply, rty)`.
    Dsply = 31,
    /// `monitor_match(env, kind, a, b) -> 0|1`: whether the pending
    /// exception matches a handler selector; 1 takes it.
    MonitorMatch = 32,
    /// `call_pgm(env, name, lib, argc, argv)`: a dynamic program call by
    /// `char(10)` name and library (`*LIBL` for the list); `argv` is an
    /// array of `argc` parameter addresses.
    CallPgm = 33,
    /// `call_pgm_var(env, addr, len, argc, argv)`: a program whose
    /// `char(10)` or `char(20)` name is in storage.
    CallPgmVar = 34,
    /// `call_pgm_ptr(env, ptr, argc, argv)`: a program by system pointer.
    CallPgmPtr = 35,
    /// `proc_target(env, ptr) -> code`: the code address a procedure
    /// pointer names, validated; 0 and a raised condition otherwise.
    ProcTarget = 36,
    /// `alloc(env, out, n)`: heap storage, a pointer written to `out`.
    Alloc = 37,
    /// `realloc(env, out, ptr, n)`: resized heap storage, to `out`.
    Realloc = 38,
    /// `dealloc(env, ptr)`: heap storage freed.
    Dealloc = 39,
    /// `sndpgmmsg(env, msgid, msgf, lib, data, dty, type, to_kind, to_n, key)`.
    SndPgmMsg = 40,
    /// `rcvmsg(env, pgmq_kind, pgmq_n, type, key, rmv, msg, mty, msgid,
    /// data, dty, keyout, sev, sevty) -> 0|1`.
    RcvMsg = 41,
    /// `rmvmsg(env, pgmq_kind, pgmq_n, key, clear)`.
    RmvMsg = 42,
    /// `db_open(env, file, infds, infds_desc) -> status`: the module's
    /// file by index, opened for the running activation.
    DbOpen = 43,
    /// `db_close(env, file) -> status`.
    DbClose = 44,
    /// `db_io(env, file, op, rec, rec_desc, key, key_desc, rrn, fmt,
    /// nolock) -> status`.
    DbIo = 45,
    /// `dev_io(env, file, op, fmt, rec, rec_desc) -> status`.
    DevIo = 46,
    /// `dtaara(env, op, name, lib, data, data_desc)`.
    Dtaara = 47,
    /// `dtaq_snd(env, name, lib, data, len)`.
    DtaqSnd = 48,
    /// `dtaq_rcv(env, name, lib, data, cap, wait) -> len`.
    DtaqRcv = 49,
    /// `rslvsp(env, out, type, name, lib)`: a system pointer written
    /// to `out`.
    Rslvsp = 50,
    /// `dbg_stmt(env, n)`: statement `n` starts; the debug variant
    /// only.
    DbgStmt = 51,
    /// `dbg_var(env, name, addr, desc)`: a source variable's place,
    /// for `EVAL`; the debug variant only.
    DbgVar = 52,
}

/// The code of a database operation, as `db_io` takes it.
#[must_use]
pub fn db_op_code(op: qir::DbOp) -> i64 {
    match op {
        qir::DbOp::Read => 1,
        qir::DbOp::ReadP => 2,
        qir::DbOp::ReadE => 3,
        qir::DbOp::ReadPE => 4,
        qir::DbOp::Chain => 5,
        qir::DbOp::SetLL => 6,
        qir::DbOp::SetGT => 7,
        qir::DbOp::Write => 8,
        qir::DbOp::Update => 9,
        qir::DbOp::Delete => 10,
        qir::DbOp::Unlock => 11,
        qir::DbOp::Feod => 12,
    }
}

/// The code of a device operation, as `dev_io` takes it.
#[must_use]
pub fn dev_op_code(op: qir::DevOp) -> i64 {
    match op {
        qir::DevOp::Write => 1,
        qir::DevOp::Read => 2,
        qir::DevOp::ExFmt => 3,
    }
}

/// The code of a data area operation, as `dtaara` takes it.
#[must_use]
pub fn dtaara_op_code(op: qir::DtaaraOp) -> i64 {
    match op {
        qir::DtaaraOp::Lock => 1,
        qir::DtaaraOp::Read => 2,
        qir::DtaaraOp::Write => 3,
        qir::DtaaraOp::Unlock => 4,
    }
}

/// Every runtime function, in table order.
pub const RT_FNS: [RtFn; 53] = [
    RtFn::Raise,
    RtFn::Enter,
    RtFn::Leave,
    RtFn::Deref,
    RtFn::Copy,
    RtFn::MovStr,
    RtFn::DecBin,
    RtFn::DecUn,
    RtFn::DecCmp,
    RtFn::DecToInt,
    RtFn::DecToFloat,
    RtFn::Convert,
    RtFn::StrCat,
    RtFn::StrSubst,
    RtFn::StrScan,
    RtFn::StrCheck,
    RtFn::StrXlate,
    RtFn::StrTrim,
    RtFn::StrCmp,
    RtFn::StrLen,
    RtFn::StrSetLen,
    RtFn::StrReplace,
    RtFn::DtAdd,
    RtFn::DtDiff,
    RtFn::DtExtract,
    RtFn::DtTest,
    RtFn::DtNow,
    RtFn::PtrAddr,
    RtFn::PtrAdd,
    RtFn::PtrDiff,
    RtFn::ProcAddr,
    RtFn::Dsply,
    RtFn::MonitorMatch,
    RtFn::CallPgm,
    RtFn::CallPgmVar,
    RtFn::CallPgmPtr,
    RtFn::ProcTarget,
    RtFn::Alloc,
    RtFn::Realloc,
    RtFn::Dealloc,
    RtFn::SndPgmMsg,
    RtFn::RcvMsg,
    RtFn::RmvMsg,
    RtFn::DbOpen,
    RtFn::DbClose,
    RtFn::DbIo,
    RtFn::DevIo,
    RtFn::Dtaara,
    RtFn::DtaqSnd,
    RtFn::DtaqRcv,
    RtFn::Rslvsp,
    RtFn::DbgStmt,
    RtFn::DbgVar,
];

impl RtFn {
    /// The table index.
    #[must_use]
    pub fn index(self) -> u32 {
        self as u32
    }

    /// From a table index.
    #[must_use]
    pub fn from_index(i: u32) -> Option<Self> {
        RT_FNS.get(i as usize).copied()
    }

    /// The name, for listings.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Raise => "raise",
            Self::Enter => "enter",
            Self::Leave => "leave",
            Self::Deref => "deref",
            Self::Copy => "copy",
            Self::MovStr => "mov_str",
            Self::DecBin => "dec_bin",
            Self::DecUn => "dec_un",
            Self::DecCmp => "dec_cmp",
            Self::DecToInt => "dec_to_int",
            Self::DecToFloat => "dec_to_float",
            Self::Convert => "convert",
            Self::StrCat => "str_cat",
            Self::StrSubst => "str_subst",
            Self::StrScan => "str_scan",
            Self::StrCheck => "str_check",
            Self::StrXlate => "str_xlate",
            Self::StrTrim => "str_trim",
            Self::StrCmp => "str_cmp",
            Self::StrLen => "str_len",
            Self::StrSetLen => "str_setlen",
            Self::StrReplace => "str_replace",
            Self::DtAdd => "dt_add",
            Self::DtDiff => "dt_diff",
            Self::DtExtract => "dt_extract",
            Self::DtTest => "dt_test",
            Self::DtNow => "dt_now",
            Self::PtrAddr => "ptr_addr",
            Self::PtrAdd => "ptr_add",
            Self::PtrDiff => "ptr_diff",
            Self::ProcAddr => "proc_addr",
            Self::Dsply => "dsply",
            Self::MonitorMatch => "monitor_match",
            Self::CallPgm => "call_pgm",
            Self::CallPgmVar => "call_pgm_var",
            Self::CallPgmPtr => "call_pgm_ptr",
            Self::ProcTarget => "proc_target",
            Self::Alloc => "alloc",
            Self::Realloc => "realloc",
            Self::Dealloc => "dealloc",
            Self::SndPgmMsg => "sndpgmmsg",
            Self::RcvMsg => "rcvmsg",
            Self::RmvMsg => "rmvmsg",
            Self::DbOpen => "db_open",
            Self::DbClose => "db_close",
            Self::DbIo => "db_io",
            Self::DevIo => "dev_io",
            Self::Dtaara => "dtaara",
            Self::DtaqSnd => "dtaq_snd",
            Self::DtaqRcv => "dtaq_rcv",
            Self::Rslvsp => "rslvsp",
            Self::DbgStmt => "dbg_stmt",
            Self::DbgVar => "dbg_var",
        }
    }

    /// Parameter count after `env` and the return type: `i64` operands
    /// throughout, except where a result is an `i32` or an `f64`.
    #[must_use]
    #[allow(clippy::match_same_arms)]
    pub fn shape(self) -> (usize, Option<types::Type>) {
        match self {
            Self::Raise => (1, None),
            Self::Enter => (2, None),
            Self::Leave => (0, None),
            Self::Deref => (2, Some(types::I64)),
            Self::Copy => (2, None),
            Self::MovStr => (4, None),
            Self::DecBin => (9, None),
            Self::DecUn => (6, None),
            Self::DecCmp => (4, Some(types::I32)),
            Self::DecToInt => (5, Some(types::I64)),
            Self::DecToFloat => (2, Some(types::F64)),
            Self::Convert => (5, None),
            Self::StrCat => (6, None),
            Self::StrSubst => (6, None),
            Self::StrScan => (5, Some(types::I32)),
            Self::StrCheck => (6, Some(types::I32)),
            Self::StrXlate => (9, None),
            Self::StrTrim => (7, None),
            Self::StrCmp => (4, Some(types::I32)),
            Self::StrLen => (2, Some(types::I32)),
            Self::StrSetLen => (3, None),
            Self::StrReplace => (8, None),
            Self::DtAdd => (6, None),
            Self::DtDiff => (5, Some(types::I64)),
            Self::DtExtract => (3, Some(types::I32)),
            Self::DtTest => (4, Some(types::I32)),
            Self::DtNow => (2, None),
            Self::PtrAddr => (3, None),
            Self::PtrAdd => (3, None),
            Self::PtrDiff => (2, Some(types::I64)),
            Self::ProcAddr => (2, None),
            Self::Dsply => (4, None),
            Self::MonitorMatch => (3, Some(types::I32)),
            Self::CallPgm => (4, None),
            Self::CallPgmVar => (4, None),
            Self::CallPgmPtr => (3, None),
            Self::ProcTarget => (1, Some(types::I64)),
            Self::Alloc => (2, None),
            Self::Realloc => (3, None),
            Self::Dealloc => (1, None),
            Self::SndPgmMsg => (9, None),
            Self::RcvMsg => (13, Some(types::I32)),
            Self::RmvMsg => (4, None),
            Self::DbOpen => (3, Some(types::I32)),
            Self::DbClose => (1, Some(types::I32)),
            Self::DbIo => (9, Some(types::I32)),
            Self::DevIo => (5, Some(types::I32)),
            Self::Dtaara => (5, None),
            Self::DtaqSnd => (4, None),
            Self::DtaqRcv => (5, Some(types::I32)),
            Self::Rslvsp => (4, None),
            Self::DbgStmt => (1, None),
            Self::DbgVar => (3, None),
        }
    }

    /// The Cranelift signature. `copy` takes no environment; `raise`
    /// takes its condition as an `i32`.
    #[must_use]
    pub fn signature(self) -> Signature {
        let mut s = Signature::new(CallConv::SystemV);
        let i64p = AbiParam::new(types::I64);
        let (n, ret) = self.shape();
        if self != Self::Copy {
            s.params.push(i64p);
        }
        if self == Self::Raise {
            s.params.push(AbiParam::new(types::I32));
        } else {
            for _ in 0..n {
                s.params.push(i64p);
            }
        }
        if self == Self::Copy {
            s.params.push(i64p);
        }
        if let Some(r) = ret {
            s.returns.push(AbiParam::new(r));
        }
        s
    }
}

/// A machine condition generated code or the runtime raises, as the
/// number of its `MCHnnnn` message.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum Cond {
    /// `MCH0601`: pointer outside its space.
    Mch0601 = 601,
    /// `MCH0602`: pointer stored unaligned.
    Mch0602 = 602,
    /// `MCH0603`: index out of range.
    Mch0603 = 603,
    /// `MCH1202`: decimal data error.
    Mch1202 = 1202,
    /// `MCH1210`: size error, a value does not fit.
    Mch1210 = 1210,
    /// `MCH1211`: division by zero.
    Mch1211 = 1211,
    /// `MCH1602`: a date, time or timestamp that is not valid (the
    /// identifier is an open question, `ILE_OPEN_QUESTIONS.md`).
    Mch1602 = 1602,
    /// `MCH3401`: object not found.
    Mch3401 = 3401,
    /// `MCH3402`: space no longer exists.
    Mch3402 = 3402,
    /// `MCH3601`: null pointer.
    Mch3601 = 3601,
    /// `MCH3602`: pointer of the wrong kind.
    Mch3602 = 3602,
    /// `MCH4431`: a service program's signature does not match.
    Mch4431 = 4431,
    /// `MCH6801`: object domain or state violation (`QSECURITY` 40+).
    Mch6801 = 6801,
}

impl Cond {
    /// The message identifier.
    #[must_use]
    pub fn msgid(self) -> String {
        format!("MCH{:04}", self as u32)
    }

    /// From the number generated code passed.
    #[must_use]
    pub fn from_code(code: u32) -> Option<Self> {
        Some(match code {
            601 => Self::Mch0601,
            602 => Self::Mch0602,
            603 => Self::Mch0603,
            1202 => Self::Mch1202,
            1210 => Self::Mch1210,
            1211 => Self::Mch1211,
            1602 => Self::Mch1602,
            3401 => Self::Mch3401,
            3402 => Self::Mch3402,
            3601 => Self::Mch3601,
            3602 => Self::Mch3602,
            4431 => Self::Mch4431,
            6801 => Self::Mch6801,
            _ => return None,
        })
    }
}

/// Selector kinds `monitor_match` receives.
pub mod selector {
    /// `*ALL`.
    pub const ALL: i64 = 0;
    /// `*PROGRAM`.
    pub const PROGRAM: i64 = 1;
    /// `*FILE`.
    pub const FILE: i64 = 2;
    /// A status range: `a` low, `b` high.
    pub const STATUS: i64 = 3;
    /// A message identifier or prefix: `a` holds up to 8 ASCII bytes,
    /// little-endian, blank padded; a trailing `*` matches a prefix.
    pub const MSGID: i64 = 4;
}

/// Bits of the `flags` word of `dec_bin`.
pub mod dec_flags {
    /// Half-adjust the assignment.
    pub const HALFADJ: i64 = 1;
    /// `*RESDECPOS` was in effect (informational).
    pub const RESDECPOS: i64 = 2;
}

/// Layout of the `opts` word of `convert`.
pub mod cvt_opts {
    /// Bit 0: half-adjust.
    pub const HALFADJ: i64 = 1;
    /// Bits 8-15: date format index plus one, 0 when none.
    pub const DATFMT_SHIFT: u32 = 8;
    /// Bits 16-23: time format index plus one, 0 when none.
    pub const TIMFMT_SHIFT: u32 = 16;
    /// Bits 32-47: CCSID, 0 when none.
    pub const CCSID_SHIFT: u32 = 32;
}

/// Pack a message identifier or prefix for `selector::MSGID`.
#[must_use]
pub fn pack_msgid(id: &str) -> i64 {
    let mut b = [b' '; 8];
    for (i, c) in id.bytes().take(8).enumerate() {
        b[i] = c;
    }
    i64::from_le_bytes(b)
}

/// Unpack a message identifier or prefix packed by [`pack_msgid`].
#[must_use]
pub fn unpack_msgid(v: i64) -> String {
    String::from_utf8_lossy(&v.to_le_bytes())
        .trim_end()
        .to_string()
}

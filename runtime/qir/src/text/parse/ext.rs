// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Parsing the storage instructions.

use super::Parser;
use crate::error::Result;
use crate::inst::{
    Callee, Cond, CvtOpts, DbOp, DecOp, DecUn, DevOp, DtInst, DtUnit, DtaaraOp, DtaqInst, Handler,
    Inst, IoInst, MsgInst, MsgQueue, MsgType, Operand, PtrInst, Selector, StrInst, TrimSide,
};
use crate::module::{FileDev, Linkage, Reg};
use crate::text::lex::Tok;
use crate::types::{DateFmt, Shape, TimeFmt};

impl Parser {
    fn comma(&mut self) -> Result<()> {
        self.expect_sym(",")
    }

    fn opt_operand(&mut self) -> Result<Option<Operand>> {
        if self.eat_sym(",") {
            Ok(Some(self.operand()?))
        } else {
            Ok(None)
        }
    }

    fn special(&mut self) -> Result<String> {
        if let Tok::Special(s) = self.bump() {
            Ok(format!("*{s}"))
        } else {
            self.at -= 1;
            Err(self.unexpected("a *WORD"))
        }
    }

    fn unit(&mut self) -> Result<DtUnit> {
        let s = self.special()?;
        DtUnit::from_name(&s).ok_or_else(|| self.err(format!("{s} is not a date unit")))
    }

    fn datfmt(&mut self) -> Result<DateFmt> {
        self.paren(|p| {
            let s = p.special()?;
            DateFmt::from_word(&s).ok_or_else(|| p.err(format!("{s} is not a date format")))
        })
    }

    fn timfmt(&mut self) -> Result<TimeFmt> {
        self.paren(|p| {
            let s = p.special()?;
            TimeFmt::from_word(&s).ok_or_else(|| p.err(format!("{s} is not a time format")))
        })
    }

    fn cond_suffix(&self, op: &str, suffix: &[String]) -> Result<Cond> {
        suffix
            .get(1)
            .and_then(|s| Cond::from_name(s))
            .ok_or_else(|| self.err(format!("{op} needs a condition: {op}.eq, {op}.lt...")))
    }

    fn file_index(&mut self) -> Result<u32> {
        let name = self.ident()?;
        self.m
            .file_named(&name)
            .map(|(i, _)| i)
            .ok_or_else(|| self.err(format!("{name} is not a declared file")))
    }

    fn msg_queue(&mut self) -> Result<MsgQueue> {
        self.paren(|p| {
            Ok(match p.peek().clone() {
                Tok::Special(s) => {
                    p.bump();
                    match s.as_str() {
                        "SAME" => MsgQueue::Same,
                        "PRV" => MsgQueue::Prv,
                        "CTLBDY" => MsgQueue::CtlBdy,
                        "EXT" => MsgQueue::Ext,
                        "SYSOPR" => MsgQueue::SysOpr,
                        _ => return Err(p.err(format!("*{s} is not a message queue"))),
                    }
                }
                _ => MsgQueue::Up(p.operand()?),
            })
        })
    }

    fn msg_type(&mut self) -> Result<MsgType> {
        self.paren(|p| {
            let s = p.special()?;
            MsgType::from_name(&s).ok_or_else(|| p.err(format!("{s} is not a message type")))
        })
    }

    /// A storage instruction that defines a register, or `None` when the
    /// opcode is not one.
    #[allow(clippy::too_many_lines, clippy::many_single_char_names)]
    pub(crate) fn ext_def(
        &mut self,
        dst: Reg,
        op: &str,
        suffix: &[String],
    ) -> Result<Option<Inst>> {
        let s1 = suffix.first().map(String::as_str);
        Ok(Some(match (op, s1) {
            ("dec", Some("cmp")) => {
                let cond = self.cond_suffix("dec.cmp", suffix)?;
                let a = self.operand()?;
                self.comma()?;
                let b = self.operand()?;
                Inst::DecCmp { dst, cond, a, b }
            }
            ("str", Some("scan")) => {
                let needle = self.operand()?;
                self.comma()?;
                let hay = self.operand()?;
                let start = self.opt_operand()?;
                Inst::Str(StrInst::Scan {
                    dst,
                    needle,
                    hay,
                    start,
                })
            }
            ("str", Some(k @ ("check" | "checkr"))) => {
                let set = self.operand()?;
                self.comma()?;
                let src = self.operand()?;
                let start = self.opt_operand()?;
                Inst::Str(StrInst::Check {
                    dst,
                    set,
                    src,
                    start,
                    reverse: k == "checkr",
                })
            }
            ("str", Some("cmp")) => {
                let cond = self.cond_suffix("str.cmp", suffix)?;
                let a = self.operand()?;
                self.comma()?;
                let b = self.operand()?;
                Inst::Str(StrInst::Cmp { dst, cond, a, b })
            }
            ("str", Some("len")) => Inst::Str(StrInst::Len {
                dst,
                src: self.operand()?,
            }),
            ("dt", Some("diff")) => {
                let a = self.operand()?;
                self.comma()?;
                let b = self.operand()?;
                self.comma()?;
                let unit = self.unit()?;
                Inst::Dt(DtInst::Diff { dst, a, b, unit })
            }
            ("dt", Some("extract")) => {
                let src = self.operand()?;
                self.comma()?;
                let unit = self.unit()?;
                Inst::Dt(DtInst::Extract { dst, src, unit })
            }
            ("dt", Some("test")) => {
                let src = self.operand()?;
                let mut datfmt = None;
                let mut timfmt = None;
                loop {
                    if self.eat_word("datfmt") {
                        datfmt = Some(self.datfmt()?);
                    } else if self.eat_word("timfmt") {
                        timfmt = Some(self.timfmt()?);
                    } else {
                        break;
                    }
                }
                Inst::Dt(DtInst::Test {
                    dst,
                    src,
                    datfmt,
                    timfmt,
                })
            }
            ("addr", None) => Inst::Ptr(PtrInst::Addr {
                dst,
                place: self.place()?,
            }),
            ("ptradd", None) => {
                let ptr = self.operand()?;
                self.comma()?;
                let n = self.operand()?;
                Inst::Ptr(PtrInst::PtrAdd { dst, ptr, n })
            }
            ("ptrdiff", None) => {
                let p = self.operand()?;
                self.comma()?;
                let q = self.operand()?;
                Inst::Ptr(PtrInst::PtrDiff { dst, p, q })
            }
            ("procaddr", None) => {
                let name = self.ident()?;
                let callee = if let Some((r, _)) = self.m.proc_named(&name) {
                    Callee::Local(r)
                } else if let Some((r, pr)) = self.m.proto_named(&name) {
                    if pr.linkage != Linkage::Import {
                        return Err(self.err(format!("{name} is not a bound procedure")));
                    }
                    Callee::Import(r)
                } else {
                    return Err(self.err(format!("{name} is not a procedure")));
                };
                Inst::Ptr(PtrInst::ProcAddr { dst, callee })
            }
            ("rslvsp", None) => {
                let obj_type = self.special()?;
                self.comma()?;
                let name = self.operand()?;
                self.comma()?;
                let lib = self.operand()?;
                Inst::Ptr(PtrInst::Rslvsp {
                    dst,
                    obj_type,
                    name,
                    lib,
                })
            }
            ("dbopen", None) => {
                let file = self.file_index()?;
                let infds = if self.eat_word("infds") {
                    Some(self.paren(Self::place)?)
                } else {
                    None
                };
                Inst::Io(IoInst::DbOpen { dst, file, infds })
            }
            ("dbclose", None) => Inst::Io(IoInst::DbClose {
                dst,
                file: self.file_index()?,
            }),
            ("dbfb", None) => Inst::Io(IoInst::DbFb {
                dst,
                file: self.file_index()?,
            }),
            ("fileref", None) => Inst::Io(IoInst::FileRef {
                dst,
                file: self.file_index()?,
            }),
            ("dbio", None) => {
                let file = self.file_index()?;
                let w = self.ident()?;
                let op = DbOp::from_name(&w)
                    .ok_or_else(|| self.err(format!("{w} is not a dbio operation")))?;
                let mut io = IoInst::DbIo {
                    dst,
                    file,
                    op,
                    rec: None,
                    key: None,
                    rrn: None,
                    fmt: None,
                    nolock: false,
                    nulls: None,
                };
                let IoInst::DbIo {
                    rec,
                    key,
                    rrn,
                    fmt,
                    nolock,
                    nulls,
                    ..
                } = &mut io
                else {
                    unreachable!()
                };
                while let Tok::Ident(w) = self.peek().clone() {
                    self.bump();
                    match w.as_str() {
                        "rec" => *rec = Some(self.paren(Self::place)?),
                        "key" => *key = Some(self.paren(Self::operand)?),
                        "rrn" => *rrn = Some(self.paren(Self::operand)?),
                        "fmt" => *fmt = Some(self.paren(Self::ident)?),
                        "nolock" => *nolock = true,
                        "nulls" => *nulls = Some(self.paren(Self::place)?),
                        _ => return Err(self.err(format!("{w} is not a dbio option"))),
                    }
                }
                Inst::Io(io)
            }
            (dev @ ("dspio" | "prtio"), None) => {
                let file = self.file_index()?;
                let want = if dev == "dspio" {
                    FileDev::Display
                } else {
                    FileDev::Printer
                };
                if self.m.files[file as usize].dev != want {
                    return Err(self.err(format!(
                        "{} is not a {dev} file",
                        self.m.files[file as usize].name
                    )));
                }
                let w = self.ident()?;
                let op = DevOp::from_name(&w)
                    .ok_or_else(|| self.err(format!("{w} is not a {dev} operation")))?;
                self.expect_word("fmt")?;
                let fmt = self.paren(Self::ident)?;
                let rec = if self.eat_word("rec") {
                    Some(self.paren(Self::place)?)
                } else {
                    None
                };
                let ctl = if self.eat_word("ctl") {
                    if want != FileDev::Printer {
                        return Err(self.err("ctl() is for printer files"));
                    }
                    let four = self.paren(|p| {
                        let a = p.operand()?;
                        p.expect_sym(",")?;
                        let b = p.operand()?;
                        p.expect_sym(",")?;
                        let c = p.operand()?;
                        p.expect_sym(",")?;
                        let d = p.operand()?;
                        Ok([a, b, c, d])
                    })?;
                    Some(Box::new(four))
                } else {
                    None
                };
                let rrn = if self.eat_word("rrn") {
                    Some(self.paren(Self::operand)?)
                } else {
                    None
                };
                let inds = if self.eat_word("inds") {
                    Some(self.paren(Self::place)?)
                } else {
                    None
                };
                Inst::Io(IoInst::DevIo {
                    dst,
                    file,
                    op,
                    fmt,
                    rec,
                    ctl,
                    rrn,
                    inds,
                })
            }
            ("rcvmsg", None) => {
                let mut msg = MsgInst::Rcv {
                    dst,
                    pgmq: MsgQueue::Same,
                    msg_type: None,
                    key: None,
                    rmv: false,
                    msg: None,
                    msgid: None,
                    data: None,
                    keyout: None,
                    sev: None,
                };
                let MsgInst::Rcv {
                    pgmq,
                    msg_type,
                    key,
                    rmv,
                    msg: text,
                    msgid,
                    data,
                    keyout,
                    sev,
                    ..
                } = &mut msg
                else {
                    unreachable!()
                };
                let mut have_q = false;
                while let Tok::Ident(w) = self.peek().clone() {
                    self.bump();
                    match w.as_str() {
                        "pgmq" => {
                            *pgmq = self.msg_queue()?;
                            have_q = true;
                        }
                        "type" => *msg_type = Some(self.msg_type()?),
                        "key" => *key = Some(self.paren(Self::operand)?),
                        "rmv" => *rmv = true,
                        "msg" => *text = Some(self.paren(Self::place)?),
                        "msgid" => *msgid = Some(self.paren(Self::place)?),
                        "data" => *data = Some(self.paren(Self::place)?),
                        "keyout" => *keyout = Some(self.paren(Self::place)?),
                        "sev" => *sev = Some(self.paren(Self::place)?),
                        _ => return Err(self.err(format!("{w} is not a rcvmsg option"))),
                    }
                }
                if !have_q {
                    return Err(self.err("rcvmsg needs pgmq()"));
                }
                Inst::Msg(msg)
            }
            ("alloc", None) => Inst::Alloc {
                dst,
                n: self.operand()?,
            },
            ("realloc", None) => {
                let ptr = self.operand()?;
                self.comma()?;
                let n = self.operand()?;
                Inst::Realloc { dst, ptr, n }
            }
            ("dtaq", Some("rcv")) => {
                let name = self.operand()?;
                self.comma()?;
                let lib = self.operand()?;
                self.comma()?;
                let data = self.place()?;
                self.comma()?;
                let len = self.operand()?;
                self.comma()?;
                let wait = self.operand()?;
                Inst::Dtaq(DtaqInst::Rcv {
                    dst,
                    name,
                    lib,
                    data,
                    len,
                    wait,
                })
            }
            _ => return Ok(None),
        }))
    }

    /// A storage instruction without a defined register, or `None`.
    #[allow(clippy::too_many_lines, clippy::many_single_char_names)]
    pub(crate) fn ext_plain(&mut self, op: &str, suffix: &[String]) -> Result<Option<Inst>> {
        let s1 = suffix.first().map(String::as_str);
        Ok(Some(match (op, s1) {
            ("dec", Some(k @ ("add" | "sub" | "mul" | "div" | "rem"))) => {
                let op = DecOp::from_name(k).expect("matched");
                let dst = self.place()?;
                self.comma()?;
                let a = self.operand()?;
                self.comma()?;
                let b = self.operand()?;
                self.expect_word("prec")?;
                let prec = self.paren(|p| {
                    let d = p.u32()?;
                    p.expect_sym(",")?;
                    let s = p.u32()?;
                    let d = u8::try_from(d).map_err(|_| p.err("digits are 1 to 63"))?;
                    let s = u8::try_from(s).map_err(|_| p.err("decimal positions are 0 to 63"))?;
                    Shape::new(d, s).map_err(|e| p.err(e.to_string()))
                })?;
                let mut halfadj = false;
                let mut resdecpos = false;
                loop {
                    if self.eat_word("halfadj") {
                        halfadj = true;
                    } else if self.eat_word("resdecpos") {
                        resdecpos = true;
                    } else {
                        break;
                    }
                }
                Inst::DecBin {
                    dst,
                    op,
                    a,
                    b,
                    prec,
                    halfadj,
                    resdecpos,
                }
            }
            ("dec", Some(k @ ("mov" | "neg" | "abs"))) => {
                let op = match k {
                    "mov" => DecUn::Mov,
                    "neg" => DecUn::Neg,
                    _ => DecUn::Abs,
                };
                let dst = self.place()?;
                self.comma()?;
                let src = self.operand()?;
                let mut halfadj = false;
                let mut trunc = false;
                loop {
                    if self.eat_word("halfadj") {
                        halfadj = true;
                    } else if self.eat_word("trunc") {
                        trunc = true;
                    } else {
                        break;
                    }
                }
                Inst::DecUn {
                    dst,
                    op,
                    src,
                    halfadj,
                    trunc,
                }
            }
            ("cvt", None) => {
                let dst = self.place()?;
                self.comma()?;
                let src = self.operand()?;
                let mut opts = CvtOpts::default();
                loop {
                    if self.eat_word("halfadj") {
                        opts.halfadj = true;
                    } else if self.eat_word("datfmt") {
                        opts.datfmt = Some(self.datfmt()?);
                    } else if self.eat_word("timfmt") {
                        opts.timfmt = Some(self.timfmt()?);
                    } else if self.eat_word("ccsid") {
                        let n = self.paren(Self::u32)?;
                        opts.ccsid =
                            Some(u16::try_from(n).map_err(|_| self.err("ccsid is 1 to 65535"))?);
                    } else {
                        break;
                    }
                }
                Inst::Convert { dst, src, opts }
            }
            ("str", Some("cat")) => {
                let dst = self.place()?;
                self.comma()?;
                let a = self.operand()?;
                self.comma()?;
                let b = self.operand()?;
                Inst::Str(StrInst::Cat { dst, a, b })
            }
            ("str", Some("subst")) => {
                let dst = self.place()?;
                self.comma()?;
                let src = self.operand()?;
                self.comma()?;
                let start = self.operand()?;
                self.comma()?;
                let len = self.operand()?;
                Inst::Str(StrInst::Subst {
                    dst,
                    src,
                    start,
                    len,
                })
            }
            ("str", Some("xlate")) => {
                let dst = self.place()?;
                self.comma()?;
                let from = self.operand()?;
                self.comma()?;
                let to = self.operand()?;
                self.comma()?;
                let src = self.operand()?;
                let start = self.opt_operand()?;
                Inst::Str(StrInst::Xlate {
                    dst,
                    from,
                    to,
                    src,
                    start,
                })
            }
            ("str", Some(k @ ("trim" | "triml" | "trimr"))) => {
                let side = match k {
                    "trim" => TrimSide::Both,
                    "triml" => TrimSide::Left,
                    _ => TrimSide::Right,
                };
                let dst = self.place()?;
                self.comma()?;
                let src = self.operand()?;
                let chars = self.opt_operand()?;
                Inst::Str(StrInst::Trim {
                    dst,
                    src,
                    chars,
                    side,
                })
            }
            ("str", Some("setlen")) => {
                let dst = self.place()?;
                self.comma()?;
                let n = self.operand()?;
                Inst::Str(StrInst::SetLen { dst, n })
            }
            ("str", Some("replace")) => {
                let dst = self.place()?;
                self.comma()?;
                let src = self.operand()?;
                self.comma()?;
                let repl = self.operand()?;
                self.comma()?;
                let start = self.operand()?;
                let len = self.opt_operand()?;
                Inst::Str(StrInst::Replace {
                    dst,
                    src,
                    repl,
                    start,
                    len,
                })
            }
            ("dt", Some("add")) => {
                let dst = self.place()?;
                self.comma()?;
                let src = self.operand()?;
                self.comma()?;
                let n = self.operand()?;
                self.comma()?;
                let unit = self.unit()?;
                Inst::Dt(DtInst::Add { dst, src, n, unit })
            }
            ("dt", Some("now")) => Inst::Dt(DtInst::Now { dst: self.place()? }),
            ("monitor", None) => {
                let name = self.ident()?;
                let mut handlers = Vec::new();
                while self.eat_sym(",") {
                    let selector = self.selector()?;
                    self.expect_sym("->")?;
                    let target = self.label_operand()?;
                    handlers.push(Handler { selector, target });
                }
                Inst::Monitor { name, handlers }
            }
            ("endmon", None) => Inst::EndMon(self.ident()?),
            ("sndpgmmsg", None) => self.sndpgmmsg()?,
            ("rmvmsg", None) => {
                let mut pgmq = None;
                let mut key = None;
                let mut clear = "*ALL".to_string();
                while let Tok::Ident(w) = self.peek().clone() {
                    self.bump();
                    match w.as_str() {
                        "pgmq" => pgmq = Some(self.msg_queue()?),
                        "key" => key = Some(self.paren(Self::operand)?),
                        "clear" => clear = self.paren(Self::special)?,
                        _ => return Err(self.err(format!("{w} is not a rmvmsg option"))),
                    }
                }
                let pgmq = pgmq.ok_or_else(|| self.err("rmvmsg needs pgmq()"))?;
                Inst::Msg(MsgInst::Rmv { pgmq, key, clear })
            }
            ("dsply", None) => {
                let src = self.operand()?;
                let reply = if self.eat_word("reply") {
                    Some(self.paren(Self::place)?)
                } else {
                    None
                };
                Inst::Dsply { src, reply }
            }
            ("dtaara", Some(k)) => {
                let op = DtaaraOp::from_name(k)
                    .ok_or_else(|| self.err(format!("dtaara.{k} is not an operation")))?;
                let name = self.operand()?;
                self.comma()?;
                let lib = self.operand()?;
                let data = if self.eat_sym(",") {
                    Some(self.place()?)
                } else {
                    None
                };
                Inst::Dtaara {
                    op,
                    name,
                    lib,
                    data,
                }
            }
            ("dealloc", None) => Inst::Dealloc {
                ptr: self.operand()?,
            },
            ("filebind", None) => {
                let file = self.file_index()?;
                self.comma()?;
                Inst::Io(IoInst::FileBind {
                    file,
                    handle: self.operand()?,
                })
            }
            ("dtaq", Some("snd")) => {
                let name = self.operand()?;
                self.comma()?;
                let lib = self.operand()?;
                self.comma()?;
                let data = self.operand()?;
                self.comma()?;
                let len = self.operand()?;
                Inst::Dtaq(DtaqInst::Snd {
                    name,
                    lib,
                    data,
                    len,
                })
            }
            _ => return Ok(None),
        }))
    }

    fn selector(&mut self) -> Result<Selector> {
        Ok(match self.bump() {
            Tok::Special(s) => match s.as_str() {
                "ALL" => Selector::All,
                "PROGRAM" => Selector::Program,
                "FILE" => Selector::File,
                _ => {
                    self.at -= 1;
                    return Err(self.err(format!("*{s} is not a monitor selector")));
                }
            },
            Tok::Int(a) => {
                let lo = u16::try_from(a).map_err(|_| self.err("status codes are 0 to 99999"))?;
                let hi = if self.eat_sym("..") {
                    let b = self.int()?;
                    u16::try_from(b).map_err(|_| self.err("status codes are 0 to 99999"))?
                } else {
                    lo
                };
                if hi < lo {
                    return Err(self.err("status range is reversed"));
                }
                Selector::Status(lo, hi)
            }
            Tok::Ident(id) => Selector::MsgId(id),
            Tok::Str(s) if s.ends_with('*') && s.len() > 1 => Selector::MsgId(s),
            _ => {
                self.at -= 1;
                return Err(
                    self.unexpected("*ALL, *PROGRAM, *FILE, a status range or a message id")
                );
            }
        })
    }

    fn sndpgmmsg(&mut self) -> Result<Inst> {
        let mut msgid = None;
        let mut msgf = None;
        let mut msgf_lib = None;
        let mut data = None;
        let mut msg_type = None;
        let mut to = MsgQueue::Prv;
        let mut key = None;
        while let Tok::Ident(w) = self.peek().clone() {
            self.bump();
            match w.as_str() {
                "msgid" => msgid = Some(self.paren(Self::operand)?),
                "msgf" => {
                    self.expect_sym("(")?;
                    msgf = Some(self.operand()?);
                    if self.eat_sym(",") {
                        msgf_lib = Some(self.operand()?);
                    }
                    self.expect_sym(")")?;
                }
                "data" => data = Some(self.paren(Self::operand)?),
                "type" => msg_type = Some(self.msg_type()?),
                "to" => to = self.msg_queue()?,
                "key" => key = Some(self.paren(Self::place)?),
                _ => return Err(self.err(format!("{w} is not a sndpgmmsg option"))),
            }
        }
        let msgid = msgid.ok_or_else(|| self.err("sndpgmmsg needs msgid()"))?;
        let msgf = msgf.ok_or_else(|| self.err("sndpgmmsg needs msgf()"))?;
        let msg_type = msg_type.ok_or_else(|| self.err("sndpgmmsg needs type()"))?;
        if msg_type == MsgType::Any {
            return Err(self.err("*ANY is not a type to send"));
        }
        Ok(Inst::Msg(MsgInst::Snd {
            msgid,
            msgf,
            msgf_lib,
            data,
            msg_type,
            to,
            key,
        }))
    }
}

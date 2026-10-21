// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Printing the storage instructions.

use std::fmt::Write as _;

use super::lex::hex;
use super::print::Printer;
use crate::inst::{
    Callee, Const, DtInst, DtaqInst, Inst, IoInst, MsgInst, MsgQueue, Operand, Place, PtrInst,
    Selector, StrInst, TrimSide,
};
use crate::module::Proc;

impl Printer<'_> {
    /// One storage instruction.
    #[allow(clippy::too_many_lines, clippy::many_single_char_names)]
    pub(super) fn ext_inst(&self, p: &Proc, i: &Inst) -> String {
        let o = |x: &Operand| self.operand(p, x);
        let pl = |x: &Place| self.place(p, x);
        let r = |x| Self::reg(p, x);
        let opt = |v: &Option<Operand>| {
            v.as_ref()
                .map_or_else(String::new, |v| format!(", {}", self.operand(p, v)))
        };
        match i {
            Inst::DecBin {
                dst,
                op,
                a,
                b,
                prec,
                halfadj,
                resdecpos,
            } => format!(
                "dec.{} {}, {}, {} prec({},{}){}{}",
                op.name(),
                pl(dst),
                o(a),
                o(b),
                prec.digits,
                prec.scale,
                if *halfadj { " halfadj" } else { "" },
                if *resdecpos { " resdecpos" } else { "" }
            ),
            Inst::DecUn {
                dst,
                op,
                src,
                halfadj,
            } => format!(
                "dec.{} {}, {}{}",
                op.name(),
                pl(dst),
                o(src),
                if *halfadj { " halfadj" } else { "" }
            ),
            Inst::DecCmp { dst, cond, a, b } => {
                format!("{} = dec.cmp.{} {}, {}", r(*dst), cond.name(), o(a), o(b))
            }
            Inst::Convert { dst, src, opts } => {
                let mut s = format!("cvt {}, {}", pl(dst), o(src));
                if opts.halfadj {
                    s.push_str(" halfadj");
                }
                if let Some(d) = opts.datfmt {
                    let _ = write!(s, " datfmt({})", d.name());
                }
                if let Some(t) = opts.timfmt {
                    let _ = write!(s, " timfmt({})", t.name());
                }
                if let Some(c) = opts.ccsid {
                    let _ = write!(s, " ccsid({c})");
                }
                s
            }
            Inst::Str(s) => match s {
                StrInst::Cat { dst, a, b } => format!("str.cat {}, {}, {}", pl(dst), o(a), o(b)),
                StrInst::Subst {
                    dst,
                    src,
                    start,
                    len,
                } => {
                    format!(
                        "str.subst {}, {}, {}, {}",
                        pl(dst),
                        o(src),
                        o(start),
                        o(len)
                    )
                }
                StrInst::Scan {
                    dst,
                    needle,
                    hay,
                    start,
                } => format!(
                    "{} = str.scan {}, {}{}",
                    r(*dst),
                    o(needle),
                    o(hay),
                    opt(start)
                ),
                StrInst::Check {
                    dst,
                    set,
                    src,
                    start,
                    reverse,
                } => format!(
                    "{} = str.check{} {}, {}{}",
                    r(*dst),
                    if *reverse { "r" } else { "" },
                    o(set),
                    o(src),
                    opt(start)
                ),
                StrInst::Xlate {
                    dst,
                    from,
                    to,
                    src,
                    start,
                } => format!(
                    "str.xlate {}, {}, {}, {}{}",
                    pl(dst),
                    o(from),
                    o(to),
                    o(src),
                    opt(start)
                ),
                StrInst::Trim {
                    dst,
                    src,
                    chars,
                    side,
                } => format!(
                    "str.trim{} {}, {}{}",
                    match side {
                        TrimSide::Both => "",
                        TrimSide::Left => "l",
                        TrimSide::Right => "r",
                    },
                    pl(dst),
                    o(src),
                    opt(chars)
                ),
                StrInst::Cmp { dst, cond, a, b } => {
                    format!("{} = str.cmp.{} {}, {}", r(*dst), cond.name(), o(a), o(b))
                }
                StrInst::Len { dst, src } => format!("{} = str.len {}", r(*dst), o(src)),
                StrInst::SetLen { dst, n } => format!("str.setlen {}, {}", pl(dst), o(n)),
                StrInst::Replace {
                    dst,
                    src,
                    repl,
                    start,
                    len,
                } => format!(
                    "str.replace {}, {}, {}, {}{}",
                    pl(dst),
                    o(src),
                    o(repl),
                    o(start),
                    opt(len)
                ),
            },
            Inst::Dt(d) => match d {
                DtInst::Add { dst, src, n, unit } => {
                    format!("dt.add {}, {}, {}, {}", pl(dst), o(src), o(n), unit.name())
                }
                DtInst::Diff { dst, a, b, unit } => {
                    format!("{} = dt.diff {}, {}, {}", r(*dst), o(a), o(b), unit.name())
                }
                DtInst::Extract { dst, src, unit } => {
                    format!("{} = dt.extract {}, {}", r(*dst), o(src), unit.name())
                }
                DtInst::Test {
                    dst,
                    src,
                    datfmt,
                    timfmt,
                } => {
                    let mut s = format!("{} = dt.test {}", r(*dst), o(src));
                    if let Some(d) = datfmt {
                        let _ = write!(s, " datfmt({})", d.name());
                    }
                    if let Some(t) = timfmt {
                        let _ = write!(s, " timfmt({})", t.name());
                    }
                    s
                }
                DtInst::Now { dst } => format!("dt.now {}", pl(dst)),
            },
            Inst::Ptr(pt) => match pt {
                PtrInst::Addr { dst, place } => format!("{} = addr {}", r(*dst), pl(place)),
                PtrInst::PtrAdd { dst, ptr, n } => {
                    format!("{} = ptradd {}, {}", r(*dst), o(ptr), o(n))
                }
                PtrInst::PtrDiff { dst, p: a, q } => {
                    format!("{} = ptrdiff {}, {}", r(*dst), o(a), o(q))
                }
                PtrInst::ProcAddr { dst, callee } => {
                    let name = match callee {
                        Callee::Local(x) => self
                            .m
                            .procs
                            .get(x.0 as usize)
                            .map_or("?", |y| y.name.as_str()),
                        Callee::Import(x) => self
                            .m
                            .protos
                            .get(x.0 as usize)
                            .map_or("?", |y| y.name.as_str()),
                    };
                    format!("{} = procaddr {name}", r(*dst))
                }
                PtrInst::Rslvsp {
                    dst,
                    obj_type,
                    name,
                    lib,
                } => {
                    format!("{} = rslvsp {obj_type}, {}, {}", r(*dst), o(name), o(lib))
                }
            },
            Inst::Monitor { name, handlers } => {
                let mut s = format!("monitor {name}");
                for h in handlers {
                    let sel = match &h.selector {
                        Selector::All => "*ALL".to_string(),
                        Selector::Program => "*PROGRAM".to_string(),
                        Selector::File => "*FILE".to_string(),
                        Selector::Status(a, b) if a == b => a.to_string(),
                        Selector::Status(a, b) => format!("{a}..{b}"),
                        Selector::MsgId(id) if id.ends_with('*') => format!("\"{id}\""),
                        Selector::MsgId(id) => id.clone(),
                    };
                    let _ = write!(s, ", {sel} -> {}", p.label_name(h.target));
                }
                s
            }
            Inst::EndMon(name) => format!("endmon {name}"),
            Inst::Io(io) => {
                let file = |ix: u32| {
                    self.m
                        .files
                        .get(ix as usize)
                        .map_or("?", |f| f.name.as_str())
                };
                match io {
                    IoInst::DbOpen {
                        dst,
                        file: f,
                        infds,
                    } => {
                        let mut s = format!("{} = dbopen {}", r(*dst), file(*f));
                        if let Some(ds) = infds {
                            let _ = write!(s, " infds({})", pl(ds));
                        }
                        s
                    }
                    IoInst::DbClose { dst, file: f } => {
                        format!("{} = dbclose {}", r(*dst), file(*f))
                    }
                    IoInst::DbIo {
                        dst,
                        file: f,
                        op,
                        rec,
                        key,
                        rrn,
                        fmt,
                        nolock,
                    } => {
                        let mut s = format!("{} = dbio {} {}", r(*dst), file(*f), op.name());
                        if let Some(x) = rec {
                            let _ = write!(s, " rec({})", pl(x));
                        }
                        if let Some(x) = key {
                            let _ = write!(s, " key({})", o(x));
                        }
                        if let Some(x) = rrn {
                            let _ = write!(s, " rrn({})", o(x));
                        }
                        if let Some(x) = fmt {
                            let _ = write!(s, " fmt({x})");
                        }
                        if *nolock {
                            s.push_str(" nolock");
                        }
                        s
                    }
                    IoInst::DevIo {
                        dst,
                        file: f,
                        op,
                        fmt,
                        rec,
                    } => {
                        let dev = match self.m.files.get(*f as usize).map(|x| x.dev) {
                            Some(crate::module::FileDev::Printer) => "prtio",
                            _ => "dspio",
                        };
                        let mut s =
                            format!("{} = {dev} {} {} fmt({fmt})", r(*dst), file(*f), op.name());
                        if let Some(x) = rec {
                            let _ = write!(s, " rec({})", pl(x));
                        }
                        s
                    }
                }
            }
            Inst::Msg(msg) => {
                let q = |q: &MsgQueue| match q {
                    MsgQueue::Same => "*SAME".to_string(),
                    MsgQueue::Prv => "*PRV".to_string(),
                    MsgQueue::CtlBdy => "*CTLBDY".to_string(),
                    MsgQueue::Ext => "*EXT".to_string(),
                    MsgQueue::SysOpr => "*SYSOPR".to_string(),
                    MsgQueue::Up(n) => self.operand(p, n),
                };
                let key_opt = |k: &Operand| match k {
                    Operand::Const(Const::Hex(h)) => format!("x\"{}\"", hex(h)),
                    other => self.operand(p, other),
                };
                match msg {
                    MsgInst::Snd {
                        msgid,
                        msgf,
                        msgf_lib,
                        data,
                        msg_type,
                        to,
                        key,
                    } => {
                        let mut s = format!("sndpgmmsg msgid({}) msgf({}", o(msgid), o(msgf));
                        if let Some(l) = msgf_lib {
                            let _ = write!(s, ", {}", o(l));
                        }
                        s.push(')');
                        if let Some(d) = data {
                            let _ = write!(s, " data({})", o(d));
                        }
                        let _ = write!(s, " type({}) to({})", msg_type.name(), q(to));
                        if let Some(k) = key {
                            let _ = write!(s, " key({})", pl(k));
                        }
                        s
                    }
                    MsgInst::Rcv {
                        dst,
                        pgmq,
                        msg_type,
                        key,
                        rmv,
                        msg: text,
                        msgid,
                        data,
                        keyout,
                        sev,
                    } => {
                        let mut s = format!("{} = rcvmsg pgmq({})", r(*dst), q(pgmq));
                        if let Some(t) = msg_type {
                            let _ = write!(s, " type({})", t.name());
                        }
                        if let Some(k) = key {
                            let _ = write!(s, " key({})", key_opt(k));
                        }
                        if *rmv {
                            s.push_str(" rmv");
                        }
                        for (name, v) in [
                            ("msg", text),
                            ("msgid", msgid),
                            ("data", data),
                            ("keyout", keyout),
                            ("sev", sev),
                        ] {
                            if let Some(x) = v {
                                let _ = write!(s, " {name}({})", pl(x));
                            }
                        }
                        s
                    }
                    MsgInst::Rmv { pgmq, key, clear } => {
                        let mut s = format!("rmvmsg pgmq({})", q(pgmq));
                        match key {
                            Some(k) => {
                                let _ = write!(s, " key({})", key_opt(k));
                            }
                            None => {
                                let _ = write!(s, " clear({clear})");
                            }
                        }
                        s
                    }
                }
            }
            Inst::Dsply { src, reply } => {
                let mut s = format!("dsply {}", o(src));
                if let Some(x) = reply {
                    let _ = write!(s, " reply({})", pl(x));
                }
                s
            }
            Inst::Dtaara {
                op,
                name,
                lib,
                data,
            } => {
                let mut s = format!("dtaara.{} {}, {}", op.name(), o(name), o(lib));
                if let Some(d) = data {
                    let _ = write!(s, ", {}", pl(d));
                }
                s
            }
            Inst::Alloc { dst, n } => format!("{} = alloc {}", r(*dst), o(n)),
            Inst::Realloc { dst, ptr, n } => {
                format!("{} = realloc {}, {}", r(*dst), o(ptr), o(n))
            }
            Inst::Dealloc { ptr } => format!("dealloc {}", o(ptr)),
            Inst::Dtaq(DtaqInst::Snd {
                name,
                lib,
                data,
                len,
            }) => {
                format!("dtaq.snd {}, {}, {}, {}", o(name), o(lib), o(data), o(len))
            }
            Inst::Dtaq(DtaqInst::Rcv {
                dst,
                name,
                lib,
                data,
                len,
                wait,
            }) => format!(
                "{} = dtaq.rcv {}, {}, {}, {}, {}",
                r(*dst),
                o(name),
                o(lib),
                pl(data),
                o(len),
                o(wait)
            ),
            _ => "?".into(),
        }
    }
}

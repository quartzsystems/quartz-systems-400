// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Checks for the storage instructions.

use super::place::place_type;
use super::typing::{operand_type, reg_type};
use crate::inst::{
    Callee, Const, CvtOpts, DecUn, DevOp, DtInst, DtUnit, DtaaraOp, DtaqInst, Inst, IoInst,
    MsgInst, MsgQueue, Operand, PtrInst, StrInst,
};
use crate::module::{FileDev, Linkage, Module, Proc};
use crate::types::{Shape, Type, TypeClass, TypeClass as C};

/// Check one storage instruction.
#[allow(clippy::too_many_lines)]
pub fn check(m: &Module, p: &Proc, i: &Inst) -> Result<(), String> {
    let cx = Cx { m, p };
    match i {
        Inst::DecBin {
            dst,
            a,
            b,
            prec,
            resdecpos,
            ..
        } => {
            let t = cx.numeric_place(dst)?;
            cx.numeric(a)?;
            cx.numeric(b)?;
            Shape::new(prec.digits, prec.scale).map_err(|e| e.to_string())?;
            if *resdecpos && t.shape().is_none() && !t.is_integer() {
                return Err("resdecpos needs a decimal or integer destination".into());
            }
            Ok(())
        }
        Inst::DecUn { dst, op, src, .. } => {
            let t = cx.numeric_place(dst)?;
            cx.numeric(src)?;
            if matches!(op, DecUn::Abs | DecUn::Neg) && t.class() == TypeClass::UInt {
                return Err(format!("dec.{} into an unsigned {t}", op.name()));
            }
            Ok(())
        }
        Inst::DecCmp { a, b, .. } => {
            cx.numeric(a)?;
            cx.numeric(b)?;
            Cx::not_both_literals(a, b)
        }
        Inst::Convert { dst, src, opts } => convert(&cx, dst, src, opts),
        Inst::Str(s) => string(&cx, s),
        Inst::Dt(d) => datetime(&cx, d),
        Inst::Ptr(pt) => pointer(&cx, pt),
        Inst::Monitor { .. } | Inst::EndMon(_) => Ok(()),
        Inst::Io(io) => io_inst(&cx, io),
        Inst::Msg(msg) => message(&cx, msg),
        Inst::Dsply { src, reply } => {
            match src {
                Operand::Reg(r) => {
                    reg_type(p, *r)?;
                }
                Operand::Place(pl) => {
                    place_type(m, p, pl)?;
                }
                Operand::Const(Const::Str(_) | Const::Int(_) | Const::Dec(_)) => {}
                Operand::Const(c) => {
                    return Err(format!("dsply of {}", super::typing::describe_const(c)));
                }
            }
            if let Some(r) = reply {
                cx.string_place(r)?;
            }
            Ok(())
        }
        Inst::Dtaara {
            op,
            name,
            lib,
            data,
        } => {
            cx.name_operand(name)?;
            cx.name_operand(lib)?;
            match (op, data) {
                (DtaaraOp::Read | DtaaraOp::Write, Some(d)) => {
                    place_type(m, p, d)?;
                    Ok(())
                }
                (DtaaraOp::Read | DtaaraOp::Write, None) => {
                    Err(format!("dtaara.{} needs a place", op.name()))
                }
                (DtaaraOp::Lock | DtaaraOp::Unlock, Some(_)) => {
                    Err(format!("dtaara.{} takes no place", op.name()))
                }
                (DtaaraOp::Lock | DtaaraOp::Unlock, None) => Ok(()),
            }
        }
        Inst::Alloc { n, .. } => cx.int(n),
        Inst::Realloc { ptr, n, .. } => {
            cx.ptr(ptr)?;
            cx.int(n)
        }
        Inst::Dealloc { ptr } => cx.ptr(ptr),
        Inst::Dtaq(DtaqInst::Snd {
            name,
            lib,
            data,
            len,
        }) => {
            cx.name_operand(name)?;
            cx.name_operand(lib)?;
            cx.bytes_operand(data)?;
            cx.int(len)
        }
        Inst::Dtaq(DtaqInst::Rcv {
            name,
            lib,
            data,
            len,
            wait,
            ..
        }) => {
            cx.name_operand(name)?;
            cx.name_operand(lib)?;
            place_type(m, p, data)?;
            cx.int(len)?;
            cx.int(wait)
        }
        _ => Err("not a storage instruction".into()),
    }
}

struct Cx<'a> {
    m: &'a Module,
    p: &'a Proc,
}

impl Cx<'_> {
    fn ty(&self, o: &Operand, hint: Option<&Type>) -> Result<Type, String> {
        operand_type(self.m, self.p, o, hint)
    }

    /// A numeric operand: a numeric place, an integer or float
    /// register, or a numeric literal.
    fn numeric(&self, o: &Operand) -> Result<(), String> {
        match o {
            Operand::Const(Const::Int(_) | Const::Dec(_) | Const::Float(_)) => Ok(()),
            Operand::Const(c) => Err(format!(
                "{} is not a number",
                super::typing::describe_const(c)
            )),
            _ => {
                let t = self.ty(o, None)?;
                if t.is_numeric() {
                    Ok(())
                } else {
                    Err(format!("a {t} is not numeric"))
                }
            }
        }
    }

    fn numeric_place(&self, pl: &crate::inst::Place) -> Result<Type, String> {
        let t = place_type(self.m, self.p, pl)?;
        if t.is_numeric() {
            Ok(t)
        } else {
            Err(format!("destination is a {t}, not numeric"))
        }
    }

    fn not_both_literals(a: &Operand, b: &Operand) -> Result<(), String> {
        if matches!((a, b), (Operand::Const(_), Operand::Const(_))) {
            Err("two literals compared".into())
        } else {
            Ok(())
        }
    }

    /// A space pointer operand: a `ptr` register or place.
    fn ptr(&self, o: &Operand) -> Result<(), String> {
        let t = self.ty(o, Some(&Type::Ptr))?;
        if t == Type::Ptr {
            Ok(())
        } else {
            Err(format!("a {t} where a ptr is needed"))
        }
    }

    /// An integer operand: a register, an integer place or a literal.
    fn int(&self, o: &Operand) -> Result<(), String> {
        let t = self.ty(o, Some(&Type::I32))?;
        if t.is_integer() {
            Ok(())
        } else {
            Err(format!("a {t} where an integer is needed"))
        }
    }

    /// A string operand: a string place, or a string literal.
    fn string(&self, o: &Operand) -> Result<Type, String> {
        match o {
            Operand::Const(Const::Str(s)) => {
                Ok(Type::Char(s.len().max(1).try_into().unwrap_or(u32::MAX)))
            }
            Operand::Const(Const::Hex(h)) => {
                Ok(Type::Char(h.len().max(1).try_into().unwrap_or(u32::MAX)))
            }
            Operand::Const(Const::Blanks) => Ok(Type::Char(1)),
            Operand::Const(c) => Err(format!(
                "{} is not a string",
                super::typing::describe_const(c)
            )),
            Operand::Reg(r) => Err(format!(
                "%{} is a register, not a string",
                self.p.reg_name(*r)
            )),
            Operand::Place(pl) => self.string_place(pl),
        }
    }

    fn string_place(&self, pl: &crate::inst::Place) -> Result<Type, String> {
        let t = place_type(self.m, self.p, pl)?;
        if t.is_string() {
            Ok(t)
        } else {
            Err(format!("a {t} is not a string"))
        }
    }

    /// Two string operands of the same class.
    fn same_string_class(&self, a: &Operand, b: &Operand) -> Result<(), String> {
        let ta = self.string(a)?;
        let tb = self.string(b)?;
        if ta.class() == tb.class() {
            Ok(())
        } else {
            Err(format!("a {ta} and a {tb} do not mix"))
        }
    }

    /// A `char(10)` place or a string literal of at most 10 characters.
    fn name_operand(&self, o: &Operand) -> Result<(), String> {
        match o {
            Operand::Const(Const::Str(s)) if !s.is_empty() && s.len() <= 10 => Ok(()),
            Operand::Const(_) => Err("a name literal is 1 to 10 characters".into()),
            Operand::Reg(_) => Err("a name is a char(10) place".into()),
            Operand::Place(pl) => match place_type(self.m, self.p, pl)? {
                Type::Char(10) => Ok(()),
                t => Err(format!("a name is a char(10), not a {t}")),
            },
        }
    }

    /// Any place, or a string or hex literal: raw bytes.
    fn bytes_operand(&self, o: &Operand) -> Result<(), String> {
        match o {
            Operand::Const(Const::Str(_) | Const::Hex(_)) => Ok(()),
            Operand::Const(c) => Err(format!("{} is not data", super::typing::describe_const(c))),
            Operand::Reg(r) => Err(format!("%{} is a register, not data", self.p.reg_name(*r))),
            Operand::Place(pl) => place_type(self.m, self.p, pl).map(|_| ()),
        }
    }

    fn datetime(&self, o: &Operand) -> Result<Type, String> {
        let t = self.ty(o, None)?;
        if t.class() == TypeClass::DateTime {
            Ok(t)
        } else {
            Err(format!("a {t} is not a date, time or timestamp"))
        }
    }
}

fn convert(cx: &Cx, dst: &crate::inst::Place, src: &Operand, opts: &CvtOpts) -> Result<(), String> {
    let t = place_type(cx.m, cx.p, dst)?;
    let u = match src {
        Operand::Const(Const::Str(s)) => {
            Type::Char(u32::try_from(s.len().max(1)).unwrap_or(u32::MAX))
        }
        Operand::Const(Const::Int(_) | Const::Dec(_)) => {
            Type::Packed(Shape::new(63, 0).expect("valid"))
        }
        Operand::Const(c) => return Err(format!("cvt from {}", super::typing::describe_const(c))),
        _ => cx.ty(src, None)?,
    };
    let ok = match (t.class(), u.class()) {
        (
            C::Char,
            C::Ucs2 | C::Graphic | C::Int | C::UInt | C::Float | C::Decimal | C::DateTime | C::Ind,
        )
        | (
            C::Ucs2 | C::Graphic | C::Int | C::UInt | C::Float | C::Decimal | C::DateTime | C::Ind,
            C::Char,
        )
        | (C::DateTime, C::Int | C::UInt | C::Decimal)
        | (C::Int | C::UInt | C::Decimal, C::DateTime) => true,
        (C::DateTime, C::DateTime) => t != u,
        _ => false,
    };
    if !ok {
        return Err(format!(
            "no conversion from a {u} to a {t}; use mov or dec.mov"
        ));
    }
    if opts.ccsid.is_some() && !(t.is_string() && u.is_string()) {
        return Err("ccsid applies to text conversions".into());
    }
    if opts.halfadj && !t.is_numeric() {
        return Err("halfadj needs a numeric destination".into());
    }
    Ok(())
}

fn string(cx: &Cx, s: &StrInst) -> Result<(), String> {
    match s {
        StrInst::Cat { dst, a, b } => {
            cx.string_place(dst)?;
            cx.same_string_class(a, b)
        }
        StrInst::Subst {
            dst,
            src,
            start,
            len,
        } => {
            cx.string_place(dst)?;
            cx.string(src)?;
            cx.int(start)?;
            cx.int(len)
        }
        StrInst::Scan {
            needle, hay, start, ..
        } => {
            cx.same_string_class(needle, hay)?;
            start.as_ref().map_or(Ok(()), |s| cx.int(s))
        }
        StrInst::Check {
            set, src, start, ..
        } => {
            cx.same_string_class(set, src)?;
            start.as_ref().map_or(Ok(()), |s| cx.int(s))
        }
        StrInst::Xlate {
            dst,
            from,
            to,
            src,
            start,
        } => {
            cx.string_place(dst)?;
            cx.same_string_class(from, to)?;
            cx.same_string_class(from, src)?;
            start.as_ref().map_or(Ok(()), |s| cx.int(s))
        }
        StrInst::Trim {
            dst, src, chars, ..
        } => {
            cx.string_place(dst)?;
            cx.string(src)?;
            chars.as_ref().map_or(Ok(()), |c| cx.string(c).map(|_| ()))
        }
        StrInst::Cmp { a, b, .. } => {
            cx.same_string_class(a, b)?;
            Cx::not_both_literals(a, b)
        }
        StrInst::Len { src, .. } => cx.string(src).map(|_| ()),
        StrInst::SetLen { dst, n } => {
            let t = cx.string_place(dst)?;
            if !t.is_varying() {
                return Err(format!("str.setlen of a {t}, which is not varying"));
            }
            cx.int(n)
        }
        StrInst::Replace {
            dst,
            src,
            repl,
            start,
            len,
        } => {
            cx.string_place(dst)?;
            cx.same_string_class(src, repl)?;
            cx.int(start)?;
            len.as_ref().map_or(Ok(()), |l| cx.int(l))
        }
    }
}

fn unit_fits(t: &Type, unit: DtUnit) -> Result<(), String> {
    let ok = match t {
        Type::Date(_) => unit.is_date(),
        Type::Time(_) => !unit.is_date() && unit != DtUnit::MSeconds,
        Type::Timestamp(_) => true,
        _ => false,
    };
    if ok {
        Ok(())
    } else {
        Err(format!("{} does not apply to a {t}", unit.name()))
    }
}

fn datetime(cx: &Cx, d: &DtInst) -> Result<(), String> {
    match d {
        DtInst::Add { dst, src, n, unit } => {
            let t = place_type(cx.m, cx.p, dst)?;
            let u = cx.datetime(src)?;
            if t != u {
                return Err(format!("dt.add of a {u} into a {t}"));
            }
            cx.int(n)?;
            unit_fits(&t, *unit)
        }
        DtInst::Diff { a, b, unit, .. } => {
            let ta = cx.datetime(a)?;
            let tb = cx.datetime(b)?;
            let class = |t: &Type| matches!(t, Type::Date(_) | Type::Timestamp(_));
            if class(&ta) != class(&tb) {
                return Err(format!("dt.diff of a {ta} and a {tb}"));
            }
            unit_fits(&ta, *unit).or_else(|_| unit_fits(&tb, *unit))
        }
        DtInst::Extract { src, unit, .. } => {
            let t = cx.datetime(src)?;
            unit_fits(&t, *unit)
        }
        DtInst::Test { src, .. } => {
            if matches!(src, Operand::Const(Const::Str(_) | Const::Int(_))) {
                return Ok(());
            }
            let t = cx.ty(src, None)?;
            if t.is_string() || t.is_numeric() || t.class() == TypeClass::DateTime {
                Ok(())
            } else {
                Err(format!("dt.test of a {t}"))
            }
        }
        DtInst::Now { dst } => {
            let t = place_type(cx.m, cx.p, dst)?;
            if t.class() == TypeClass::DateTime {
                Ok(())
            } else {
                Err(format!("dt.now into a {t}"))
            }
        }
    }
}

fn pointer(cx: &Cx, pt: &PtrInst) -> Result<(), String> {
    match pt {
        PtrInst::Addr { place, .. } => place_type(cx.m, cx.p, place).map(|_| ()),
        PtrInst::PtrAdd { ptr, n, .. } => {
            let t = cx.ty(ptr, Some(&Type::Ptr))?;
            if t != Type::Ptr {
                return Err(format!("ptradd of a {t}"));
            }
            cx.int(n)
        }
        PtrInst::PtrDiff { p, q, .. } => {
            for o in [p, q] {
                let t = cx.ty(o, Some(&Type::Ptr))?;
                if t != Type::Ptr {
                    return Err(format!("ptrdiff of a {t}"));
                }
            }
            Ok(())
        }
        PtrInst::ProcAddr { callee, .. } => match callee {
            Callee::Local(r) if (r.0 as usize) < cx.m.procs.len() => Ok(()),
            Callee::Import(r) => match cx.m.protos.get(r.0 as usize) {
                Some(pr) if pr.linkage == Linkage::Import => Ok(()),
                Some(pr) => Err(format!("{} is not a bound procedure", pr.name)),
                None => Err("prototype does not exist".into()),
            },
            Callee::Local(_) => Err("procedure does not exist".into()),
        },
        PtrInst::Rslvsp {
            obj_type,
            name,
            lib,
            ..
        } => {
            if !obj_type.starts_with('*') || obj_type.len() < 2 {
                return Err(format!("{obj_type} is not an object type"));
            }
            cx.name_operand(name)?;
            cx.name_operand(lib)
        }
    }
}

#[allow(clippy::too_many_lines)] // one arm per instruction
fn io_inst(cx: &Cx, io: &IoInst) -> Result<(), String> {
    let file = |ix: u32| {
        cx.m.files
            .get(ix as usize)
            .ok_or_else(|| "file does not exist".to_string())
    };
    match io {
        IoInst::DbOpen { file: f, infds, .. } => {
            file(*f)?;
            if let Some(ds) = infds {
                let t = place_type(cx.m, cx.p, ds)?;
                if t.class() != TypeClass::Aggregate && t.class() != TypeClass::Char {
                    return Err(format!("infds is a {t}"));
                }
            }
            Ok(())
        }
        IoInst::DbClose { file: f, .. } => file(*f).map(|_| ()),
        IoInst::FileRef { file: f, .. } => {
            let fr = file(*f)?;
            if fr.dev != FileDev::Db {
                return Err(format!("{} is not a database file", fr.name));
            }
            Ok(())
        }
        IoInst::FileBind { file: f, handle } => {
            let fr = file(*f)?;
            if fr.dev != FileDev::Db {
                return Err(format!("{} is not a database file", fr.name));
            }
            cx.int(handle)
        }
        IoInst::DbFb { file: f, .. } => {
            let fr = file(*f)?;
            if fr.dev == FileDev::Printer {
                return Err(format!("{} is a printer file", fr.name));
            }
            Ok(())
        }
        IoInst::DbIo {
            file: f,
            op,
            rec,
            key,
            rrn,
            fmt,
            nulls,
            ..
        } => {
            let fr = file(*f)?;
            if fr.dev != FileDev::Db {
                return Err(format!("{} is not a database file", fr.name));
            }
            if let Some(r) = rec {
                place_type(cx.m, cx.p, r)?;
            } else if op.reads() || op.writes() {
                return Err(format!("{} needs rec()", op.name()));
            }
            if let Some(n) = nulls {
                let t = place_type(cx.m, cx.p, n)?;
                if t.class() != TypeClass::Aggregate && t.class() != TypeClass::Char {
                    return Err(format!("nulls is a {t}"));
                }
            }
            if key.is_some() && rrn.is_some() {
                return Err("key() and rrn() together".into());
            }
            if (key.is_some() || rrn.is_some()) && !op.positions() {
                return Err(format!("{} takes no key or rrn", op.name()));
            }
            if op.positions() && key.is_none() && rrn.is_none() {
                return Err(format!("{} needs key() or rrn()", op.name()));
            }
            if let Some(k) = key {
                cx.bytes_operand(k)?;
                if !fr.keyed {
                    return Err(format!("{} is not keyed", fr.name));
                }
            }
            if let Some(n) = rrn {
                cx.int(n)?;
            }
            if let Some(name) = fmt
                && !fr.formats.is_empty()
                && !fr.formats.iter().any(|x| x.name == *name)
            {
                return Err(format!("{} has no format {name}", fr.name));
            }
            Ok(())
        }
        IoInst::DevIo {
            file: f,
            op,
            rec,
            ctl,
            rrn,
            inds,
            ..
        } => {
            if let Some(c) = ctl {
                for o in c.iter() {
                    cx.int(o)?;
                }
            }
            if let Some(n) = rrn {
                cx.int(n)?;
            }
            if let Some(p) = inds {
                place_type(cx.m, cx.p, p)?;
            }
            let fr = file(*f)?;
            if fr.dev == FileDev::Db {
                return Err(format!("{} is a database file", fr.name));
            }
            if ctl.is_some() && fr.dev != FileDev::Printer {
                return Err(format!("{} is not a printer file; ctl() is for printers", fr.name));
            }
            if fr.dev == FileDev::Printer && *op != DevOp::Write {
                return Err(format!(
                    "a printer file takes only WRITE, not {}",
                    op.name()
                ));
            }
            if let Some(r) = rec {
                place_type(cx.m, cx.p, r)?;
            }
            Ok(())
        }
    }
}

fn queue(cx: &Cx, q: &MsgQueue) -> Result<(), String> {
    match q {
        MsgQueue::Up(n) => cx.int(n),
        _ => Ok(()),
    }
}

fn message(cx: &Cx, msg: &MsgInst) -> Result<(), String> {
    let fixed = |pl: &crate::inst::Place, n: u32, what: &str| -> Result<(), String> {
        match place_type(cx.m, cx.p, pl)? {
            Type::Char(k) if k == n => Ok(()),
            t => Err(format!("{what} is a char({n}), not a {t}")),
        }
    };
    let msgid = |o: &Operand| -> Result<(), String> {
        match o {
            Operand::Const(Const::Str(s)) if s.len() == 7 => Ok(()),
            Operand::Place(pl) => fixed(pl, 7, "msgid"),
            _ => Err("msgid is a char(7) place or a 7-character literal".into()),
        }
    };
    match msg {
        MsgInst::Snd {
            msgid: id,
            msgf,
            msgf_lib,
            data,
            to,
            key,
            ..
        } => {
            msgid(id)?;
            cx.name_operand(msgf)?;
            if let Some(l) = msgf_lib {
                cx.name_operand(l)?;
            }
            if let Some(d) = data {
                cx.bytes_operand(d)?;
            }
            queue(cx, to)?;
            key.as_ref().map_or(Ok(()), |k| fixed(k, 4, "key"))
        }
        MsgInst::Rcv {
            pgmq,
            key,
            msg,
            msgid: id,
            data,
            keyout,
            sev,
            ..
        } => {
            queue(cx, pgmq)?;
            if let Some(k) = key {
                match k {
                    Operand::Const(Const::Hex(h)) if h.len() == 4 => {}
                    Operand::Place(pl) => fixed(pl, 4, "key")?,
                    _ => return Err("key is a char(4) place or 4 hex bytes".into()),
                }
            }
            if let Some(pl) = msg {
                cx.string_place(pl)?;
            }
            if let Some(pl) = id {
                fixed(pl, 7, "msgid")?;
            }
            if let Some(pl) = data {
                place_type(cx.m, cx.p, pl)?;
            }
            if let Some(pl) = keyout {
                fixed(pl, 4, "keyout")?;
            }
            if let Some(pl) = sev {
                cx.numeric_place(pl)?;
            }
            Ok(())
        }
        MsgInst::Rmv { pgmq, key, clear } => {
            queue(cx, pgmq)?;
            if let Some(k) = key {
                match k {
                    Operand::Const(Const::Hex(h)) if h.len() == 4 => {}
                    Operand::Place(pl) => fixed(pl, 4, "key")?,
                    _ => return Err("key is a char(4) place or 4 hex bytes".into()),
                }
            }
            if !matches!(clear.as_str(), "*ALL" | "*OLD" | "*NEW" | "*KEEPUNANS") {
                return Err(format!(
                    "clear({clear}) is not *ALL, *OLD, *NEW or *KEEPUNANS"
                ));
            }
            Ok(())
        }
    }
}

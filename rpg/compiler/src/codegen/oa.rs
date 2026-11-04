// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Open Access: before a `HANDLER` file opens, the runtime is given the
//! handler (a name, or a procedure pointer for a prototype or pointer
//! variable), the user area and a description of the file
//! (`_QRNX_OAOPEN`); the open and every operation after it go to the
//! handler (`qrpgrt::oa`).
//!
//! The description is records separated by `;`: `F|device|external|
//! keyed|blocked|commit|file|library|member|compile file|compile
//! library|record length|form length|overflow line|update`, then per format
//! `R|name|level|length|key fields` and per field `V|name|type|digits|
//! decimals|format|separator|offset|bytes|descriptor|usage` (`type` as
//! `QrnDatatype_*`, the descriptor as the runtime's).

use rpgfront::ast::{Expr, Lit};

use super::Gen;
use super::emit::quote;
use super::expr::{V, Vk, Want};
use crate::files::{Dev, File};
use crate::names::mangle;
use crate::syms::Kind;
use crate::types::Ty;

/// `QrnDatatype_*` of a field, its defined length (digits, or bytes of
/// an integer or float) and decimals.
fn datatype(t: &Ty) -> (u8, u32, u8) {
    match t {
        // The defined length is for numbers only.
        Ty::Char(_) => (1, 0, 0),
        Ty::VarChar(..) => (2, 0, 0),
        Ty::Ucs2(_) => (3, 0, 0),
        Ty::VarUcs2(..) => (4, 0, 0),
        Ty::Graph(_) => (5, 0, 0),
        Ty::VarGraph(..) => (6, 0, 0),
        Ty::Ind => (7, 0, 0),
        Ty::Packed(d, s) | Ty::Zoned(d, s) | Ty::Bin(d, s) => (8, u32::from(*d), *s),
        Ty::Int(d) => (9, int_bytes(*d), 0),
        Ty::Uns(d) => (10, int_bytes(*d), 0),
        Ty::Float(b) => (11, u32::from(*b), 0),
        Ty::Date(..) => (12, 0, 0),
        Ty::Time(..) => (13, 0, 0),
        Ty::Timestamp(_) => (14, 0, 0),
        _ => (1, 0, 0),
    }
}

/// `QrnDtzFormat_*` of a date or time format.
fn dtz_code(f: &str) -> u8 {
    match f.to_ascii_uppercase().as_str() {
        "ISO" => 1,
        "USA" => 2,
        "EUR" => 3,
        "JIS" => 4,
        "YMD" => 5,
        "MDY" => 6,
        "DMY" => 7,
        "JUL" => 8,
        "HMS" => 9,
        _ => 0,
    }
}

fn int_bytes(d: u8) -> u32 {
    match d {
        3 => 1,
        5 => 2,
        10 => 4,
        _ => 8,
    }
}

impl Gen<'_> {
    /// The file's description for the runtime.
    fn oa_desc(&self, f: &File) -> String {
        let dev = match f.dev {
            Dev::Printer => 'P',
            Dev::Workstn => 'U',
            _ => 'D',
        };
        let flag = |b: bool| if b { '1' } else { '0' };
        let fmts = f.ext.as_ref().map(|e| e.formats.clone()).unwrap_or_default();
        let reclen = fmts.iter().map(|r| r.len).max().unwrap_or(f.reclen);
        let (complib, compfile) = match f.ext.as_ref() {
            Some(e) => (e.lib.clone(), if f.desc.is_empty() { f.name.clone() } else { f.desc.clone() }),
            None => (String::new(), String::new()),
        };
        let mut s = format!(
            "F|{dev}|{}|{}|{}|{}|{}|{}|{}|{compfile}|{complib}|{reclen}|{}|{}|{}",
            flag(f.external),
            flag(f.keyed),
            '0',
            flag(f.commit.is_some()),
            f.obj.1,
            f.obj.0,
            f.member,
            f.form.0,
            f.form.1,
            flag(f.update)
        );
        for fm in &fmts {
            s.push_str(&format!(";R|{}|{}|{}|{}", fm.name, fm.level_id, fm.len, fm.keys.join(",")));
            for (i, x) in fm.fields.iter().enumerate() {
                let (dt, len, dec) = datatype(&x.ty);
                let (fmt, sep) = match &x.ty {
                    Ty::Date(d, sep) => (dtz_code(&format!("{d:?}")), *sep),
                    Ty::Time(t, sep) => (dtz_code(&format!("{t:?}")), *sep),
                    _ => (0, ' '),
                };
                let bytes = crate::extfiles::field_len(fm, i);
                let desc = self.desc_of(&V::place(x.ty.clone(), String::new()));
                s.push_str(&format!(
                    ";V|{}|{dt}|{len}|{dec}|{fmt}|{sep}|{}|{bytes}|{desc}|{}",
                    x.name, x.offset, x.usage
                ));
            }
        }
        s
    }

    /// Give the runtime the handler for the open that follows.
    pub fn oa_open(&mut self, f: &File) {
        let Some(k) = f.handler_kw.clone() else { return };
        let mut ptr = self.b.temp("char(16)");
        self.b.ins(format!("mov {ptr}, *LOVAL"));
        let mut name = (self.b.temp("char(1)"), self.desc_of(&V::place(Ty::Char(1), String::new())));
        match k.arg(0).and_then(|a| a.expr.clone()) {
            Some(Expr::Lit(Lit::Char(t), _)) => {
                if let Some(r) = self.literal(&Lit::Char(t)) {
                    let r = self.rt_of(&r, None);
                    name = (r.op, r.desc);
                }
            }
            Some(Expr::Ref { name: n, steps, at }) => {
                let up = n.to_ascii_uppercase();
                let proto = matches!(self.t.find(self.scope, &up).and_then(|id| self.t.syms[id].kind.clone()), Some(Kind::Proto(_)));
                if proto {
                    if let Some(id) = self.t.find(self.scope, &up) {
                        self.t.refer(id, self.at.line, false);
                    }
                    let ext = match self.t.find(self.scope, &up).and_then(|id| self.t.syms[id].kind.clone()) {
                        Some(Kind::Proto(pi)) => self.t.protos[pi].extproc.clone().unwrap_or_else(|| up.clone()),
                        _ => up.clone(),
                    };
                    let target = if self.local_procs.contains(&up) { mangle(&up) } else { mangle(&ext) };
                    let r = self.b.reg();
                    self.b.ins(format!("{r} = procaddr {target}"));
                    ptr = self.b.temp("procptr");
                    self.b.ins(format!("store {ptr}, {r}"));
                } else if let Some(v) = self.expr(&Expr::Ref { name: n, steps, at }, Want::default()) {
                    if v.ty == Ty::ProcPtr {
                        let r = self.b.reg();
                        self.b.ins(format!("{r} = load {}", v.op));
                        ptr = self.b.temp("procptr");
                        self.b.ins(format!("store {ptr}, {r}"));
                    } else {
                        let r = self.rt_of(&v, None);
                        name = (r.op, r.desc);
                    }
                }
            }
            Some(e) => {
                if let Some(v) = self.expr(&e, Want::default()) {
                    let r = self.rt_of(&v, None);
                    name = (r.op, r.desc);
                }
            }
            None => {}
        }
        let (ua, has) = match k.arg(1).and_then(|a| a.expr.clone()) {
            Some(e) => match self.expr(&e, Want::default()) {
                Some(v) if v.kind == Vk::Place => {
                    if let Some(s) = v.sym {
                        self.t.refer(s, self.at.line, true);
                    }
                    let op = if matches!(v.ty, Ty::Ds(_)) { format!("{}.all_", v.op) } else { v.op.clone() };
                    let size = self.t.size(&v.ty) * v.dim.max(1);
                    (op, i32::try_from(size).unwrap_or(i32::MAX).max(1))
                }
                _ => (self.b.temp("char(1)"), 0),
            },
            None => (self.b.temp("char(1)"), 0),
        };
        let desc = self.oa_desc(f);
        self.rt_proc(
            "_QRNX_OAOPEN",
            "($name: bytes(1) *VARSIZE, byval $nd: i64, $ptr: bytes(16) *VARSIZE, $ua: bytes(1) *VARSIZE, \
             byval $size: i32, const $desc: varchar(32000))",
        );
        self.b.ins(format!("callb _QRNX_OAOPEN({}, {}, {ptr}, {ua}, {has}, {})", name.0, name.1, quote(&desc)));
    }
}

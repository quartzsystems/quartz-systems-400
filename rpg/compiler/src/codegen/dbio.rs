// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Database file operations. Each disk file has a record buffer; a
//! read fills it and moves the format's fields (or the program-described
//! input fields) into the program's, a write or update moves them back
//! first; data structure I/O uses the structure instead. After every
//! operation the file's feedback (`dbfb`) sets `%EOF`, `%FOUND` and
//! `%EQUAL` for the file and for the operation kind; `(E)` and error
//! indicators set `%ERROR` and `%STATUS`. Keys are built in record
//! layout from a single value, a list `(a:b)`, `%KDS` or a `KLIST`;
//! `*START`, `*END`, `*LOVAL` and `*HIVAL` position at the limits.

use std::fmt::Write as _;

use rpgfront::ast::{Expr, Op, Step};

use crate::codegen::Gen;
use crate::codegen::expr::{V, Vk, Want};
use crate::diag::ids;
use crate::extfiles::{RecFmt, input_type};
use crate::files::{Dev, File};
use crate::names::mangle;
use crate::syms::Kind;
use crate::types::Ty;

/// What positions an operation.
#[derive(Clone, Debug)]
pub enum KeyArg {
    /// Nothing: the key of the last positioning.
    None,
    /// Key fields in record layout, a place.
    Key(String),
    /// A relative record number, an `i64` register.
    Rrn(String),
    /// A limit: -1 `*START`, -2 `*END`, -3 `*LOVAL`, -4 `*HIVAL`.
    Limit(i32),
}

/// The statics a module's files need: record buffers, feedback flags
/// and statuses, and the last-operation flags.
#[must_use]
pub fn statics(files: &[File], t: &crate::syms::Table) -> String {
    let mut out = String::new();
    for f in files
        .iter()
        .filter(|f| (matches!(f.dev, Dev::Disk | Dev::Workstn | Dev::Special) || f.dev == Dev::Printer && f.external) && !f.template)
    {
        let n = mangle(&f.name);
        let len = buffer_len(f, t).max(1);
        let _ = writeln!(out, "static $rb_{n}: char({len})");
        if (f.dev == Dev::Workstn || f.recno.is_some()) && f.infds.is_none() {
            // The display feedback a read by file name looks at.
            let _ = writeln!(out, "static $fi_{n}: char(416)");
        }
        for x in ["eof", "fnd", "eq"] {
            let _ = writeln!(out, "static ${x}_{n}: ind");
        }
        let _ = writeln!(out, "static $st_{n}: zoned(5,0)");
        let _ = writeln!(out, "static $rt_{n}: i32");
    }
    // The last operation's flags; its status and error are the
    // program's ($status, $errind), which exceptions set too.
    for x in ["eof", "fnd", "eq"] {
        let _ = writeln!(out, "static ${x}_: ind");
    }
    out
}

/// A file's record buffer length: its longest format, or the
/// program-described length.
fn buffer_len(f: &File, t: &crate::syms::Table) -> u32 {
    let ext = t.recfmts.iter().filter(|r| r.file_name == f.name).map(|r| r.fmt.len).max();
    ext.unwrap_or(0).max(f.reclen)
}

impl Gen<'_> {
    /// The file an operand names (a file, a record format or
    /// `file.format`) and the format.
    pub fn db_target(&mut self, e: &Expr) -> Option<(File, Option<RecFmt>)> {
        let (name, fmt) = match e {
            Expr::Ref { name, steps, .. } => match steps.first() {
                Some(Step::Field(f)) => (name.clone(), Some(f.clone())),
                _ => (name.clone(), None),
            },
            _ => {
                self.err(ids::FILE_NOT_DEFINED, 30, "");
                return None;
            }
        };
        if let Some(fmt) = fmt {
            if let Some(i) = self.t.recfmts.iter().position(|r| r.file_name == name && r.name == fmt) {
                self.used_formats.insert(i);
            }
            let r = self.t.recfmts.iter().find(|r| r.file_name == name && r.name == fmt).cloned();
            let f = self.files.iter().find(|f| f.name == name).cloned();
            return match (f, r) {
                (Some(f), Some(r)) => Some((f, Some(r))),
                _ => {
                    self.err(ids::FILE_NOT_DEFINED, 30, format!("{name}.{fmt}"));
                    None
                }
            };
        }
        let sym = self.t.find(self.scope, &name).map(|id| self.t.syms[id].kind.clone());
        match sym {
            Some(Some(Kind::File(i))) => {
                let f = self.files.get(i).cloned()?;
                let r = self.t.recfmts.iter().find(|r| r.file_name == f.name).cloned();
                Some((f, r))
            }
            Some(Some(Kind::Format(i))) => {
                self.used_formats.insert(i);
                let r = self.t.recfmts.get(i).cloned()?;
                let f = self.files.get(r.file).cloned()?;
                Some((f, Some(r)))
            }
            // A name declared nowhere; the caller says how it is used.
            // A file dropped at its declaration (`RNF2109`).
            None if self.t.dropped_files.contains(&name) => {
                self.err(ids::UNDEFINED_OPERAND, 30, "");
                None
            }
            None => {
                self.err(ids::NOT_DEFINED, 30, name);
                None
            }
            _ => {
                self.err(ids::FILE_NOT_DEFINED, 30, name);
                None
            }
        }
    }

    /// `OPEN` a file, at initialization or by the operation: its status
    /// register.
    pub fn db_open(&mut self, f: &File) -> String {
        if f.dev == Dev::Special {
            return self.special_open(f);
        }
        // BLOCKED(phase9): Open Access handlers are not called; opening a
        // HANDLER file ends the program as a handler IBM i cannot find
        // does (its error is no file error the operation can take).
        if f.handler {
            self.rt_proc("_QRNX_NOHANDLER", "()");
            self.b.ins("callb _QRNX_NOHANDLER()");
        }
        let r = self.b.reg();
        let infds = if f.dev == Dev::Workstn || f.recno.is_some() {
            match f.infds.as_ref().and_then(|n| self.t.find(self.scope, n)) {
                Some(id) => Some(crate::codegen::place_of(self.t, id)),
                None => Some(format!("$fi_{}", mangle(&f.name))),
            }
        } else {
            f.infds.as_ref().and_then(|n| self.t.find(self.scope, n)).map(|id| crate::codegen::place_of(self.t, id))
        };
        match infds {
            Some(p) => self.b.ins(format!("{r} = dbopen {} infds({p})", mangle(&f.name))),
            None => self.b.ins(format!("{r} = dbopen {}", mangle(&f.name))),
        }
        if matches!(f.dev, Dev::Disk | Dev::Workstn) {
            let n = mangle(&f.name);
            self.b.ins(format!("store $eof_{n}, *OFF"));
        } else {
            // Other files keep whether they are open themselves.
            let po = self.open_flag(f);
            let ok = self.b.reg();
            self.b.ins(format!("{ok} = cmp.lt {r}, 1000"));
            self.b.ins(format!("store {po}, {ok}"));
        }
        r
    }

    /// A file the program opens at its start: with `EXTIND`, only when
    /// the external indicator is on.
    pub fn init_open(&mut self, f: &File) {
        let Some(ind) = f.extind.clone() else {
            let _ = self.db_open(f);
            return;
        };
        let Some(x) = self.indicator(ind.trim_start_matches("*IN")) else { return };
        let on = self.b.reg();
        self.b.ins(format!("{on} = load {}", x.op));
        let (yes, no) = (self.b.label(), self.b.label());
        self.b.term(format!("brif {on}, {yes}, {no}"));
        self.b.place(&yes);
        let _ = self.db_open(f);
        self.b.term(format!("br {no}"));
        self.b.place(&no);
    }

    /// `RECNO`: a record read gives its relative record number (INFDS
    /// position 397) to the field.
    fn recno_after(&mut self, f: &File, status: &str) {
        let Some(name) = f.recno.clone() else { return };
        let Some(id) = self.t.find(self.scope, &name.to_ascii_uppercase()) else {
            self.err(ids::NOT_DEFINED, 30, name);
            return;
        };
        let infds = match f.infds.as_ref().and_then(|n| self.t.find(self.scope, n)) {
            Some(i) => crate::codegen::place_of(self.t, i),
            None => format!("$fi_{}", mangle(&f.name)),
        };
        let field = crate::codegen::place_of(self.t, id);
        let (yes, no) = (self.b.label(), self.b.label());
        let ok = self.b.reg();
        self.b.ins(format!("{ok} = cmp.eq {status}, 0"));
        self.b.term(format!("brif {ok}, {yes}, {no}"));
        self.b.place(&yes);
        let a = self.b.reg();
        self.b.ins(format!("{a} = addr {infds}"));
        let r = self.b.reg();
        self.b.ins(format!("{r} = load [{a} +396]:i32"));
        let t = self.b.temp("i32");
        self.b.ins(format!("store {t}, {r}"));
        self.b.ins(format!("dec.mov {field}, {t}"));
        self.b.term(format!("br {no}"));
        self.b.place(&no);
    }

    /// The flag a printer or special file keeps of being open.
    pub fn open_flag(&mut self, f: &File) -> String {
        let po = format!("$po_{}", mangle(&f.name));
        self.externs.entry(po.clone()).or_insert_with(|| format!("static {po}: ind inz(*OFF)"));
        po
    }

    /// One database operation statement.
    pub fn db_op(&mut self, op: &Op, base: &str) {
        match base {
            "OPEN" => return self.open_op(op),
            "CLOSE" => return self.close_op(op),
            "KLIST" | "KFLD" => return,
            _ => {}
        }
        let Some(target) = op.f2.first().cloned() else {
            self.err(ids::FILE_NOT_DEFINED, 30, op.code.clone());
            return;
        };
        let Some((f, fmt)) = self.db_target(&target) else {
            // An undefined file of an operation with a search argument.
            if !op.f1.is_empty() && self.diags.last().is_some_and(|d| d.msgid == ids::NOT_DEFINED) {
                self.err(ids::UNDEFINED_OPERAND, 30, "");
            }
            return;
        };
        // What a printer cannot do.
        if f.dev == Dev::Printer {
            match base {
                "CHAIN" | "SETLL" | "SETGT" | "DELETE" | "UPDATE" | "UNLOCK" => {
                    self.err(ids::OP_NOT_FOR_FILE, 30, f.name.clone());
                    return;
                }
                "WRITE" if !f.external && op.result.is_empty() => {
                    self.err(ids::WRITE_NO_DS, 30, f.name.clone());
                    return;
                }
                _ => {}
            }
        }
        if f.dev == Dev::Workstn {
            return self.dsp_op(op, base, &f, fmt, &target);
        }
        if f.dev == Dev::Printer && f.external && base == "WRITE" {
            return self.prt_write(&f, fmt);
        }
        if f.dev == Dev::Special {
            return self.special_op(op, base, &f);
        }
        if f.dev != Dev::Disk {
            self.err(ids::BAD_FILE_OP, 30, f.name.clone());
            return;
        }
        let n = mangle(&f.name);
        let nolock = op.ext.contains('N');
        // The record place: a result structure, or the file's buffer.
        let ds = match op.result.first() {
            Some(Expr::Bif { name, .. }) if name == "%FIELDS" => None,
            Some(e) => {
                let v = self.expr(e, Want::default());
                v.map(|v| v.op)
            }
            None => None,
        };
        let rec = ds.clone().unwrap_or_else(|| format!("$rb_{n}"));
        let key = match base {
            "CHAIN" | "SETLL" | "SETGT" | "READE" | "READPE" => match op.f1.first() {
                Some(e) => {
                    let e = e.clone();
                    match self.key_of(&f, fmt.as_ref(), &e) {
                        Some(k) => k,
                        None => return,
                    }
                }
                None => KeyArg::None,
            },
            "DELETE" => match op.f1.first() {
                Some(e) => {
                    let e = e.clone();
                    match self.key_of(&f, fmt.as_ref(), &e) {
                        Some(k) => k,
                        None => return,
                    }
                }
                None => KeyArg::None,
            },
            _ => KeyArg::None,
        };
        // Fields into the buffer before an output operation.
        if matches!(base, "WRITE" | "UPDATE") && ds.is_none() {
            let only: Option<Vec<String>> = match op.result.first() {
                Some(Expr::Bif { name, args, .. }) if name == "%FIELDS" => {
                    Some(args.iter().filter_map(|a| a.name().map(str::to_string)).collect())
                }
                _ => None,
            };
            match &fmt {
                Some(r) => self.fields_out(&n, r, only.as_deref()),
                None => self.err(ids::BAD_FILE_OP, 30, f.name.clone()),
            }
        }
        let status;
        if base == "DELETE" && !matches!(key, KeyArg::None) {
            // DELETE by key: find the record, then delete it.
            status = self.dbio(&n, "CHAIN", Some("$rb_".to_string() + &n), &key, false);
            self.feedback(&n, "CHAIN");
            let del = self.b.label();
            let done = self.b.label();
            let ok = self.b.reg();
            self.b.ins(format!("{ok} = cmp.eq {status}, 0"));
            self.b.term(format!("brif {ok}, {del}, {done}"));
            self.b.place(&del);
            let s2 = self.dbio(&n, "DELETE", None, &KeyArg::None, false);
            self.b.ins(format!("{status} = add {s2}, 0"));
            self.b.place(&done);
        } else {
            let irop = match base {
                "READ" | "READP" | "READE" | "READPE" | "CHAIN" | "SETLL" | "SETGT" | "WRITE" | "UPDATE" | "DELETE" | "UNLOCK" | "FEOD" => base,
                _ => {
                    self.err(ids::UNSUPPORTED, 30, op.code.clone());
                    return;
                }
            };
            let rec = matches!(base, "READ" | "READP" | "READE" | "READPE" | "CHAIN" | "WRITE" | "UPDATE").then(|| rec.clone());
            let key = if matches!(base, "READE" | "READPE") && matches!(key, KeyArg::None) {
                KeyArg::Rrn("0".into())
            } else {
                key.clone()
            };
            status = self.dbio(&n, irop, rec, &key, nolock);
            self.feedback(&n, base);
            if base == "FEOD" && f.input {
                // The next read of the file is at its end.
                let _ = self.dbio(&n, "SETLL", None, &KeyArg::Limit(-2), false);
            }
        }
        // A record read: into the program's fields.
        if matches!(base, "READ" | "READP" | "READE" | "READPE" | "CHAIN") && ds.is_none() {
            let got = self.b.label();
            let skip = self.b.label();
            let ok = self.b.reg();
            self.b.ins(format!("{ok} = cmp.eq {status}, 0"));
            self.b.term(format!("brif {ok}, {got}, {skip}"));
            self.b.place(&got);
            match &fmt {
                Some(r) => self.fields_in(&n, r),
                None => self.input_specs(&f),
            }
            self.b.place(&skip);
        }
        self.db_result(op, base, &n, &status, &key);
        if matches!(base, "READ" | "READP" | "READE" | "READPE" | "CHAIN") {
            self.recno_after(&f, &status);
        }
        let by_file = matches!(&target, Expr::Ref { name, steps, .. } if *name == f.name && steps.is_empty());
        self.infds_opcode(&f, base, by_file);
    }

    /// The INFDS operation code: five characters of the operation and
    /// `F` (by file name) or `R` (by record format).
    pub fn infds_opcode(&mut self, f: &File, base: &str, by_file: bool) {
        let Some(id) = f.infds.as_ref().and_then(|n| self.t.find(self.scope, n)) else { return };
        let Some(Kind::Ds(di)) = self.t.syms[id].kind.clone() else { return };
        if self.t.ds_len(di) < 21 {
            return;
        }
        let code = match base {
            "READPE" => "REDPE".to_string(),
            b => b.chars().take(5).collect::<String>(),
        };
        let text = format!("{code:<5}{}", if by_file { 'F' } else { 'R' });
        let place = crate::codegen::place_of(self.t, id);
        let p = self.b.reg();
        self.b.ins(format!("{p} = addr {place}"));
        self.b.ins(format!("mov [{p} +15]:char(6), {}", crate::codegen::emit::quote(&text)));
    }

    /// The `dbio` instruction; its status register.
    fn dbio(&mut self, n: &str, op: &str, rec: Option<String>, key: &KeyArg, nolock: bool) -> String {
        let r = self.b.reg();
        let mut s = format!("{r} = dbio {n} {op}");
        if let Some(p) = rec {
            let _ = write!(s, " rec({p})");
        }
        match key {
            KeyArg::None => {}
            KeyArg::Key(p) => {
                let _ = write!(s, " key({p})");
            }
            KeyArg::Rrn(x) => {
                let _ = write!(s, " rrn({x})");
            }
            KeyArg::Limit(l) => {
                let _ = write!(s, " rrn({l})");
            }
        }
        if nolock {
            s.push_str(" nolock");
        }
        self.b.ins(s);
        r
    }

    /// The file's feedback after an operation into its flags and the
    /// last-operation flags.
    fn feedback(&mut self, n: &str, base: &str) {
        let fb = self.b.reg();
        self.b.ins(format!("{fb} = dbfb {n}"));
        let bit = |g: &mut Self, m: i32| {
            let a = g.b.reg();
            g.b.ins(format!("{a} = and {fb}, {m}"));
            let c = g.b.reg();
            g.b.ins(format!("{c} = cmp.ne {a}, 0"));
            c
        };
        match base {
            "READ" | "READP" | "READE" | "READPE" => {
                let e = bit(self, 1);
                self.b.ins(format!("store $eof_{n}, {e}"));
                self.b.ins(format!("store $eof_, {e}"));
            }
            "CHAIN" | "SETGT" | "DELETE" => {
                let f = bit(self, 2);
                self.b.ins(format!("store $fnd_{n}, {f}"));
                self.b.ins(format!("store $fnd_, {f}"));
                if base != "DELETE" {
                    self.b.ins(format!("store $eof_{n}, *OFF"));
                }
            }
            "SETLL" => {
                let f = bit(self, 2);
                let q = bit(self, 4);
                self.b.ins(format!("store $fnd_{n}, {f}"));
                self.b.ins(format!("store $fnd_, {f}"));
                self.b.ins(format!("store $eq_{n}, {q}"));
                self.b.ins(format!("store $eq_, {q}"));
                self.b.ins(format!("store $eof_{n}, *OFF"));
            }
            _ => {}
        }
    }

    /// `%ERROR`, `%STATUS` and the resulting indicators of an operation.
    pub fn db_result(&mut self, op: &Op, base: &str, n: &str, status: &str, key: &KeyArg) {
        let err = self.b.reg();
        self.b.ins(format!("{err} = cmp.ge {status}, 1000"));
        let handled = op.ext.contains('E') || !op.resind[1].is_empty();
        // The file's status is the operation's (00012 too); the last
        // error status only changes under (E) or an error indicator.
        self.b.ins(format!("dec.mov $st_{n}, {status}"));
        if handled {
            self.b.ins(format!("store $errind, {err}"));
            let st = self.b.reg();
            self.b.ins(format!("{st} = select {err}, {status}, 0"));
            self.b.ins(format!("dec.mov $status, {st}"));
        }
        // TODO(phase9): an unhandled file error goes to the file's
        // exception handling (INFSR, the default handler).
        let ind = |g: &mut Self, slot: usize, r: &str| {
            let name = op.resind[slot].clone();
            if !name.is_empty() {
                g.set_ind(&name, r);
            }
        };
        ind(self, 1, &err);
        match base {
            "READ" | "READP" | "READE" | "READPE" => {
                let e = self.b.reg();
                self.b.ins(format!("{e} = load $eof_{n}"));
                ind(self, 2, &e);
            }
            "CHAIN" | "SETGT" => {
                let f = self.b.reg();
                self.b.ins(format!("{f} = load $fnd_{n}"));
                let nf = self.b.reg();
                self.b.ins(format!("{nf} = cmp.eq {f}, *OFF"));
                ind(self, 0, &nf);
            }
            "SETLL" => {
                let f = self.b.reg();
                self.b.ins(format!("{f} = load $fnd_{n}"));
                let nf = self.b.reg();
                self.b.ins(format!("{nf} = cmp.eq {f}, *OFF"));
                ind(self, 0, &nf);
                let q = self.b.reg();
                self.b.ins(format!("{q} = load $eq_{n}"));
                ind(self, 2, &q);
            }
            "DELETE" if !matches!(key, KeyArg::None) => {
                let f = self.b.reg();
                self.b.ins(format!("{f} = load $fnd_{n}"));
                let nf = self.b.reg();
                self.b.ins(format!("{nf} = cmp.eq {f}, *OFF"));
                ind(self, 0, &nf);
            }
            _ => {}
        }
    }

    /// The buffer's fields into the program's, then the field
    /// indicators of the format's input specifications.
    pub fn fields_in(&mut self, n: &str, r: &RecFmt) {
        let p = self.b.reg();
        self.b.ins(format!("{p} = addr $rb_{n}"));
        for (i, fd) in r.fmt.fields.iter().enumerate() {
            if !matches!(fd.usage, 'I' | 'B') {
                continue;
            }
            let Some(name) = r.names.get(i).cloned() else { continue };
            let Some(id) = self.t.find(self.scope, &name) else { continue };
            self.t.refer(id, self.at.line, true);
            let dst = V::place(self.t.syms[id].ty.clone(), crate::codegen::place_of(self.t, id));
            let src = V::place(fd.ty.clone(), format!("[{p} +{}]:{}", fd.offset, self.ir_ty(&fd.ty)));
            if dst.ty == src.ty {
                self.b.ins(format!("mov {}, {}", dst.op, src.op));
            } else {
                self.assign(&dst, &src, false);
            }
        }
        let specs: Vec<rpgfront::ast::Input> =
            self.inputs.iter().filter(|i| i.external && (i.name == r.name || i.name == r.fmt.name)).cloned().collect();
        for i in specs {
            if !i.rid.is_empty()
                && let Some(x) = self.indicator(&i.rid)
            {
                self.b.ins(format!("store {}, *ON", x.op));
            }
            for x in i.fields.iter().filter(|x| x.inds.iter().any(|s| !s.is_empty())) {
                let Some(k) = r.fmt.fields.iter().position(|d| d.name == x.extname) else { continue };
                let Some(name) = r.names.get(k).cloned() else { continue };
                let Some(id) = self.t.find(self.scope, &name) else { continue };
                let v = V::place(self.t.syms[id].ty.clone(), crate::codegen::place_of(self.t, id));
                self.field_inds(&v, &x.inds);
            }
        }
    }

    /// The program's fields into the buffer (only `only`'s, when given).
    fn fields_out(&mut self, n: &str, r: &RecFmt, only: Option<&[String]>) {
        let p = self.b.reg();
        self.b.ins(format!("{p} = addr $rb_{n}"));
        for (i, fd) in r.fmt.fields.iter().enumerate() {
            if !matches!(fd.usage, 'O' | 'B') {
                continue;
            }
            let Some(name) = r.names.get(i).cloned() else { continue };
            if only.is_some_and(|o| !o.contains(&name)) {
                continue;
            }
            let Some(id) = self.t.find(self.scope, &name) else { continue };
            let src = V::place(self.t.syms[id].ty.clone(), crate::codegen::place_of(self.t, id));
            let dst = V::place(fd.ty.clone(), format!("[{p} +{}]:{}", fd.offset, self.ir_ty(&fd.ty)));
            if dst.ty == src.ty {
                self.b.ins(format!("mov {}, {}", dst.op, src.op));
            } else {
                self.assign(&dst, &src, false);
            }
        }
    }

    /// A program-described record just read: its record type, the
    /// record identifying indicator, the fields and their indicators.
    pub fn input_specs(&mut self, f: &File) {
        self.identify(f);
        self.move_fields(f);
    }

    /// The program-described input records of a file.
    pub fn records_of(&self, f: &File) -> Vec<rpgfront::ast::Input> {
        self.inputs.iter().filter(|i| !i.external && i.name == f.name).cloned().collect()
    }

    /// The record type of the record in the buffer by the record
    /// identification codes into `$rt_FILE` (-1 for none), and its
    /// record identifying indicator on (the others off).
    pub fn identify(&mut self, f: &File) {
        self.identify_as(f, true);
    }

    /// [`Self::identify`], setting the record identifying indicator or
    /// not (a cycle's lookahead record).
    pub fn identify_as(&mut self, f: &File, rid: bool) {
        let n = mangle(&f.name);
        let recs = self.records_of(f);
        let p = self.b.reg();
        self.b.ins(format!("{p} = addr $rb_{n}"));
        self.b.ins(format!("store $rt_{n}, -1"));
        for r in recs.iter().filter(|_| rid) {
            if !r.rid.is_empty()
                && r.rid != "**"
                && let Some(x) = self.indicator(&r.rid.clone())
            {
                self.b.ins(format!("store {}, *OFF", x.op));
            }
        }
        let done = self.b.label();
        for (k, r) in recs.iter().enumerate() {
            let next = self.b.label();
            // Alternatives (OR) of conjunctions (AND).
            let hit = self.b.label();
            let groups: Vec<_> = r.codes.iter().filter(|g| !g.is_empty()).cloned().collect();
            if groups.is_empty() {
                self.b.term(format!("br {hit}"));
            }
            for (gi, g) in groups.iter().enumerate() {
                let alt = if gi + 1 < groups.len() { self.b.label() } else { next.clone() };
                for c in g {
                    let byte = self.b.reg();
                    self.b.ins(format!("{byte} = load [{p} +{}]:u8", c.pos.saturating_sub(1)));
                    let want = ccsid::to_ebcdic(&c.ch.to_string()).ok().and_then(|v| v.first().copied()).unwrap_or(0x40);
                    let (mask, val) = match c.part {
                        'Z' => (0xF0, want & 0xF0),
                        'D' => (0x0F, want & 0x0F),
                        _ => (0xFF, want),
                    };
                    let m = self.b.reg();
                    self.b.ins(format!("{m} = and {byte}, {mask}"));
                    let eq = self.b.reg();
                    self.b.ins(format!("{eq} = cmp.{} {m}, {val}", if c.not { "ne" } else { "eq" }));
                    let ok = self.b.label();
                    self.b.term(format!("brif {eq}, {ok}, {alt}"));
                    self.b.place(&ok);
                }
                self.b.term(format!("br {hit}"));
                if gi + 1 < groups.len() {
                    self.b.place(&alt);
                }
            }
            self.b.place(&hit);
            self.b.ins(format!("store $rt_{n}, {k}"));
            if rid
                && !r.rid.is_empty()
                && r.rid != "**"
                && let Some(x) = self.indicator(&r.rid.clone())
            {
                self.b.ins(format!("store {}, *ON", x.op));
            }
            self.b.term(format!("br {done}"));
            if groups.is_empty() {
                // Every record is this type; no later one is reached.
                break;
            }
            self.b.place(&next);
        }
        self.b.place(&done);
    }

    /// The record identifying indicator of the record type in
    /// `$rt_FILE` on.
    pub fn set_rid(&mut self, f: &File) {
        let n = mangle(&f.name);
        let recs = self.records_of(f);
        let rt = self.b.reg();
        self.b.ins(format!("{rt} = load $rt_{n}"));
        for (k, r) in recs.iter().enumerate() {
            if r.rid.is_empty() || r.rid == "**" {
                continue;
            }
            let Some(x) = self.indicator(&r.rid) else { continue };
            let c = self.b.reg();
            self.b.ins(format!("{c} = cmp.eq {rt}, {k}"));
            let on = self.b.label();
            let next = self.b.label();
            self.b.term(format!("brif {c}, {on}, {next}"));
            self.b.place(&on);
            self.b.ins(format!("store {}, *ON", x.op));
            self.b.place(&next);
        }
    }

    /// The fields of the record type `identify` found, from the buffer.
    pub fn move_fields(&mut self, f: &File) {
        let n = mangle(&f.name);
        let recs = self.records_of(f);
        if recs.is_empty() {
            return;
        }
        let p = self.b.reg();
        self.b.ins(format!("{p} = addr $rb_{n}"));
        let rt = self.b.reg();
        self.b.ins(format!("{rt} = load $rt_{n}"));
        let done = self.b.label();
        let labels: Vec<String> = recs.iter().map(|_| self.b.label()).collect();
        let arms: Vec<String> = labels.iter().enumerate().map(|(k, l)| format!("{k}: {l}")).collect();
        self.b.term(format!("switch {rt}, [{}], default {done}", arms.join(", ")));
        for (r, l) in recs.iter().zip(&labels) {
            self.b.place(l);
            for x in &r.fields {
                self.input_field(&p, x);
            }
            self.b.term(format!("br {done}"));
        }
        self.b.place(&done);
    }

    /// One program-described input field from the buffer.
    fn input_field(&mut self, p: &str, x: &rpgfront::ast::InputField) {
        let Some(ty) = input_type(x) else { return };
        let from: u32 = x.from.trim().parse().unwrap_or(1);
        let Some(id) = self.t.find(self.scope, &x.name) else { return };
        self.t.refer(id, x.at.line, true);
        let dst = V::place(self.t.syms[id].ty.clone(), crate::codegen::place_of(self.t, id));
        let src = V::place(ty.clone(), format!("[{p} +{}]:{}", from - 1, self.ir_ty(&ty)));
        if dst.ty == src.ty {
            self.b.ins(format!("mov {}, {}", dst.op, src.op));
        } else {
            self.assign(&dst, &src, false);
        }
        self.field_inds(&dst, &x.inds);
    }

    /// Field indicators: plus, minus, zero or blank.
    pub fn field_inds(&mut self, v: &V, inds: &[String; 3]) {
        if inds.iter().all(String::is_empty) {
            return;
        }
        for (k, name) in inds.iter().enumerate() {
            if name.is_empty() {
                continue;
            }
            let r = if v.ty.is_numeric() {
                let c = ["gt", "lt", "eq"][k];
                let r = self.b.reg();
                let o = self.dec_operand(v);
                self.b.ins(format!("{r} = dec.cmp.{c} {o}, 0"));
                r
            } else if k == 2 {
                let r = self.b.reg();
                self.b.ins(format!("{r} = str.cmp.eq {}, *BLANKS", v.op));
                r
            } else {
                continue;
            };
            self.set_ind(name, &r);
        }
    }

    /// The key of a positioning operation.
    pub fn key_of(&mut self, f: &File, fmt: Option<&RecFmt>, e: &Expr) -> Option<KeyArg> {
        if let Expr::Special { word, index: None, .. } = e {
            match word.as_str() {
                "*START" => return Some(KeyArg::Limit(-1)),
                "*END" => return Some(KeyArg::Limit(-2)),
                "*LOVAL" => return Some(KeyArg::Limit(-3)),
                "*HIVAL" => return Some(KeyArg::Limit(-4)),
                "*KEY" => return Some(KeyArg::None),
                _ => {}
            }
        }
        if let Expr::Fig(fig, _) = e {
            match fig {
                rpgfront::ast::Fig::Loval => return Some(KeyArg::Limit(-3)),
                rpgfront::ast::Fig::Hival => return Some(KeyArg::Limit(-4)),
                _ => {}
            }
        }
        if !f.keyed {
            let v = self.expr(e, Want::default())?;
            let r = self.int_reg(&v)?;
            return Some(KeyArg::Rrn(r));
        }
        let values: Vec<V> = match e {
            Expr::List(items, _) => {
                let mut out = Vec::new();
                for i in items {
                    out.push(self.expr(i, Want::default())?);
                }
                out
            }
            Expr::Bif { name, args, .. } if name == "%KDS" => self.kds(args)?,
            Expr::Ref { name, steps, .. } if steps.is_empty() && self.klists.contains_key(name) => {
                let items = self.klists[name].clone();
                let mut out = Vec::new();
                for i in &items {
                    out.push(self.expr(i, Want::default())?);
                }
                out
            }
            _ => {
                let v = self.expr(e, Want::default())?;
                // A single argument has the first key field's shape.
                if let Some(r) = fmt
                    && let Some(k) = r.fmt.keys.first()
                    && let Some(fd) = r.fmt.fields.iter().find(|x| x.name == *k)
                {
                    let same = match (&v.ty, &fd.ty) {
                        (a, b) if a.is_numeric() && b.is_numeric() => a.shape() == b.shape() || v.kind == Vk::Lit && v.fig.is_some(),
                        (Ty::Unknown, _) => true,
                        (a, b) if a.is_string() && b.is_string() => a.str_len() == b.str_len(),
                        (a, b) => a == b,
                    };
                    if !same {
                        self.err(ids::KEY_MISMATCH, 30, "");
                        return None;
                    }
                }
                vec![v]
            }
        };
        let Some(r) = fmt else {
            // A program-described keyed file: the key is the value's
            // characters, as long as the key.
            if !f.external && f.keylen > 0 && values.len() == 1 {
                let t = self.b.temp(&format!("char({})", f.keylen));
                let v = self.as_chars(&values[0]);
                if matches!(v.ty, Ty::Ds(_)) {
                    self.b.ins(format!("mov {t}, {}.all_", v.op));
                } else {
                    self.assign(&V::place(Ty::Char(f.keylen), t.clone()), &v, false);
                }
                return Some(KeyArg::Key(t));
            }
            self.err(ids::BAD_FILE_OP, 30, f.name.clone());
            return None;
        };
        let mut fields = Vec::new();
        for k in &r.fmt.keys {
            if let Some(fd) = r.fmt.fields.iter().find(|x| x.name == *k) {
                fields.push(fd.ty.clone());
            }
        }
        if values.len() > fields.len() {
            self.err(ids::BAD_FILE_OP, 30, f.name.clone());
            return None;
        }
        let mut layout = String::new();
        let mut at = 0;
        for (i, ty) in fields.iter().take(values.len()).enumerate() {
            if i > 0 {
                layout.push_str(", ");
            }
            let _ = write!(layout, "k{i}: {} @{at}", self.ir_ty(ty));
            at += self.t.size(ty);
        }
        let t = self.b.temp(&format!("struct({}) {{ {layout} }}", at.max(1)));
        for (i, v) in values.iter().enumerate() {
            let dst = V::place(fields[i].clone(), format!("{t}.k{i}"));
            self.assign(&dst, v, false);
        }
        Some(KeyArg::Key(t))
    }

    /// `%KDS(ds[:n])`: the structure's first `n` subfields.
    fn kds(&mut self, args: &[Expr]) -> Option<Vec<V>> {
        let ds = args.first()?;
        let v = self.expr(ds, Want::default())?;
        let Ty::Ds(di) = v.ty else {
            self.err(ids::NOT_A_DS, 30, "%KDS");
            return None;
        };
        let n = match args.get(1) {
            Some(Expr::Lit(rpgfront::ast::Lit::Num(x), _)) => x.parse().unwrap_or(usize::MAX),
            _ => usize::MAX,
        };
        let subs: Vec<usize> = self.t.ds[di].subfields.clone();
        let mut out = Vec::new();
        for s in subs.into_iter().take(n) {
            let sym = self.t.syms[s].clone();
            out.push(V::place(sym.ty.clone(), format!("{}.{}", v.op, mangle(&sym.name))));
        }
        Some(out)
    }

    fn open_op(&mut self, op: &Op) {
        let Some(e) = op.f2.first().cloned() else { return };
        let Some((f, _)) = self.db_target(&e) else { return };
        let n = mangle(&f.name);
        let st = self.b.temp("i32");
        // Already open: 01215.
        if matches!(f.dev, Dev::Disk | Dev::Workstn) {
            let fb = self.b.reg();
            self.b.ins(format!("{fb} = dbfb {n}"));
            let a = self.b.reg();
            self.b.ins(format!("{a} = and {fb}, 8"));
            let open = self.b.reg();
            self.b.ins(format!("{open} = cmp.ne {a}, 0"));
            let yes = self.b.label();
            let no = self.b.label();
            let done = self.b.label();
            self.b.term(format!("brif {open}, {yes}, {no}"));
            self.b.place(&yes);
            self.b.ins(format!("store {st}, 1215"));
            self.b.term(format!("br {done}"));
            self.b.place(&no);
            let r = self.db_open(&f);
            self.b.ins(format!("store {st}, {r}"));
            self.b.place(&done);
        } else {
            let r = self.db_open(&f);
            self.b.ins(format!("store {st}, {r}"));
        }
        let s = self.b.reg();
        self.b.ins(format!("{s} = load {st}"));
        if matches!(f.dev, Dev::Disk | Dev::Workstn | Dev::Special) {
            self.db_result(op, "OPEN", &n, &s, &KeyArg::None);
            self.infds_opcode(&f, "OPEN", true);
        }
    }

    /// `POST`, `ACQ`, `REL` and `NEXT`: a closed file is 01211; an open
    /// one has one requester, so there is nothing to acquire, release
    /// or switch to, and `POST` has the feedback already.
    pub fn device_op(&mut self, op: &Op, base: &str) {
        let Some(e) = op.f2.first().cloned() else {
            self.err(ids::FILE_NOT_DEFINED, 30, op.code.clone());
            return;
        };
        let Some((f, _)) = self.db_target(&e) else { return };
        if base != "POST" && f.dev != Dev::Workstn {
            self.err(ids::OP_NOT_FOR_FILE, 30, f.name.clone());
            return;
        }
        for x in op.f1.clone() {
            let _ = self.expr(&x, Want::default());
        }
        let n = mangle(&f.name);
        let open = self.b.reg();
        if matches!(f.dev, Dev::Disk | Dev::Workstn) {
            let fb = self.b.reg();
            self.b.ins(format!("{fb} = dbfb {n}"));
            let a = self.b.reg();
            self.b.ins(format!("{a} = and {fb}, 8"));
            self.b.ins(format!("{open} = cmp.ne {a}, 0"));
        } else {
            let po = self.open_flag(&f);
            self.b.ins(format!("{open} = load {po}"));
        }
        let (ok, closed) = (self.b.reg(), self.b.reg());
        self.b.ins(format!("{ok} = const.i32 0"));
        self.b.ins(format!("{closed} = const.i32 1211"));
        let st = self.b.reg();
        self.b.ins(format!("{st} = select {open}, {ok}, {closed}"));
        if f.dev == Dev::Printer {
            // A printer keeps no status of its own.
            if op.ext.contains('E') || !op.resind[1].is_empty() {
                let err = self.b.reg();
                self.b.ins(format!("{err} = cmp.ge {st}, 1000"));
                self.b.ins(format!("store $errind, {err}"));
                self.b.ins(format!("dec.mov $status, {st}"));
                let ind = op.resind[1].clone();
                self.set_ind(&ind, &err);
            }
        } else {
            self.db_result(op, base, &n, &st, &KeyArg::None);
        }
        self.infds_opcode(&f, base, true);
    }

    fn close_op(&mut self, op: &Op) {
        let all = matches!(op.f2.first(), Some(Expr::Special { word, .. }) if word == "*ALL");
        let files: Vec<File> = if all {
            self.files.iter().filter(|f| f.scope == 0 && !f.template).cloned().collect()
        } else {
            let Some(e) = op.f2.first().cloned() else { return };
            match self.db_target(&e) {
                Some((f, _)) => vec![f],
                None => return,
            }
        };
        for f in files {
            let n = mangle(&f.name);
            if matches!(f.dev, Dev::Disk | Dev::Workstn) {
                let fb = self.b.reg();
                self.b.ins(format!("{fb} = dbfb {n}"));
                let a = self.b.reg();
                self.b.ins(format!("{a} = and {fb}, 8"));
                let open = self.b.reg();
                self.b.ins(format!("{open} = cmp.ne {a}, 0"));
                let yes = self.b.label();
                let done = self.b.label();
                self.b.term(format!("brif {open}, {yes}, {done}"));
                self.b.place(&yes);
                let r = self.b.reg();
                self.b.ins(format!("{r} = dbclose {n}"));
                self.b.place(&done);
            } else if f.dev == Dev::Special {
                self.special_close(&f);
            } else {
                let r = self.b.reg();
                self.b.ins(format!("{r} = dbclose {n}"));
                let po = self.open_flag(&f);
                self.b.ins(format!("store {po}, *OFF"));
            }
        }
        if op.ext.contains('E') {
            self.b.ins("store $errind, *OFF");
            self.b.ins("mov $status, *ZEROS");
        }
    }

    /// `%EOF %FOUND %EQUAL %OPEN %ERROR %STATUS`; `None` for other names.
    pub fn db_bif(&mut self, name: &str, args: &[Expr]) -> Option<Option<V>> {
        let flag = match name {
            "%EOF" => "eof",
            "%FOUND" => "fnd",
            "%EQUAL" => "eq",
            "%ERROR" => return Some(Some(V::place(Ty::Ind, "$errind"))),
            "%STATUS" => {
                return Some(match args.first() {
                    None => Some(V::place(Ty::Zoned(5, 0), "$status")),
                    Some(e) => {
                        let e = e.clone();
                        self.db_target(&e).map(|(f, _)| V::place(Ty::Zoned(5, 0), format!("$st_{}", mangle(&f.name))))
                    }
                });
            }
            "%OPEN" => {
                let e = args.first()?.clone();
                let Some((f, _)) = self.db_target(&e) else {
                    return Some(None);
                };
                if !matches!(f.dev, Dev::Disk | Dev::Workstn) {
                    let po = self.open_flag(&f);
                    return Some(Some(V::place(Ty::Ind, po)));
                }
                let fb = self.b.reg();
                self.b.ins(format!("{fb} = dbfb {}", mangle(&f.name)));
                let a = self.b.reg();
                self.b.ins(format!("{a} = and {fb}, 8"));
                let c = self.b.reg();
                self.b.ins(format!("{c} = cmp.ne {a}, 0"));
                return Some(Some(V::reg(Ty::Ind, c)));
            }
            _ => return None,
        };
        Some(match args.first() {
            None => Some(V::place(Ty::Ind, format!("${flag}_"))),
            Some(e) => {
                let e = e.clone();
                let r = self.db_target(&e).map(|(f, _)| V::place(Ty::Ind, format!("${flag}_{}", mangle(&f.name))));
                if r.is_none() && self.diags.last().is_some_and(|d| d.msgid == ids::NOT_DEFINED) {
                    self.err(ids::BIF_NAME_UNDEFINED, 30, name.to_string());
                }
                r
            }
        })
    }

    /// Program-described output to a disk file: the record built field
    /// by field at its end positions, then added (or updated, deleted).
    pub fn disk_output(&mut self, o: &rpgfront::ast::Output, f: &File) {
        let n = mangle(&f.name);
        let len = f.reclen.max(1);
        let line = format!("$rb_{n}");
        let add = o.fetch.contains("ADD") || !f.update;
        let del = o.fetch.contains("DEL");
        if add {
            self.b.ins(format!("mov {line}, *BLANKS"));
        }
        let p = self.b.reg();
        self.b.ins(format!("{p} = addr {line}"));
        let mut prev_end = 0;
        for fld in &o.fields {
            self.at = fld.at;
            let skip = if fld.cond.is_empty() {
                None
            } else {
                let Some(r) = self.conditioning(&fld.cond) else { continue };
                let run = self.b.label();
                let skip = self.b.label();
                self.b.term(format!("brif {r}, {run}, {skip}"));
                self.b.place(&run);
                Some(skip)
            };
            let v = if fld.field.is_empty() {
                match &fld.constant {
                    Some(c) => self.expr(c, Want::default()),
                    None => None,
                }
            } else {
                let e = Expr::Ref {
                    name: fld.field.clone(),
                    steps: Vec::new(),
                    at: fld.at,
                };
                self.expr(&e, Want::default())
            };
            let Some(v) = v else { continue };
            let v = self.as_chars(&v);
            // The field's form in the record: by data format, or as is.
            let ty = match (fld.format.as_str(), &v.ty) {
                ("P", t) => match t.shape() {
                    Some((d, s)) => Ty::Packed(d, s),
                    None => v.ty.clone(),
                },
                ("B", t) => match t.shape() {
                    Some((d, s)) => Ty::Bin(d, s),
                    None => v.ty.clone(),
                },
                (_, Ty::Packed(d, s) | Ty::Bin(d, s)) => Ty::Zoned(*d, *s),
                (_, t) => t.clone(),
            };
            let width = self.t.size(&ty);
            let end = match fld.end.trim() {
                "" => prev_end + width,
                e if e.starts_with('+') => prev_end + e[1..].trim().parse::<u32>().unwrap_or(0) + width,
                e => e.parse::<u32>().unwrap_or(prev_end + width),
            };
            if end > len || width > end {
                self.err(ids::INDEX_RANGE, 30, fld.end.clone());
                continue;
            }
            let dst = V::place(ty.clone(), format!("[{p} +{}]:{}", end - width, self.ir_ty(&ty)));
            self.assign(&dst, &v, false);
            if fld.blank && v.kind == Vk::Place {
                let lit = if v.ty.is_numeric() { "*ZEROS" } else { "*BLANKS" };
                self.b.ins(format!("mov {}, {lit}", v.op));
            }
            prev_end = end;
            if let Some(s) = skip {
                self.b.place(&s);
            }
        }
        let op = if del {
            "DELETE"
        } else if add {
            "WRITE"
        } else {
            "UPDATE"
        };
        let rec = (!del).then(|| line.clone());
        let r = self.dbio(&n, op, rec, &KeyArg::None, false);
        let st = self.b.reg();
        self.b.ins(format!("{st} = add {r}, 0"));
    }
}

/// The file notes of the listing: record formats no operation names
/// (`RNF7066`), and input files RPG reads in blocks (`RNF7086`): input
/// only, without `BLOCK(*NO)`, read only by `READ`.
pub fn file_notes(g: &mut Gen<'_>, m: &rpgfront::ast::Module) {
    let ops: Vec<&rpgfront::ast::Op> = m.calcs.iter().chain(m.procs.iter().flat_map(|p| p.calcs.iter())).map(|s| &s.op).collect();
    for (i, r) in g.t.recfmts.clone().iter().enumerate() {
        let named_out = g.outputs.iter().any(|o| o.name == r.name);
        let cycle = g.files.get(r.file).is_some_and(|f| matches!(f.designation.as_str(), "P" | "S"));
        if !g.used_formats.contains(&i) && !named_out && !cycle {
            let line = g.files.get(r.file).map_or(0, |f| f.def_line);
            g.diags.push(rpgfront::Diag::new(ids::FORMAT_UNUSED, 0, rpgfront::ast::Pos { line, col: 7 }).arg(r.name.clone()));
        }
    }
    for f in g.files.iter().filter(|f| f.ext.as_ref().is_some_and(|e| e.indara)) {
        g.diags.push(rpgfront::Diag::new(ids::INDARA, 0, rpgfront::ast::Pos { line: f.def_line, col: 7 }).arg(f.name.clone()));
    }
    // A file read only, or written only, is blocked. A file whose
    // description was not found is ignored.
    for f in g.files.iter().filter(|f| {
        f.dev == Dev::Disk && !f.template && f.input != f.output && !f.update && !f.noblock && (!f.external || f.ext.is_some())
    }) {
        let allowed: &[&str] = if f.input { &["READ", "OPEN", "CLOSE", "FEOD", "FORCE"] } else { &["WRITE", "OPEN", "CLOSE", "FEOD"] };
        let names: Vec<String> = std::iter::once(f.name.clone())
            .chain(g.t.recfmts.iter().filter(|r| r.file_name == f.name).map(|r| r.name.clone()))
            .collect();
        let only_read = ops.iter().all(|op| {
            let target = op.f2.first().and_then(|e| match e {
                Expr::Ref { name, .. } => Some(name.clone()),
                _ => None,
            });
            match target {
                Some(t) if names.contains(&t) => allowed.contains(&op.code.as_str()),
                _ => true,
            }
        });
        if only_read {
            g.diags.push(rpgfront::Diag::new(ids::BLOCKED, 0, rpgfront::ast::Pos { line: f.def_line, col: 7 }).arg(f.name.clone()));
        }
    }
}

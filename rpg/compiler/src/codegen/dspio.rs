// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Display file operations. A `WORKSTN` file has one record buffer for
//! both directions: an output moves the format's shown fields and its
//! option indicators into it (`*IN`, the `INDDS` structure, or the
//! separate indicator area with `INDARA`), an input moves the fields
//! read and the response indicators back. `EXFMT` writes then reads,
//! `WRITE` of a subfile record puts it at the `SFILE` number, `CHAIN`
//! and `READC` read subfile records, `UPDATE` replaces the one last
//! read. After a read the attention identifier sets `*INKA`-`*INKY`;
//! `READC` leaves the record's number in the `SFILE` field.

use rpgfront::ast::{Expr, Op};

use crate::codegen::Gen;
use crate::codegen::expr::{V, Want};
use crate::diag::ids;
use crate::extfiles::RecFmt;
use crate::files::File;
use crate::names::mangle;
use crate::syms::Kind;
use crate::types::Ty;

/// The function key indicators, F1 to F24.
const FKEYS: [&str; 24] = [
    "inka", "inkb", "inkc", "inkd", "inke", "inkf", "inkg", "inkh", "inki", "inkj", "inkk", "inkl",
    "inkm", "inkn", "inkp", "inkq", "inkr", "inks", "inkt", "inku", "inkv", "inkw", "inkx", "inky",
];

/// The attention identifier of function key `n` (1-24).
fn aid_of(n: usize) -> i32 {
    let n = i32::try_from(n).unwrap_or(0);
    if n <= 12 { 0x30 + n } else { 0xB0 + n - 12 }
}

impl Gen<'_> {
    /// One display file operation statement.
    pub fn dsp_op(&mut self, op: &Op, base: &str, f: &File, fmt: Option<RecFmt>, target: &Expr) {
        let n = mangle(&f.name);
        let by_file = matches!(target, Expr::Ref { name, steps, .. } if *name == f.name && steps.is_empty());
        let rec = format!("$rb_{n}");
        // The format: named, or (a READ by file name) the file's.
        let fmt = if by_file && base == "READ" { None } else { fmt };
        if fmt.is_none() && base != "READ" {
            self.err(ids::BAD_FILE_OP, 30, f.name.clone());
            return;
        }
        let subfile = fmt.as_ref().and_then(|r| r.fmt.dsp.as_ref()).is_some_and(|d| d.subfile);
        let fname = fmt.as_ref().map_or_else(|| mangle(&f.obj.1), |r| mangle(&r.fmt.name));
        let inds = self.inds_area(f);
        let status = match base {
            "WRITE" | "EXFMT" | "UPDATE" => {
                let Some(r) = fmt.clone() else { return };
                self.dsp_out(f, &r);
                let rrn = if base == "WRITE" && subfile {
                    match self.sfile_field(f, &r) {
                        Some(v) => self.int_reg(&v),
                        None => {
                            self.err(ids::BAD_FILE_OP, 30, r.name.clone());
                            return;
                        }
                    }
                } else {
                    None
                };
                if base != "WRITE" && subfile != (base == "UPDATE") {
                    self.err(ids::BAD_FILE_OP, 30, r.name.clone());
                    return;
                }
                self.dspio(&n, base, &fname, &rec, rrn.as_deref(), inds.as_deref())
            }
            "READ" | "READC" => {
                if base == "READC" && !subfile {
                    self.err(ids::BAD_FILE_OP, 30, fname);
                    return;
                }
                self.dspio(&n, base, &fname, &rec, None, inds.as_deref())
            }
            "CHAIN" => {
                if !subfile {
                    self.err(ids::BAD_FILE_OP, 30, fname);
                    return;
                }
                let Some(k) = op.f1.first().cloned() else {
                    self.err(ids::BAD_FILE_OP, 30, fname);
                    return;
                };
                let Some(v) = self.expr(&k, Want::default()) else { return };
                let Some(r) = self.int_reg(&v) else { return };
                self.dspio(&n, base, &fname, &rec, Some(&r), inds.as_deref())
            }
            _ => {
                self.err(ids::BAD_FILE_OP, 30, f.name.clone());
                return;
            }
        };
        // The feedback: end of file for READC, found for CHAIN.
        let fb = self.b.reg();
        self.b.ins(format!("{fb} = dbfb {n}"));
        let flag = |g: &mut Self, m: i32| {
            let a = g.b.reg();
            g.b.ins(format!("{a} = and {fb}, {m}"));
            let c = g.b.reg();
            g.b.ins(format!("{c} = cmp.ne {a}, 0"));
            c
        };
        match base {
            "READC" => {
                let e = flag(self, 1);
                self.b.ins(format!("store $fcb_{n}.eof, {e}"));
                self.b.ins(format!("store $eof_, {e}"));
            }
            "CHAIN" => {
                let x = flag(self, 2);
                self.b.ins(format!("store $fcb_{n}.fnd, {x}"));
                self.b.ins(format!("store $fnd_, {x}"));
            }
            _ => {}
        }
        if matches!(base, "READ" | "EXFMT" | "READC" | "CHAIN") {
            // A record came back: its fields and indicators.
            let got = self.b.label();
            let skip = self.b.label();
            let ok = self.b.reg();
            if matches!(base, "READC" | "CHAIN") {
                let x = flag(self, 2);
                let z = self.b.reg();
                self.b.ins(format!("{z} = cmp.eq {status}, 0"));
                self.b.ins(format!("{ok} = select {x}, {z}, {x}"));
            } else {
                self.b.ins(format!("{ok} = cmp.eq {status}, 0"));
            }
            self.b.term(format!("brif {ok}, {got}, {skip}"));
            self.b.place(&got);
            match &fmt {
                Some(r) => self.dsp_in(f, r),
                None => self.dsp_in_any(f),
            }
            if matches!(base, "READ" | "EXFMT") {
                self.fkeys(&fb);
            }
            if base == "READC"
                && let Some(r) = &fmt
                && let Some(v) = self.sfile_field(f, r)
            {
                let a = self.b.reg();
                self.b.ins(format!("{a} = shr {fb}, 16"));
                let src = V::reg(Ty::Int(10), a);
                self.assign(&v, &src, false);
            }
            self.b.place(&skip);
        }
        let as_db = match base {
            "READC" => "READ",
            "CHAIN" => "CHAIN",
            _ => "WRITE",
        };
        self.dsp_result(op, as_db, &n, &status);
        self.infds_opcode(f, base, by_file);
    }

    /// The `dspio` instruction; its status register.
    fn dspio(&mut self, n: &str, op: &str, fmt: &str, rec: &str, rrn: Option<&str>, inds: Option<&str>) -> String {
        let r = self.b.reg();
        let mut s = format!("{r} = dspio {n} {op} fmt({fmt}) rec({rec})");
        if let Some(x) = rrn {
            s.push_str(&format!(" rrn({x})"));
        }
        if let Some(p) = inds {
            s.push_str(&format!(" inds({p})"));
        }
        self.b.ins(s);
        r
    }

    /// `%ERROR`, `%STATUS` and the resulting indicators.
    fn dsp_result(&mut self, op: &Op, base: &str, n: &str, status: &str) {
        let err = self.b.reg();
        self.b.ins(format!("{err} = cmp.ge {status}, 1000"));
        self.b.ins(format!("dec.mov $fcb_{n}.st, {status}"));
        if op.ext.contains('E') || !op.resind[1].is_empty() {
            self.b.ins(format!("store $errind, {err}"));
            let st = self.b.reg();
            self.b.ins(format!("{st} = select {err}, {status}, 0"));
            self.b.ins(format!("dec.mov $status, {st}"));
        }
        let name = op.resind[1].clone();
        if !name.is_empty() {
            self.set_ind(&name, &err);
        }
        match base {
            "READ" => {
                let name = op.resind[2].clone();
                if !name.is_empty() {
                    let e = self.b.reg();
                    self.b.ins(format!("{e} = load $fcb_{n}.eof"));
                    self.set_ind(&name, &e);
                }
            }
            "CHAIN" => {
                let name = op.resind[0].clone();
                if !name.is_empty() {
                    let x = self.b.reg();
                    self.b.ins(format!("{x} = load $fcb_{n}.fnd"));
                    let nf = self.b.reg();
                    self.b.ins(format!("{nf} = cmp.eq {x}, *OFF"));
                    self.set_ind(&name, &nf);
                }
            }
            _ => {}
        }
    }

    /// The separate indicator area an `INDARA` file's operations pass:
    /// the `INDDS` structure, or `*IN`.
    fn inds_area(&mut self, f: &File) -> Option<String> {
        if !f.ext.as_ref().is_some_and(|e| e.indara) {
            return None;
        }
        match self.indds_place(f) {
            Some(p) => Some(p),
            None => Some("$in".into()),
        }
    }

    /// The `INDDS` structure's place.
    fn indds_place(&mut self, f: &File) -> Option<String> {
        let name = f.indds.clone()?;
        let id = self.t.find(self.scope, &name)?;
        if !matches!(self.t.syms[id].kind, Some(Kind::Ds(_))) {
            return None;
        }
        Some(crate::codegen::place_of(self.t, id))
    }

    /// Indicator `k` (1-99) as the file uses it: in `INDDS`, or `*INkk`.
    fn file_ind(&mut self, f: &File, k: u8) -> String {
        if let Some(p) = self.indds_place(f) {
            let a = self.b.reg();
            self.b.ins(format!("{a} = addr {p}"));
            return format!("[{a} +{}]:ind", u32::from(k).saturating_sub(1));
        }
        format!("$in.x[{}]", u32::from(k).saturating_sub(1))
    }

    /// The `SFILE` field of a subfile record format.
    fn sfile_field(&mut self, f: &File, r: &RecFmt) -> Option<V> {
        let (_, field) = f.sfiles.iter().find(|(fm, _)| *fm == r.name || *fm == r.fmt.name)?.clone();
        let e = Expr::Ref {
            name: field,
            steps: Vec::new(),
            at: self.at,
        };
        self.expr(&e, Want::default())
    }

    /// The program's fields and option indicators into the buffer.
    fn dsp_out(&mut self, f: &File, r: &RecFmt) {
        self.used_mark(r);
        let n = mangle(&f.name);
        let Some(d) = r.fmt.dsp.clone() else { return };
        let p = self.b.reg();
        self.b.ins(format!("{p} = addr $rb_{n}"));
        if !f.ext.as_ref().is_some_and(|e| e.indara) {
            for (k, ind) in d.option_inds.iter().enumerate() {
                let src = self.file_ind(f, *ind);
                self.b.ins(format!("mov [{p} +{k}]:ind, {src}"));
            }
        }
        for (i, fd) in r.fmt.fields.iter().enumerate() {
            let Some((Some(off), _)) = d.offsets.get(i).copied() else { continue };
            let Some(name) = r.names.get(i).cloned() else { continue };
            let Some(id) = self.t.find(self.scope, &name) else { continue };
            let src = V::place(self.t.syms[id].ty.clone(), crate::codegen::place_of(self.t, id));
            let dst = V::place(fd.ty.clone(), format!("[{p} +{off}]:{}", self.ir_ty(&fd.ty)));
            if dst.ty == src.ty {
                self.b.ins(format!("mov {}, {}", dst.op, src.op));
            } else {
                self.assign(&dst, &src, false);
            }
        }
    }

    /// The buffer's fields and response indicators into the program's.
    fn dsp_in(&mut self, f: &File, r: &RecFmt) {
        self.used_mark(r);
        let n = mangle(&f.name);
        let Some(d) = r.fmt.dsp.clone() else { return };
        let p = self.b.reg();
        self.b.ins(format!("{p} = addr $rb_{n}"));
        if !f.ext.as_ref().is_some_and(|e| e.indara) {
            for (k, ind) in d.response_inds.iter().enumerate() {
                let dst = self.file_ind(f, *ind);
                self.b.ins(format!("mov {dst}, [{p} +{k}]:ind"));
            }
        }
        for (i, fd) in r.fmt.fields.iter().enumerate() {
            let Some((_, Some(off))) = d.offsets.get(i).copied() else { continue };
            let Some(name) = r.names.get(i).cloned() else { continue };
            let Some(id) = self.t.find(self.scope, &name) else { continue };
            self.t.refer(id, self.at.line, true);
            let dst = V::place(self.t.syms[id].ty.clone(), crate::codegen::place_of(self.t, id));
            let src = V::place(fd.ty.clone(), format!("[{p} +{off}]:{}", self.ir_ty(&fd.ty)));
            if dst.ty == src.ty {
                self.b.ins(format!("mov {}, {}", dst.op, src.op));
            } else {
                self.assign(&dst, &src, false);
            }
        }
    }

    /// A read by file name: the fields of the format the information
    /// data structure names.
    fn dsp_in_any(&mut self, f: &File) {
        let formats: Vec<RecFmt> = self
            .t
            .recfmts
            .iter()
            .filter(|r| r.file_name == f.name && r.fmt.dsp.as_ref().is_some_and(|d| !d.subfile))
            .cloned()
            .collect();
        let Some(infds) = self.dsp_infds(f) else {
            if let Some(r) = formats.first() {
                self.dsp_in(f, r);
            }
            return;
        };
        let a = self.b.reg();
        self.b.ins(format!("{a} = addr {infds}"));
        let done = self.b.label();
        for r in &formats {
            let next = self.b.label();
            let hit = self.b.label();
            let c = self.b.reg();
            let name = crate::codegen::emit::quote(&format!("{:<8}", r.fmt.name));
            self.b.ins(format!("{c} = str.cmp.eq [{a} +37]:char(8), {name}"));
            self.b.term(format!("brif {c}, {hit}, {next}"));
            self.b.place(&hit);
            self.dsp_in(f, r);
            self.b.term(format!("br {done}"));
            self.b.place(&next);
        }
        self.b.place(&done);
    }

    /// The information data structure a display file's operations keep:
    /// the program's `INFDS` when it has one, else the compiler's.
    pub fn dsp_infds(&mut self, f: &File) -> Option<String> {
        if let Some(id) = f.infds.as_ref().and_then(|n| self.t.find(self.scope, n)) {
            if let Some(Kind::Ds(di)) = self.t.syms[id].kind.clone()
                && self.t.ds_len(di) >= 45
            {
                return Some(crate::codegen::place_of(self.t, id));
            }
            return None;
        }
        Some(format!("$fi_{}", mangle(&f.name)))
    }

    /// `*INKA`-`*INKY` from the attention identifier in the feedback.
    fn fkeys(&mut self, fb: &str) {
        let a = self.b.reg();
        self.b.ins(format!("{a} = shr {fb}, 8"));
        let aid = self.b.reg();
        self.b.ins(format!("{aid} = and {a}, 255"));
        for (k, name) in FKEYS.iter().enumerate() {
            let c = self.b.reg();
            self.b.ins(format!("{c} = cmp.eq {aid}, {}", aid_of(k + 1)));
            self.b.ins(format!("store ${name}, {c}"));
        }
    }

    /// `WRITE` of an externally described printer file's format: the
    /// fields and option indicators into the buffer, the line printed;
    /// at the overflow line the `OFLIND` indicator goes on.
    pub fn prt_write(&mut self, f: &File, fmt: Option<RecFmt>) {
        let Some(r) = fmt else {
            self.err(ids::BAD_FILE_OP, 30, f.name.clone());
            return;
        };
        self.dsp_out(f, &r);
        let n = mangle(&f.name);
        let st = self.b.reg();
        self.b.ins(format!("{st} = prtio {n} WRITE fmt({}) rec($rb_{n})", mangle(&r.fmt.name)));
        if let Some(ind) = f.oflind.clone() {
            let ov = self.overflow_crossed(&f.name, &st);
            let on = self.b.label();
            let cont = self.b.label();
            self.b.term(format!("brif {ov}, {on}, {cont}"));
            self.b.place(&on);
            self.oflind_on(&ind);
            self.b.place(&cont);
        }
    }

    fn used_mark(&mut self, r: &RecFmt) {
        if let Some(i) = self.t.recfmts.iter().position(|x| x.file_name == r.file_name && x.name == r.name) {
            self.used_formats.insert(i);
        }
    }
}

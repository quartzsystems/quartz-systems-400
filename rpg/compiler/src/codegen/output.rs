// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Output specifications of program-described printer files: each
//! record whose type (`H D T E`, the `EXCEPT` name) and conditioning
//! match is built in a line buffer, field by field, right-aligned at
//! its end position (`+n` after the previous field), edited by code or
//! word, and printed with the record's space and skip entries (one
//! line after when none is given). `B` blanks a field after output.

use rpgfront::ast::{Expr, Lit, Output, OutputField};

use crate::codegen::Gen;
use crate::codegen::emit::quote;
use crate::codegen::expr::{V, Vk, Want};
use crate::diag::ids;
use crate::files::Dev;
use crate::names::mangle;
use crate::types::Ty;

/// Which records an output step prints.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step<'a> {
    /// Heading and detail records.
    HeadingDetail,
    /// Total records.
    Total,
    /// Exception records with this name (empty: unnamed ones).
    Except(&'a str),
}

impl Gen<'_> {
    /// Print the output records a step selects.
    pub fn output(&mut self, outputs: &[Output], step: Step<'_>) {
        for o in outputs {
            let hit = match step {
                Step::HeadingDetail => o.kind == "H" || o.kind == "D",
                Step::Total => o.kind == "T",
                Step::Except(n) => o.kind == "E" && o.except == n,
            };
            if hit {
                self.output_record(o);
            }
        }
    }

    fn output_record(&mut self, o: &Output) {
        let Some(f) = self.files.iter().find(|f| f.name == o.name).cloned() else {
            self.err(ids::FILE_NOT_DEFINED, 30, o.name.clone());
            return;
        };
        if f.dev != Dev::Printer || f.external {
            self.err(ids::UNSUPPORTED, 30, "output");
            return;
        }
        self.at = o.at;
        let skip = self.b.label();
        if !o.cond.is_empty() {
            let r = self.conditioning(&o.cond);
            let run = self.b.label();
            if let Some(r) = r {
                self.b.term(format!("brif {r}, {run}, {skip}"));
            }
            self.b.place(&run);
        }
        let len = if f.reclen == 0 { 132 } else { f.reclen };
        let line = self.b.temp(&format!("char({len})"));
        self.b.ins(format!("mov {line}, *BLANKS"));
        let mut prev_end: u32 = 0;
        for fld in &o.fields {
            if let Some(end) = self.output_field(fld, &line, len, prev_end) {
                prev_end = end;
            }
        }
        let n = |s: &str| s.trim().parse::<u16>().unwrap_or(0);
        let (sb, sa, kb, ka) = (n(&o.space[0]), n(&o.space[1]), n(&o.space[2]), n(&o.space[3]));
        let sa = if o.space.iter().all(|s| s.trim().is_empty()) { 1 } else { sa };
        let st = self.b.reg();
        self.b.ins(format!(
            "{st} = prtio {} WRITE fmt({}) rec({line}) ctl({sb}, {sa}, {kb}, {ka})",
            mangle(&f.name),
            mangle(&f.name)
        ));
        if let Some(ind) = &f.oflind {
            let ov = self.b.reg();
            self.b.ins(format!("{ov} = cmp.eq {st}, 1"));
            let on = self.b.label();
            let cont = self.b.label();
            self.b.term(format!("brif {ov}, {on}, {cont}"));
            self.b.place(&on);
            let ind = ind.trim_start_matches("*IN").to_string();
            if let Some(x) = self.indicator(&ind) {
                self.b.ins(format!("store {}, *ON", x.op));
            }
            self.b.place(&cont);
        }
        self.b.place(&skip);
    }

    /// One field into the line; its end position.
    fn output_field(&mut self, fld: &OutputField, line: &str, len: u32, prev_end: u32) -> Option<u32> {
        self.at = fld.at;
        let skip = if fld.cond.is_empty() {
            None
        } else {
            let r = self.conditioning(&fld.cond)?;
            let run = self.b.label();
            let skip = self.b.label();
            self.b.term(format!("brif {r}, {run}, {skip}"));
            self.b.place(&run);
            Some(skip)
        };
        // The text and its width.
        let (text, width): (String, u32) = if fld.field.is_empty() {
            match &fld.constant {
                Some(Expr::Lit(Lit::Char(s), _)) => (quote(s), u32::try_from(s.chars().count()).unwrap_or(0)),
                _ => {
                    self.err(ids::BAD_KEYWORD_VALUE, 30, "constant");
                    return None;
                }
            }
        } else {
            let name = fld.field.clone();
            let e = Expr::Ref {
                name: name.clone(),
                steps: Vec::new(),
                at: fld.at,
            };
            let v = if name == "PAGE" || name.starts_with("PAGE") && name.len() == 5 {
                self.page_field(&name)
            } else {
                self.expr(&e, Want::default())?
            };
            let v = self.as_chars(&v);
            let edited = self.edited(&v, fld)?;
            if fld.blank {
                self.blank_after(&v);
            }
            edited
        };
        let end = match fld.end.trim() {
            "" => prev_end + width,
            e if e.starts_with('+') => prev_end + e[1..].trim().parse::<u32>().unwrap_or(0) + width,
            e => e.parse::<u32>().unwrap_or(prev_end + width),
        };
        if end > len || width > end {
            self.err(ids::INDEX_RANGE, 30, fld.end.clone());
            return None;
        }
        let start = end - width;
        if width > 0 {
            let p = self.b.reg();
            self.b.ins(format!("{p} = addr {line}"));
            self.b.ins(format!("mov [{p} +{start}]:char({width}), {text}"));
        }
        if let Some(s) = skip {
            self.b.place(&s);
        }
        Some(end)
    }

    /// A field's printed form: characters as they are; numbers by edit
    /// code, edit word, or as zoned digits.
    fn edited(&mut self, v: &V, fld: &OutputField) -> Option<(String, u32)> {
        if v.ty.is_string() {
            if v.ty.is_varying() {
                let n = v.ty.str_len().unwrap_or(0);
                let t = self.b.temp(&format!("char({n})"));
                self.b.ins(format!("mov {t}, {}", v.op));
                return Some((t, n));
            }
            return Some((v.op.clone(), v.ty.str_len().unwrap_or(0)));
        }
        if v.ty == Ty::Ind {
            let c = self.ind_as_char(v);
            return Some((c.op, 1));
        }
        if v.ty.is_datetime() {
            let n = self.t.size(&v.ty);
            return Some((v.op.clone(), n));
        }
        if !v.ty.is_numeric() {
            self.err(ids::TYPE_MISMATCH, 30, fld.field.clone());
            return None;
        }
        if let Some(Expr::Lit(Lit::Char(word), _)) = &fld.constant {
            let src = self.num_place(v);
            let n = u32::try_from(word.chars().count()).unwrap_or(1).max(1);
            let t = self.b.temp(&format!("char({n})"));
            let desc = self.desc_of(&src);
            self.rt_proc(
                "_QRNX_EDITW",
                "($dst: char(1) *VARSIZE, byval $dlen: i32, $src: bytes(1) *VARSIZE, byval $desc: i64, const $word: char(1) *VARSIZE, byval $wlen: i32)",
            );
            self.b.ins(format!("callb _QRNX_EDITW({t}, {n}, {}, {desc}, {}, {n})", src.op, quote(word)));
            return Some((t, n));
        }
        if !fld.edit.is_empty() {
            let code = fld.edit.chars().next()?;
            let src = self.num_place(v);
            let (d, s) = src.ty.shape().unwrap_or((30, 0));
            let shape = qdecimal::Shape::new(d.max(1), s).ok()?;
            let width = qdecimal::code_width(shape, code, &qdecimal::EditOpts::default()).ok()?;
            let width = u32::try_from(width).unwrap_or(0);
            let t = self.b.temp(&format!("varchar({})", width.max(1)));
            let desc = self.desc_of(&src);
            self.rt_proc(
                "_QRNX_EDITC",
                "($dst: varchar(1) *VARSIZE, $src: bytes(1) *VARSIZE, byval $desc: i64, byval $code: i32, byval $fill: i32, const $cur: char(1) *VARSIZE)",
            );
            self.b.ins(format!(
                "callb _QRNX_EDITC({t}, {}, {desc}, {}, 0, {})",
                src.op,
                u32::from(code),
                quote(" ")
            ));
            let c = self.b.temp(&format!("char({})", width.max(1)));
            self.b.ins(format!("mov {c}, {t}"));
            return Some((c, width));
        }
        // No edit: the zoned digits, the sign in the last zone.
        let (d, s) = match &v.ty {
            Ty::Int(n) | Ty::Uns(n) => (*n, 0),
            Ty::Float(_) => (15, 0),
            t => t.shape()?,
        };
        let z = self.b.temp(&format!("struct({d}) {{ all_: char({d}) @0, z: zoned({d},{s}) @0 }}"));
        let opnd = self.dec_operand(v);
        self.b.ins(format!("dec.mov {z}.z, {opnd}"));
        let _ = Vk::Place;
        Some((format!("{z}.all_"), u32::from(d)))
    }

    /// `B`: blanks or zeros into a field after it is printed.
    fn blank_after(&mut self, v: &V) {
        if v.kind != Vk::Place {
            return;
        }
        let lit = if v.ty.is_numeric() { "*ZEROS" } else { "*BLANKS" };
        self.b.ins(format!("mov {}, {lit}", v.op));
    }

    /// `PAGE`, `PAGE1`..`PAGE7`: the page counter, advanced each time a
    /// record holding it prints.
    fn page_field(&mut self, name: &str) -> V {
        let place = format!("$page_{}", name.to_ascii_lowercase());
        self.pages.insert(place.clone());
        self.b.ins(format!("dec.add {place}, {place}, 1 prec(5,0)"));
        V::place(Ty::Zoned(4, 0), place)
    }
}

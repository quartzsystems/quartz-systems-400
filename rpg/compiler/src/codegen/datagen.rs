// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `DATA-GEN`: the variable described to the runtime as a shape, which
//! calls the generator `%GEN` names for each event (`qrpgrt::datagen`).
//!
//! A shape is one record per node, prefix order, separated by `;`:
//! `name|desc|off|dim|stride|children|type|format|separator|ccsid` with
//! the name as the source spells it, the descriptor word of a scalar (0
//! for a structure), `QrnDatatype_*`, `QrnDtzFormat_*`, the separator
//! (or decimal point) and the field's CCSID (0 for none).

use rpgfront::ast::{Def, Expr, Op};

use super::Gen;
use super::emit::quote;
use super::expr::{V, Vk, Want};
use crate::diag::ids;
use crate::types::Ty;

/// The spellings of the module's names as the source writes them: each
/// definition's name as its line has it.
pub fn spellings<S: std::hash::BuildHasher>(m: &rpgfront::ast::Module, lines: &[rpgfront::reader::Line], out: &mut std::collections::HashMap<String, String, S>) {
    fn walk<S: std::hash::BuildHasher>(defs: &[Def], lines: &[rpgfront::reader::Line], out: &mut std::collections::HashMap<String, String, S>) {
        for d in defs {
            let up = d.name.to_ascii_uppercase();
            if let Some(l) = lines.iter().find(|l| l.no == d.at.line) {
                let text = l.text.to_ascii_uppercase();
                let mut from = 0;
                while let Some(i) = text[from..].find(&up).map(|i| i + from) {
                    let before = text[..i].chars().last().is_none_or(|c| !c.is_ascii_alphanumeric() && c != '_');
                    let after = text[i + up.len()..].chars().next().is_none_or(|c| !c.is_ascii_alphanumeric() && c != '_');
                    if before && after && l.text.is_char_boundary(i) && l.text.is_char_boundary(i + up.len()) {
                        out.entry(up.clone()).or_insert_with(|| l.text[i..i + up.len()].to_string());
                        break;
                    }
                    from = i + 1;
                }
            }
            walk(&d.children, lines, out);
        }
    }
    walk(&m.defs, lines, out);
    for p in &m.procs {
        walk(&p.defs, lines, out);
    }
}

impl Gen<'_> {
    /// The name as written.
    fn spelled(&self, name: &str) -> String {
        let short = name.rsplit('.').next().unwrap_or(name).to_ascii_uppercase();
        self.t.spellings.get(&short).cloned().unwrap_or(short)
    }

    /// A variable's shape for `DATA-GEN`.
    fn gen_shape(&self, name: &str, ty: &Ty, dim: u32, off: u32, out: &mut String) {
        let stride = self.t.size(ty);
        let n = self.spelled(name);
        match ty {
            Ty::Ds(i) => {
                let subs = self.t.ds.get(*i).map(|d| d.subfields.clone()).unwrap_or_default();
                let subs: Vec<_> = subs.into_iter().filter(|s| !self.t.syms[*s].name.starts_with('*')).collect();
                out.push_str(&format!("{n}|0|{off}|{dim}|{stride}|{}|0|0| |0;", subs.len()));
                for s in subs {
                    let sym = self.t.syms[s].clone();
                    self.gen_shape(&sym.name, &sym.ty, sym.dim, sym.offset, out);
                }
            }
            t => {
                let d = self.desc_of(&V::place(t.clone(), String::new()));
                let (dt, fmt, sep, ccsid) = match t {
                    Ty::Char(_) | Ty::VarChar(..) => (1, 0, '\0', 37),
                    Ty::Ucs2(_) | Ty::VarUcs2(..) => (3, 0, '\0', 13488),
                    Ty::Graph(_) | Ty::VarGraph(..) => (5, 0, '\0', 0),
                    Ty::Ind => (7, 0, '\0', 0),
                    Ty::Packed(..) | Ty::Zoned(..) | Ty::Bin(..) => (8, 0, '.', 0),
                    Ty::Int(_) => (9, 0, '.', 0),
                    Ty::Uns(_) => (10, 0, '.', 0),
                    Ty::Float(_) => (11, 0, '.', 0),
                    Ty::Date(f, s) => (12, dtz(&format!("{f:?}")), *s, 0),
                    Ty::Time(f, s) => (13, dtz(&format!("{f:?}")), *s, 0),
                    Ty::Timestamp(_) => (14, 1, '-', 0),
                    _ => (1, 0, '\0', 0),
                };
                let sep = if sep == '\0' { "~".to_string() } else { sep.to_string() };
                out.push_str(&format!("{n}|{d}|{off}|{dim}|{stride}|0|{dt}|{fmt}|{sep}|{ccsid};"));
            }
        }
    }

    /// `DATA-GEN var %DATA(out:opts) %GEN(generator:parm)`.
    pub fn data_gen(&mut self, op: &Op, var: &Expr, dargs: &[Expr], gargs: &[Expr]) {
        let Some(v) = self.expr(var, Want::default()) else { return };
        if v.kind != Vk::Place {
            self.err(ids::NOT_ASSIGNABLE, 30, op.code.clone());
            return;
        }
        let name = v.sym.map_or_else(String::new, |s| self.t.syms[s].name.clone());
        if let Some(s) = v.sym {
            self.t.refer(s, self.at.line, false);
        }
        let mut shape = String::new();
        self.gen_shape(&name, &v.ty, v.dim, 0, &mut shape);
        let Some(o) = dargs.first().and_then(|e| self.expr(e, Want::default())) else { return };
        if o.kind != Vk::Place || !o.ty.is_string() {
            self.err(ids::NOT_ASSIGNABLE, 30, op.code.clone());
            return;
        }
        if let Some(s) = o.sym {
            self.t.refer(s, self.at.line, true);
        }
        let od = self.desc_of(&o);
        let Some((opts, optd)) = self.xml_text(dargs.get(1)) else { return };
        let Some((pname, nd, ptr, up, ud, uk)) = self.callee_operands(gargs) else { return };
        let target = if matches!(v.ty, Ty::Ds(_)) && v.dim == 0 { format!("{}.all_", v.op) } else { v.op.clone() };
        self.rt_proc(
            "_QRNX_DATAGEN",
            "($v: bytes(1) *VARSIZE, const $shape: varchar(32000), $out: bytes(1) *VARSIZE, byval $od: i64, \
             $opts: bytes(1) *VARSIZE, byval $optd: i64, $name: bytes(1) *VARSIZE, byval $nd: i64, \
             $ptr: bytes(16) *VARSIZE, $up: bytes(1) *VARSIZE, byval $ud: i64, byval $uk: i32)",
        );
        self.b.ins(format!(
            "callb _QRNX_DATAGEN({target}, {}, {}, {od}, {opts}, {optd}, {pname}, {nd}, {ptr}, {up}, {ud}, {uk})",
            quote(&shape),
            o.op
        ));
    }
}

/// `QrnDtzFormat_*` of a format.
fn dtz(f: &str) -> u8 {
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

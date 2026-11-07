// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `DUMP`: the runtime prints the formatted dump (`qrpgrt::dump`) from
//! a description of the variables, their addresses and the indicators.
//!
//! The description is records separated by `;`: `H|module|srcfile|
//! srclib|srcmbr` first, `P|name` before a subprocedure's own
//! variables, and `V|depth|name|attributes|dim|type|slot|stride` per
//! variable (`type` as [`type_code`] gives it, `slot` the variable's
//! address in the address list, -1 for none, -2 for storage that is
//! not active here).

use rpgfront::ast::{Expr, Lit, Op};

use super::Gen;
use super::emit::quote;
use crate::names::mangle;
use crate::syms::{Kind, Storage, SymId};
use crate::types::Ty;

/// The indicators in the order the runtime prints them; `01`-`99` are
/// the general ones.
pub const DUMP_INDS: &[&str] = &[
    "H1", "H2", "H3", "H4", "H5", "H6", "H7", "H8", "H9", "KA", "KB", "KC", "KD", "KE", "KF", "KG",
    "KH", "KI", "KJ", "KK", "KL", "KM", "KN", "KP", "KQ", "KR", "KS", "KT", "KU", "KV", "KW", "KX",
    "KY", "L1", "L2", "L3", "L4", "L5", "L6", "L7", "L8", "L9", "OA", "OB", "OC", "OD", "OE", "OF",
    "OG", "OV", "U1", "U2", "U3", "U4", "U5", "U6", "U7", "U8", "LR", "MR", "RT", "1P",
];

/// The runtime's code for a type: `C:n` characters, `V:n:p` varying,
/// `U:n` UCS-2, `W:n:p` varying UCS-2, `G:n` graphic, `N` indicator,
/// `P:d:s` packed, `Z:d:s` zoned, `B:d:s` binary, `I:d` and `J:d`
/// integers, `F:b` float, `D:n`, `T:n`, `S:n` dates, times and
/// timestamps of `n` characters, `*` and `&` pointers, `X:n` bytes.
fn type_code(g: &Gen<'_>, t: &Ty) -> String {
    match t {
        Ty::Char(n) => format!("C:{n}"),
        Ty::VarChar(n, p) => format!("V:{n}:{p}"),
        Ty::Ucs2(n) => format!("U:{n}"),
        Ty::VarUcs2(n, p) => format!("W:{n}:{p}"),
        Ty::Graph(n) => format!("G:{n}"),
        Ty::Ind => "N".into(),
        Ty::Packed(d, s) => format!("P:{d}:{s}"),
        Ty::Zoned(d, s) => format!("Z:{d}:{s}"),
        Ty::Bin(d, s) => format!("B:{d}:{s}"),
        Ty::Int(d) => format!("I:{d}"),
        Ty::Uns(d) => format!("J:{d}"),
        Ty::Float(b) => format!("F:{b}"),
        Ty::Date(..) => format!("D:{}", g.t.size(t)),
        Ty::Time(..) => format!("T:{}", g.t.size(t)),
        Ty::Timestamp(_) => format!("S:{}", g.t.size(t)),
        Ty::Ptr => "*".into(),
        Ty::ProcPtr => "&".into(),
        other => format!("X:{}", g.t.size(other)),
    }
}

/// The attributes column: the type as the dump spells it.
fn attributes(g: &Gen<'_>, t: &Ty) -> String {
    match t {
        Ty::Char(n) => format!("CHAR({n})"),
        Ty::VarChar(n, p) => format!("CHAR({n}) VARYING({p})"),
        Ty::Ucs2(n) => format!("UCS2({n}) CCSID(13488)"),
        Ty::VarUcs2(n, p) => format!("UCS2({n}) VARYING({p})"),
        Ty::Graph(n) => format!("GRAPHIC({n})"),
        Ty::VarGraph(n, p) => format!("GRAPHIC({n}) VARYING({p})"),
        Ty::Ind => "INDICATOR(1)".into(),
        Ty::Packed(d, s) => format!("PACKED({d},{s})"),
        Ty::Zoned(d, s) => format!("ZONED({d},{s})"),
        Ty::Bin(d, s) => format!("BIN({d},{s})"),
        Ty::Int(d) => format!("INT({d})"),
        Ty::Uns(d) => format!("UNS({d})"),
        Ty::Float(b) => format!("FLT({b})"),
        Ty::Date(..) => format!("DATE({})", g.t.size(t)),
        Ty::Time(..) => format!("TIME({})", g.t.size(t)),
        Ty::Timestamp(_) => format!("TIMESTAMP({})", g.t.size(t)),
        Ty::Ptr => "POINTER".into(),
        Ty::ProcPtr => "PROCPTR".into(),
        Ty::Ds(_) => "DS".into(),
        other => format!("CHAR({})", g.t.size(other)),
    }
}

impl Gen<'_> {
    /// The variables a dump shows of a scope: by name, data structures
    /// with their subfields.
    fn dump_ids(&self, scope: usize) -> Vec<SymId> {
        let mut ids: Vec<SymId> = (0..self.t.syms.len())
            .filter(|&i| {
                let s = &self.t.syms[i];
                s.scope == scope
                    && s.parent.is_none()
                    && !s.template
                    && !s.name.starts_with('_')
                    && !s.name.starts_with('*')
                    && !s.ir.is_empty()
                    && matches!(s.kind, Some(Kind::Field | Kind::Ds(_) | Kind::Param))
                    && !matches!(s.storage, Storage::Based(_) | Storage::Import)
            })
            .collect();
        ids.sort_by(|a, b| self.t.syms[*a].name.cmp(&self.t.syms[*b].name));
        ids.dedup_by(|a, b| self.t.syms[*a].name == self.t.syms[*b].name);
        ids
    }

    /// `DUMP` (`DUMP(A)` always, `DUMP` under `DEBUG(*YES)`).
    pub fn dump_op(&mut self, op: &Op) {
        let ident = match op.f1.first().or(op.f2.first()) {
            Some(Expr::Lit(Lit::Char(s), _)) => s.clone(),
            _ => String::new(),
        };
        let mut desc = format!(
            "H|{}|{}|{}|{}",
            self.opts.name.to_ascii_uppercase(),
            self.opts.src.1.to_ascii_uppercase(),
            self.opts.src.0.to_ascii_uppercase(),
            self.opts.src.2.to_ascii_uppercase()
        );
        // Variables only with a debug view, as IBM's.
        let mut places: Vec<String> = Vec::new();
        if self.debug {
            let mut scopes = vec![0];
            scopes.extend(1..=self.proc_order.len());
            for scope in scopes {
                if scope > 0 {
                    let name = self.proc_order[scope - 1].clone();
                    desc.push_str(&format!(";P|{name}"));
                }
                // Another procedure's storage is not reachable from here.
                let here = scope == 0 || scope == self.scope;
                for id in self.dump_ids(scope) {
                    let s = self.t.syms[id].clone();
                    if s.kind == Some(Kind::Param) && !here {
                        continue;
                    }
                    let place = super::place_of(self.t, id);
                    // A structure without subfields shows as its characters.
                    let empty =
                        matches!(s.kind, Some(Kind::Ds(di)) if self.t.ds[di].subfields.is_empty());
                    if empty {
                        let ty = Ty::Char(self.t.size(&s.ty));
                        self.dump_var(
                            &mut desc,
                            &mut places,
                            0,
                            &s.name,
                            &ty,
                            s.dim,
                            &format!("{place}.all_"),
                            here,
                        );
                        continue;
                    }
                    self.dump_var(
                        &mut desc,
                        &mut places,
                        0,
                        &s.name,
                        &s.ty,
                        s.dim,
                        &place,
                        here,
                    );
                    if let (Some(Kind::Ds(di)), 0) = (&s.kind, s.dim) {
                        let subs = self.t.ds[*di].subfields.clone();
                        for sub in subs {
                            let ss = self.t.syms[sub].clone();
                            if ss.name.starts_with('*') || ss.name.is_empty() {
                                continue;
                            }
                            let sp = format!("{place}.{}", mangle(&ss.name));
                            self.dump_var(
                                &mut desc,
                                &mut places,
                                1,
                                &ss.name,
                                &ss.ty,
                                ss.dim,
                                &sp,
                                here,
                            );
                        }
                    }
                }
            }
        }
        let n = places.len().max(1);
        let addrs = self.b.temp(&format!("char({})", 16 * n));
        let base = self.b.reg();
        self.b.ins(format!("{base} = addr {addrs}"));
        for (i, p) in places.iter().enumerate() {
            let r = self.b.reg();
            self.b.ins(format!("{r} = addr {p}"));
            self.b.ins(format!("store [{base} +{}]:ptr, {r}", 16 * i));
        }
        let inds = self.b.temp(&format!("char({})", DUMP_INDS.len() + 99));
        let ib = self.b.reg();
        self.b.ins(format!("{ib} = addr {inds}"));
        let names = DUMP_INDS
            .iter()
            .take(DUMP_INDS.len() - 4)
            .map(|s| (*s).to_string())
            .chain((1..=99).map(|i| format!("{i:02}")))
            .chain(["LR", "MR", "RT", "1P"].iter().map(|s| (*s).to_string()));
        for (k, name) in names.enumerate() {
            if let Some(v) = self.indicator(&name) {
                let r = self.b.reg();
                self.b.ins(format!("{r} = load {}", v.op));
                self.b.ins(format!("store [{ib} +{k}]:ind, {r}"));
            }
        }
        self.rt_proc(
            "_QRNX_DUMP",
            "(const $id: varchar(200), const $desc: varchar(32000), $addrs: bytes(1) *VARSIZE, $inds: bytes(1) *VARSIZE)",
        );
        if desc.len() > 32000 {
            desc.truncate(desc[..32000].rfind(';').unwrap_or(0));
        }
        self.b.ins(format!(
            "callb _QRNX_DUMP({}, {}, {addrs}, {inds})",
            quote(&ident),
            quote(&desc)
        ));
    }

    /// One variable's record; its address goes to the list when it is
    /// reachable.
    #[allow(clippy::too_many_arguments)]
    fn dump_var(
        &self,
        desc: &mut String,
        places: &mut Vec<String>,
        depth: u32,
        name: &str,
        ty: &Ty,
        dim: u32,
        place: &str,
        here: bool,
    ) {
        let slot = if !here {
            -2
        } else if matches!(ty, Ty::Ds(_)) && dim == 0 {
            -1
        } else {
            places.push(place.to_string());
            i64::try_from(places.len()).unwrap_or(0) - 1
        };
        let stride = self.t.size(ty);
        desc.push_str(&format!(
            ";V|{depth}|{}|{}|{dim}|{}|{slot}|{stride}",
            name.to_ascii_uppercase(),
            attributes(self, ty),
            type_code(self, ty)
        ));
    }
}

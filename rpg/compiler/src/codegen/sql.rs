// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! An embedded SQL statement as calls to the runtime: each input host
//! variable (`_QRNX_SQLIN`) and output host variable (`_QRNX_SQLOUT`)
//! by address with its descriptor and indicator, then `_QRNX_SQLRUN`
//! with the SQLCA and the statement's descriptor text, which runs it
//! and fills the outputs; then the `WHENEVER` branches.

use rpgfront::ast::{BinOp, Expr, Lit, Pos, Step, Stmt};
use rpgsqlpre::{HostRef, Rows};
use rpgsqlpre::{Desc, Op};

use crate::codegen::Gen;
use crate::codegen::emit::quote;
use crate::codegen::expr::V;
use crate::names::mangle;
use crate::syms::Kind;
use crate::types::{DatFmt, TimFmt, Ty};

/// One host variable as the runtime takes it.
struct Host {
    place: String,
    desc: i64,
    extra: i64,
    ind: Option<String>,
}

/// A date or time host variable's format for the runtime: the format's
/// code in the low byte, the separator above it.
fn extra_of(ty: &Ty) -> i64 {
    match ty {
        Ty::Date(f, sep) => {
            let code = match f {
                DatFmt::Iso => 1,
                DatFmt::Usa => 2,
                DatFmt::Eur => 3,
                DatFmt::Jis => 4,
                DatFmt::Mdy => 5,
                DatFmt::Dmy => 6,
                DatFmt::Ymd => 7,
                DatFmt::Jul => 8,
            };
            code | (i64::from(u32::from(*sep)) << 8)
        }
        Ty::Time(f, sep) => {
            let code = match f {
                TimFmt::Iso => 1,
                TimFmt::Hms => 2,
                TimFmt::Usa => 3,
                TimFmt::Eur => 4,
                TimFmt::Jis => 5,
            };
            code | (i64::from(u32::from(*sep)) << 8)
        }
        _ => 0,
    }
}

impl Gen<'_> {
    /// A host variable's reference, `DS.SUB` as a qualified one.
    fn host_ref(&mut self, name: &str, modify: bool, at: Pos) -> Option<V> {
        let mut parts = name.split('.');
        let first = parts.next()?.to_string();
        let steps: Vec<Step> = parts.map(|p| Step::Field(p.to_string())).collect();
        self.ref_at = at;
        self.reference(&first, &steps, modify)
    }

    /// The places of a host variable: a structure's subfields in
    /// order, element 1 of an array of structures; each with its
    /// indicator (an array's elements or a structure's subfields, in
    /// step with the fields).
    fn hosts(&mut self, h: &HostRef, modify: bool) -> Vec<Host> {
        let at = Pos { line: h.line, col: 0 };
        let Some(v) = self.host_ref(&h.name, modify, at) else { return Vec::new() };
        let mut out = Vec::new();
        let base = if v.dim > 0 { format!("{}[0]", v.op) } else { v.op.clone() };
        match v.ty {
            Ty::Ds(di) => {
                for sid in self.t.ds[di].subfields.clone() {
                    let s = self.t.syms[sid].clone();
                    if s.name.is_empty() || s.name == "*N" {
                        continue;
                    }
                    let place = format!("{base}.{}", mangle(&s.name));
                    let fv = V::place(s.ty.clone(), place.clone());
                    out.push(Host {
                        desc: self.desc_of(&fv),
                        extra: extra_of(&s.ty),
                        place,
                        ind: None,
                    });
                }
            }
            _ => {
                let fv = V::place(v.ty.clone(), base.clone());
                out.push(Host {
                    desc: self.desc_of(&fv),
                    extra: extra_of(&v.ty),
                    place: base,
                    ind: None,
                });
            }
        }
        if let Some(ind) = &h.ind
            && let Some(iv) = self.host_ref(ind, true, at)
        {
            let inds: Vec<String> = match (&iv.ty, iv.dim) {
                (_, d) if d > 0 => (0..d).map(|i| format!("{}[{i}]", iv.op)).collect(),
                (Ty::Ds(di), _) => self.t.ds[*di]
                    .subfields
                    .iter()
                    .map(|&s| format!("{}.{}", iv.op, mangle(&self.t.syms[s].name)))
                    .collect(),
                _ => vec![iv.op.clone()],
            };
            for (k, x) in out.iter_mut().enumerate() {
                x.ind = inds.get(k).cloned();
            }
        }
        out
    }

    /// The element size of an array of structures named by a host
    /// variable, for a multiple-row `FETCH`.
    fn stride_of(&mut self, h: &HostRef) -> i64 {
        let Some(id) = self.t.find(self.scope, h.name.split('.').next().unwrap_or("")) else { return 0 };
        let s = &self.t.syms[id];
        match (&s.kind, &s.ty) {
            (Some(Kind::Ds(_) | Kind::Field | Kind::Param), Ty::Ds(di)) if s.dim > 0 => i64::from(self.t.ds_len(*di)),
            (_, t) if s.dim > 0 => i64::from(self.t.size(t)),
            _ => 0,
        }
    }

    fn sql_in(&mut self, x: &Host) {
        self.rt_proc(
            "_QRNX_SQLIN",
            "($v: bytes(1) *VARSIZE, byval $d: i64, byval $x: i64, $i: bytes(1) *VARSIZE, byval $hi: i64)",
        );
        let (ind, has) = match &x.ind {
            Some(i) => (i.clone(), 1),
            None => (x.place.clone(), 0),
        };
        self.b.ins(format!("callb _QRNX_SQLIN({}, {}, {}, {ind}, {has})", x.place, x.desc, x.extra));
    }

    fn sql_out(&mut self, x: &Host, stride: i64) {
        self.rt_proc(
            "_QRNX_SQLOUT",
            "($v: bytes(1) *VARSIZE, byval $d: i64, byval $x: i64, $i: bytes(1) *VARSIZE, byval $hi: i64, byval $st: i64)",
        );
        let (ind, has) = match &x.ind {
            Some(i) => (i.clone(), 1),
            None => (x.place.clone(), 0),
        };
        self.b.ins(format!("callb _QRNX_SQLOUT({}, {}, {}, {ind}, {has}, {stride})", x.place, x.desc, x.extra));
    }

    /// One embedded statement.
    pub fn exec_sql(&mut self, s: &Stmt) {
        let Some(st) = self.sql.get(&s.at.line).cloned() else { return };
        for h in &st.inputs {
            for x in self.hosts(h, false) {
                self.sql_in(&x);
            }
        }
        match &st.rows {
            Some(Rows::Host(h)) => {
                for x in self.hosts(h, false) {
                    self.sql_in(&x);
                }
            }
            Some(Rows::Lit(n)) => {
                let ty = Ty::Int(20);
                let t = self.b.temp(&self.ir_ty(&ty));
                self.b.ins(format!("store {t}, {n}"));
                let desc = self.desc_of(&V::place(ty, t.clone()));
                self.sql_in(&Host {
                    place: t,
                    desc,
                    extra: 0,
                    ind: None,
                });
            }
            None => {}
        }
        for h in &st.outputs {
            let stride = if st.rows.is_some() { self.stride_of(h) } else { 0 };
            for x in self.hosts(h, true) {
                self.sql_out(&x, stride);
            }
        }
        if let Some(h) = &st.da
            && let Some(da) = self.host_ref(&h.name, true, Pos { line: h.line, col: 0 })
        {
            let mode = match st.desc.op {
                Op::Describe | Op::Prepare => 3,
                Op::Fetch => 2,
                _ => 1,
            };
            let da = self.as_chars(&da);
            self.rt_proc("_QRNX_SQLDA", "($da: bytes(1) *VARSIZE, byval $m: i64)");
            self.b.ins(format!("callb _QRNX_SQLDA({}, {mode})", da.op));
        }
        let Some(ca) = self.host_ref("SQLCA", true, s.at) else { return };
        let ca = self.as_chars(&ca);
        self.sql_run(&ca.op, &st.desc);
        self.whenever(&st.whenever, s.at);
    }

    /// `_QRNX_SQLRUN` of a statement with the SQLCA at `ca`.
    fn sql_run(&mut self, ca: &str, desc: &Desc) {
        let text = desc.encode();
        let lit = V::lit(Ty::Char(u32::try_from(text.len()).unwrap_or(1).max(1)), quote(&text));
        let t = self.rt_of(&lit, None);
        let scope = self.opts.name.to_ascii_uppercase();
        let sl = V::lit(Ty::Char(u32::try_from(scope.len()).unwrap_or(1).max(1)), quote(&scope));
        let sr = self.rt_of(&sl, None);
        self.rt_proc(
            "_QRNX_SQLRUN",
            "($ca: bytes(1) *VARSIZE, $t: bytes(1) *VARSIZE, byval $tl: i64, $s: bytes(1) *VARSIZE, byval $sl: i64)",
        );
        let ca = if ca.is_empty() { t.op.clone() } else { ca.to_string() };
        self.b.ins(format!("callb _QRNX_SQLRUN({ca}, {}, {}, {}, {})", t.op, text.len(), sr.op, scope.len()));
    }

    /// `_qrni_sqlend`, which the program entry calls as it returns under
    /// `CLOSQLCSR(*ENDMOD)`: the module's cursors close and its
    /// prepared statements go. The runtime leaves the SQLCA alone.
    pub fn sql_end_proc(&mut self) -> String {
        let saved = std::mem::take(&mut self.b);
        let scope = std::mem::replace(&mut self.scope, 0);
        let end = Desc {
            op: Op::End,
            ..Desc::default()
        };
        self.sql_run("", &end);
        self.b.term("ret");
        let body = std::mem::replace(&mut self.b, saved);
        self.scope = scope;
        body.finish("proc _qrni_sqlend()", "ret")
    }

    /// The `WHENEVER` branches after a statement.
    fn whenever(&mut self, w: &rpgsqlpre::Whenever, at: Pos) {
        let code = || Expr::Ref {
            name: "SQLCODE".into(),
            steps: Vec::new(),
            at,
        };
        let num = |n: &str| Box::new(Expr::Lit(Lit::Num(n.into()), at));
        let mut tests: Vec<(Expr, String)> = Vec::new();
        if let Some(l) = &w.error {
            tests.push((Expr::Binary(BinOp::Lt, Box::new(code()), num("0"), at), l.clone()));
        }
        if let Some(l) = &w.not_found {
            tests.push((Expr::Binary(BinOp::Eq, Box::new(code()), num("100"), at), l.clone()));
        }
        if let Some(l) = &w.warning {
            let pos = Expr::Binary(BinOp::Gt, Box::new(code()), num("0"), at);
            let not100 = Expr::Binary(BinOp::Ne, Box::new(code()), num("100"), at);
            tests.push((Expr::Binary(BinOp::And, Box::new(pos), Box::new(not100), at), l.clone()));
        }
        for (e, label) in tests {
            if let Some(r) = self.cond(&e) {
                let tag = self.tag_label(&label);
                let cont = self.b.label();
                self.b.term(format!("brif {r}, {tag}, {cont}"));
                self.b.place(&cont);
            }
        }
    }
}

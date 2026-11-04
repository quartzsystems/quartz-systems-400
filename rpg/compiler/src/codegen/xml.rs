// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `XML-INTO` and `XML-SAX`: the document and options of `%XML`, the
//! target described to the runtime as a shape (`_QRNX_XMLINTO`), or the
//! `%HANDLER` procedure the runtime calls (`_QRNX_XMLSAX`).
//!
//! A shape is one record per node, prefix order: `name,desc,off,dim,
//! stride,children;` with the descriptor word of a scalar (0 for a
//! structure), the offset in the parent, the element count (0 for a
//! scalar) and the element size.

use rpgfront::ast::{Expr, Lit, Op};

use super::Gen;
use super::emit::quote;
use super::expr::{V, Vk, Want};
use crate::diag::ids;
use crate::names::mangle;
use crate::syms::Kind;
use crate::types::Ty;

/// The values of the `*XML_...` special words (`qrpgrt::xml::Kind`).
pub const WORDS: &[(&str, u32)] = &[
    ("*XML_START_DOCUMENT", 1),
    ("*XML_VERSION_INFO", 2),
    ("*XML_ENCODING_DECL", 3),
    ("*XML_STANDALONE_DECL", 4),
    ("*XML_DOCTYPE_DECL", 5),
    ("*XML_START_ELEMENT", 6),
    ("*XML_CHARS", 7),
    ("*XML_PREDEF_REF", 8),
    ("*XML_UCS2_REF", 9),
    ("*XML_UNKNOWN_REF", 10),
    ("*XML_END_ELEMENT", 11),
    ("*XML_ATTR_NAME", 12),
    ("*XML_ATTR_CHARS", 13),
    ("*XML_ATTR_PREDEF_REF", 14),
    ("*XML_ATTR_UCS2_REF", 15),
    ("*XML_UNKNOWN_ATTR_REF", 16),
    ("*XML_END_ATTR", 17),
    ("*XML_PI_TARGET", 18),
    ("*XML_PI_DATA", 19),
    ("*XML_START_CDATA", 20),
    ("*XML_END_CDATA", 21),
    ("*XML_COMMENT", 22),
    ("*XML_EXCEPTION", 23),
    ("*XML_END_DOCUMENT", 24),
];

/// The value of an `*XML_...` word.
#[must_use]
pub fn word_value(w: &str) -> Option<u32> {
    WORDS.iter().find(|(n, _)| *n == w).map(|(_, v)| *v)
}

impl Gen<'_> {
    /// The operands of `XML-INTO` and `XML-SAX`: the first operand and
    /// the arguments of `%XML`.
    fn xml_operands(&mut self, op: &Op) -> Option<(Expr, Vec<Expr>)> {
        let items = match &op.xf2 {
            Some(Expr::List(v, _)) => v.clone(),
            Some(e) => vec![e.clone()],
            None => Vec::new(),
        };
        if items.len() != 2 {
            self.err(ids::BAD_EXPRESSION_TYPE, 30, op.code.clone());
            return None;
        }
        match &items[1] {
            Expr::Bif { name, args, .. } if name == "%XML" && !args.is_empty() && args.len() <= 2 => {
                Some((items[0].clone(), args.clone()))
            }
            _ => {
                self.err(ids::BAD_EXPRESSION_TYPE, 30, op.code.clone());
                None
            }
        }
    }

    /// `%XML` options written as a literal are checked here.
    fn xml_options_ok(&mut self, e: Option<&Expr>) -> bool {
        let Some(Expr::Lit(Lit::Char(t), _)) = e else { return true };
        let bad = t.split_whitespace().find(|w| {
            let Some((k, v)) = w.split_once('=') else { return true };
            let v = v.to_ascii_lowercase();
            match k.to_ascii_lowercase().as_str() {
                "doc" => !matches!(v.as_str(), "string" | "file"),
                "case" => !matches!(v.as_str(), "lower" | "upper" | "any" | "convert"),
                "allowmissing" | "allowextra" => !matches!(v.as_str(), "yes" | "no"),
                "trim" => !matches!(v.as_str(), "all" | "none"),
                "path" | "ccsid" | "countprefix" | "datasubf" | "ns" | "nsprefix" => false,
                _ => true,
            }
        });
        match bad {
            Some(w) => {
                self.err(ids::XML_OPTIONS, 30, w.to_string());
                false
            }
            None => true,
        }
    }

    /// A character operand for the runtime: storage and descriptor.
    pub(crate) fn xml_text(&mut self, e: Option<&Expr>) -> Option<(String, i64)> {
        let mut v = match e {
            Some(e) => self.expr(e, Want::default())?,
            None => self.literal(&Lit::Char(String::new()))?,
        };
        // A UCS-2 document goes to the runtime as the job's characters.
        if let (Some(e), Ty::Ucs2(_) | Ty::VarUcs2(..)) = (e, &v.ty) {
            let at = self.at;
            v = self.expr(&Expr::Bif { name: "%CHAR".into(), args: vec![e.clone()], at }, Want::default())?;
        }
        if !v.ty.is_string() && v.fig.is_none() {
            self.err(ids::BAD_EXPRESSION_TYPE, 30, "%XML");
            return None;
        }
        let r = self.rt_of(&v, None);
        Some((r.op, r.desc))
    }

    /// A target's shape.
    fn xml_shape(&self, name: &str, ty: &Ty, dim: u32, off: u32, out: &mut String) {
        let short = name.rsplit('.').next().unwrap_or(name).to_ascii_uppercase();
        let stride = self.t.size(ty);
        match ty {
            Ty::Ds(i) => {
                let subs = self.t.ds.get(*i).map(|d| d.subfields.clone()).unwrap_or_default();
                out.push_str(&format!("{short},0,{off},{dim},{stride},{};", subs.len()));
                for s in subs {
                    let sym = self.t.syms[s].clone();
                    self.xml_shape(&sym.name, &sym.ty, sym.dim, sym.offset, out);
                }
            }
            t => {
                let d = self.desc_of(&V::place(t.clone(), String::new()));
                out.push_str(&format!("{short},{d},{off},{dim},{stride},0;"));
            }
        }
    }

    /// `XML-INTO target %XML(doc:opts)` or `XML-INTO %HANDLER(proc:comm)
    /// %XML(doc:opts)`. The number of elements set goes to the PSDS
    /// (positions 372-379).
    pub fn xml_into(&mut self, op: &Op) {
        let Some((first, xargs)) = self.xml_operands(op) else { return };
        if !self.xml_options_ok(xargs.get(1)) {
            return;
        }
        let Some((doc, dd)) = self.xml_text(xargs.first()) else { return };
        let Some((opts, od)) = self.xml_text(xargs.get(1)) else { return };
        self.rt_proc(
            "_QRNX_XMLINTO",
            "($t: bytes(1) *VARSIZE, const $shape: varchar(32000), $doc: bytes(1) *VARSIZE, byval $dd: i64, \
             $opts: bytes(1) *VARSIZE, byval $od: i64, $h: bytes(16) *VARSIZE, $comm: bytes(1) *VARSIZE) -> i64",
        );
        let mut shape = String::new();
        let (target, hproc, comm) = match &first {
            Expr::Bif { name, args, .. } if name == "%HANDLER" => {
                let Some((h, comm, parms)) = self.handler(args) else { return };
                // The handler takes the communication area, the elements
                // (a constant array) and their count (UNS(10) VALUE).
                let fits = parms.len() == 3
                    && parms[1].dim > 0
                    && parms[1].konst
                    && parms[2].value
                    && parms[2].ty == Ty::Uns(10);
                if !fits {
                    let n = match args.first() {
                        Some(Expr::Ref { name, .. }) => name.clone(),
                        _ => String::new(),
                    };
                    self.err(ids::HANDLER_PROC, 30, n);
                    return;
                }
                let p = parms[1].clone();
                self.xml_shape(&p.name, &p.ty, p.dim.max(1), 0, &mut shape);
                let buf = self.b.temp("char(1)");
                (buf, h, comm)
            }
            e => {
                let Some(v) = self.expr(e, Want::default()) else { return };
                if v.kind != Vk::Place {
                    self.err(ids::NOT_ASSIGNABLE, 30, op.code.clone());
                    return;
                }
                let name = v.sym.map_or_else(String::new, |s| self.t.syms[s].name.clone());
                if let Some(s) = v.sym {
                    self.t.refer(s, self.at.line, true);
                }
                self.xml_shape(&name, &v.ty, v.dim, 0, &mut shape);
                // No handler: sixteen zero bytes.
                let h = self.b.temp("char(16)");
                self.b.ins(format!("mov {h}, *LOVAL"));
                (v.op, h, self.b.temp("char(1)"))
            }
        };
        let n = self.b.reg();
        self.b.ins(format!(
            "{n} = callb _QRNX_XMLINTO({target}, {}, {doc}, {dd}, {opts}, {od}, {hproc}, {comm})",
            quote(&shape)
        ));
        let (p, len) = self.psds();
        if len >= 379 {
            let a = self.b.reg();
            self.b.ins(format!("{a} = addr {p}"));
            self.b.ins(format!("store [{a} +371]:i64, {n}"));
        }
    }

    /// `%HANDLER(proc:comm)`: a pointer to the procedure, the
    /// communication area and the procedure's parameters.
    fn handler(&mut self, args: &[Expr]) -> Option<(String, String, Vec<crate::syms::Parm>)> {
        let (Some(Expr::Ref { name, .. }), Some(c)) = (args.first(), args.get(1)) else {
            self.err(ids::BIF_ARGS, 30, "%HANDLER");
            return None;
        };
        let parms = match self.t.find(0, name).and_then(|id| self.t.syms[id].kind.clone()) {
            Some(Kind::Proto(pi)) => self.t.protos[pi].parms.clone(),
            _ => {
                self.err(ids::NOT_DEFINED, 30, name.clone());
                return None;
            }
        };
        if let Some(id) = self.t.find(0, name) {
            self.t.refer(id, self.at.line, false);
        }
        let cv = self.expr(c, Want::default())?;
        if cv.kind != Vk::Place {
            self.err(ids::NOT_ASSIGNABLE, 30, "%HANDLER");
            return None;
        }
        let h = self.b.temp("procptr");
        let r = self.b.reg();
        self.b.ins(format!("{r} = procaddr {}", mangle(name)));
        self.b.ins(format!("store {h}, {r}"));
        Some((h, cv.op, parms))
    }

    /// `XML-SAX %HANDLER(proc:comm) %XML(doc:opts)`.
    pub fn xml_sax(&mut self, op: &Op) {
        let Some((first, xargs)) = self.xml_operands(op) else { return };
        let Expr::Bif { name, args, .. } = &first else {
            self.err(ids::BAD_EXPRESSION_TYPE, 30, op.code.clone());
            return;
        };
        if name != "%HANDLER" {
            self.err(ids::BAD_EXPRESSION_TYPE, 30, op.code.clone());
            return;
        }
        let Some((h, comm, _)) = self.handler(&args.clone()) else { return };
        if !self.xml_options_ok(xargs.get(1)) {
            return;
        }
        let Some((doc, dd)) = self.xml_text(xargs.first()) else { return };
        let Some((opts, od)) = self.xml_text(xargs.get(1)) else { return };
        self.rt_proc(
            "_QRNX_XMLSAX",
            "($h: bytes(16) *VARSIZE, $comm: bytes(1) *VARSIZE, $doc: bytes(1) *VARSIZE, byval $dd: i64, \
             $opts: bytes(1) *VARSIZE, byval $od: i64)",
        );
        self.b.ins(format!("callb _QRNX_XMLSAX({h}, {comm}, {doc}, {dd}, {opts}, {od})"));
    }
}

impl Gen<'_> {
    /// `%DATA` options written as a literal are checked here: `RNF0236`.
    fn data_options_ok(&mut self, e: Option<&Expr>, generate: bool) -> bool {
        let Some(Expr::Lit(Lit::Char(t), _)) = e else { return true };
        let bad = t.split_whitespace().find(|w| {
            let Some((k, v)) = w.split_once('=') else { return true };
            let v = v.to_ascii_lowercase();
            let yes_no = matches!(v.as_str(), "yes" | "no");
            match k.to_ascii_lowercase().as_str() {
                "doc" => !matches!(v.as_str(), "string" | "file"),
                "ccsid" if !generate => !matches!(v.as_str(), "job" | "ucs2"),
                "case" if !generate => !matches!(v.as_str(), "lower" | "upper" | "any" | "convert"),
                "allowmissing" | "allowextra" if !generate => !yes_no,
                "trim" if !generate => !matches!(v.as_str(), "all" | "none"),
                "path" | "countprefix" if !generate => false,
                "countprefix" | "name" | "renameprefix" | "fileccsid" | "output" if generate => false,
                _ => true,
            }
        });
        match bad {
            Some(w) => {
                self.err(ids::DATA_OPTIONS, 30, w.to_string());
                false
            }
            None => true,
        }
    }

    /// The operands of `%PARSER` or `%GEN`: the name (characters) or a
    /// procedure pointer, and the user parameter as the runtime takes it
    /// (place, descriptor, kind: 0 none, 1 a string, 2 a variable, 3 a
    /// data structure).
    pub fn callee_operands(&mut self, args: &[Expr]) -> Option<(String, i64, String, String, i64, i32)> {
        let first = args.first()?;
        let v = self.expr(first, Want::default())?;
        let mut ptr = self.b.temp("char(16)");
        self.b.ins(format!("mov {ptr}, *LOVAL"));
        let (name, nd) = if v.ty == Ty::ProcPtr {
            let r = self.b.reg();
            self.b.ins(format!("{r} = load {}", v.op));
            ptr = self.b.temp("procptr");
            self.b.ins(format!("store {ptr}, {r}"));
            let t = self.b.temp("char(1)");
            (t, self.desc_of(&V::place(Ty::Char(1), String::new())))
        } else {
            if !v.ty.is_string() && v.fig.is_none() {
                self.err(ids::BAD_EXPRESSION_TYPE, 30, "%PARSER");
                return None;
            }
            let r = self.rt_of(&v, None);
            (r.op, r.desc)
        };
        let (up, ud, uk) = match args.get(1) {
            None => (self.b.temp("char(1)"), 0, 0),
            Some(e) => {
                let u = self.expr(e, Want::default())?;
                if u.kind == Vk::Place && u.fig.is_none() {
                    if let Some(s) = u.sym {
                        self.t.refer(s, self.at.line, true);
                    }
                    let kind = if matches!(u.ty, Ty::Ds(_)) { 3 } else { 2 };
                    let ty = if kind == 3 { Ty::Char(self.t.size(&u.ty)) } else { u.ty.clone() };
                    let d = self.desc_of(&V::place(ty, String::new()));
                    let op = if kind == 3 { format!("{}.all_", u.op) } else { u.op.clone() };
                    (op, d, kind)
                } else {
                    let r = self.rt_of(&u, None);
                    (r.op, r.desc, 1)
                }
            }
        };
        Some((name, nd, ptr, up, ud, uk))
    }

    /// `DATA-INTO target %DATA(doc:opts) %PARSER(parser:parm)` and
    /// `DATA-GEN var %DATA(out:opts) %GEN(generator:parm)`.
    pub fn data_op(&mut self, op: &Op, generate: bool) {
        let items = match &op.xf2 {
            Some(Expr::List(v, _)) => v.clone(),
            Some(e) => vec![e.clone()],
            None => Vec::new(),
        };
        let want = if generate { "%GEN" } else { "%PARSER" };
        let ok = items.len() == 3
            && matches!(&items[1], Expr::Bif { name, args, .. } if name == "%DATA" && !args.is_empty())
            && matches!(&items[2], Expr::Bif { name, args, .. } if name == want && !args.is_empty());
        if !ok {
            self.err(ids::BAD_EXPRESSION_TYPE, 30, op.code.clone());
            return;
        }
        let (Expr::Bif { args: dargs, .. }, Expr::Bif { args: pargs, .. }) = (&items[1], &items[2]) else { return };
        let (dargs, pargs) = (dargs.clone(), pargs.clone());
        if !self.data_options_ok(dargs.get(1), generate) {
            return;
        }
        if generate {
            self.data_gen(op, &items[0], &dargs, &pargs);
            return;
        }
        let Some((doc, dd)) = self.xml_text(dargs.first()) else { return };
        let Some((opts, od)) = self.xml_text(dargs.get(1)) else { return };
        let Some((name, nd, ptr, up, ud, uk)) = self.callee_operands(&pargs) else { return };
        let mut shape = String::new();
        let (target, hproc, comm) = match &items[0] {
            Expr::Bif { name, args, .. } if name == "%HANDLER" => {
                let Some((h, comm, parms)) = self.handler(&args.clone()) else { return };
                let fits = parms.len() == 3
                    && parms[1].dim > 0
                    && parms[1].konst
                    && parms[2].value
                    && matches!(parms[2].ty, Ty::Uns(10) | Ty::Int(10));
                if !fits {
                    let n = match args.first() {
                        Some(Expr::Ref { name, .. }) => name.clone(),
                        _ => String::new(),
                    };
                    self.err(ids::HANDLER_PROC, 30, n);
                    return;
                }
                let p = parms[1].clone();
                self.xml_shape(&p.name, &p.ty, p.dim.max(1), 0, &mut shape);
                (self.b.temp("char(1)"), h, comm)
            }
            e => {
                let Some(v) = self.expr(e, Want::default()) else { return };
                if v.kind != Vk::Place {
                    self.err(ids::NOT_ASSIGNABLE, 30, op.code.clone());
                    return;
                }
                let name = v.sym.map_or_else(String::new, |s| self.t.syms[s].name.clone());
                if let Some(s) = v.sym {
                    self.t.refer(s, self.at.line, true);
                }
                self.xml_shape(&name, &v.ty, v.dim, 0, &mut shape);
                let h = self.b.temp("char(16)");
                self.b.ins(format!("mov {h}, *LOVAL"));
                (v.op, h, self.b.temp("char(1)"))
            }
        };
        self.rt_proc(
            "_QRNX_DATAINTO",
            "($t: bytes(1) *VARSIZE, const $shape: varchar(32000), $doc: bytes(1) *VARSIZE, byval $dd: i64, \
             $opts: bytes(1) *VARSIZE, byval $od: i64, $name: bytes(1) *VARSIZE, byval $nd: i64, \
             $ptr: bytes(16) *VARSIZE, $up: bytes(1) *VARSIZE, byval $ud: i64, byval $uk: i32, \
             $h: bytes(16) *VARSIZE, $comm: bytes(1) *VARSIZE) -> i64",
        );
        let n = self.b.reg();
        self.b.ins(format!(
            "{n} = callb _QRNX_DATAINTO({target}, {}, {doc}, {dd}, {opts}, {od}, {name}, {nd}, {ptr}, {up}, {ud}, {uk}, {hproc}, {comm})",
            quote(&shape)
        ));
        let (p, len) = self.psds();
        if len >= 379 {
            let a = self.b.reg();
            self.b.ins(format!("{a} = addr {p}"));
            self.b.ins(format!("store [{a} +371]:i64, {n}"));
        }
    }
}

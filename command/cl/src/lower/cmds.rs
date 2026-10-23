// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Commands and calls: `CALL` and `CALLPRC`, the subroutine calls, the
//! message statements, the file statements, the data areas, and every
//! other command through its definition: a direct call of the
//! processing program with a parameter block built here, or a command
//! string built at run time.

use cmd::analyze::{Env, Resolved};
use cmd::block::{self, Patch};
use cmd::def::ParmType;
use cmd::token::{Tok, Tokens};
use cmd::value::Value;

use super::Gen;
use super::expr::{Val, qhex, qstr};
use crate::ast::{CallParm, CallTarget, DtaAra, Expr, FileOp, Lit, PrcParm, Stmt, Target, VarType};
use crate::diag::ids;
use crate::sem::Ty;

impl Gen<'_, '_> {
    /// `CALL PGM PARM`: character constants padded to 32, numbers as
    /// `packed(15,5)`, logical constants one byte, `*N` a null.
    pub(super) fn call(&mut self, s: &Stmt, target: &CallTarget, parms: &[CallParm]) {
        let mut args = Vec::new();
        for p in parms {
            let a = match p {
                CallParm::Var(v) => match self.t.var(v) {
                    Some(var) => var.place(),
                    None => {
                        self.err(s.line, ids::UNDECLARED, format!("&{v} is not declared"));
                        return;
                    }
                },
                CallParm::Lit(Lit::Str(text)) => {
                    let n = u32::try_from(text.chars().count()).unwrap_or(32).max(32);
                    let t = self.temp(&Ty::Char(n));
                    self.ins(&format!("mov {t}, {}", qstr(text)));
                    t
                }
                CallParm::Lit(Lit::Hex(b)) => {
                    let n = u32::try_from(b.len()).unwrap_or(32).max(32);
                    let t = self.temp(&Ty::Char(n));
                    self.ins(&format!("mov {t}, {}", qhex(b)));
                    t
                }
                CallParm::Lit(Lit::Num(n)) => {
                    let t = self.temp(&Ty::Dec(15, 5));
                    self.ins(&format!("dec.mov {t}, {n}"));
                    t
                }
                CallParm::Omit => {
                    let t = self.temp(&Ty::Char(1));
                    self.ins(&format!("mov {t}, *BLANKS"));
                    t
                }
                CallParm::Expr(e) => match self.ty(e) {
                    Ok(ty) => {
                        let ty = match ty {
                            Ty::Char(n) => Ty::Char(n.max(32)),
                            Ty::Dec(..) => Ty::Dec(15, 5),
                            other => other,
                        };
                        let t = self.temp(&ty);
                        if let Err(err) = self.into_place(&t, &ty, e) {
                            self.err(s.line, ids::EXPRESSION, err);
                            return;
                        }
                        t
                    }
                    Err(err) => {
                        self.err(s.line, ids::EXPRESSION, err);
                        return;
                    }
                },
            };
            args.push(a);
        }
        let list = args.join(", ");
        match target {
            CallTarget::Name(name, lib) if lib == "*LIBL" => {
                self.ins(&format!("call {name}({list})"));
            }
            CallTarget::Name(name, lib) => self.ins(&format!("call {lib}/{name}({list})")),
            CallTarget::Var(v) => match self.t.var(v) {
                Some(var) => self.ins(&format!("call {}({list})", var.place())),
                None => self.err(s.line, ids::UNDECLARED, format!("&{v} is not declared")),
            },
        }
    }

    /// `CALLPRC`: a bound call through a prototype built from the
    /// arguments.
    pub(super) fn callprc(&mut self, s: &Stmt, prc: &str, parms: &[PrcParm], rtn: Option<&Target>) {
        let mut sig = Vec::new();
        let mut args = Vec::new();
        for (i, p) in parms.iter().enumerate() {
            let ty = match self.ty(&p.value) {
                Ok(t) => t,
                Err(e) => {
                    self.err(s.line, ids::EXPRESSION, e);
                    return;
                }
            };
            if p.byval {
                let v = match self.value(&p.value) {
                    Ok(v) => v,
                    Err(e) => {
                        self.err(s.line, ids::EXPRESSION, e);
                        return;
                    }
                };
                match self.by_value(&v) {
                    Ok(r) => {
                        let qt = match (&v, &ty) {
                            (Val::Ind(_), _) => "ind".to_string(),
                            (_, Ty::Ptr) => "ptr".to_string(),
                            _ => "i32".to_string(),
                        };
                        sig.push(format!("byval $p{i}: {qt}"));
                        args.push(r);
                    }
                    Err(e) => {
                        self.err(s.line, ids::EXPRESSION, e);
                        return;
                    }
                }
                continue;
            }
            let place = match &p.value {
                Expr::Var(v) => match self.t.var(v) {
                    Some(var) => var.place(),
                    None => {
                        self.err(s.line, ids::UNDECLARED, format!("&{v} is not declared"));
                        return;
                    }
                },
                e => {
                    let t = self.temp(&ty);
                    if let Err(err) = self.into_place(&t, &ty, e) {
                        self.err(s.line, ids::EXPRESSION, err);
                        return;
                    }
                    t
                }
            };
            let qt = match &ty {
                Ty::Char(n) => format!("char({n})"),
                Ty::Dec(d, sc) => format!("packed({d},{sc})"),
                Ty::Lgl => "ind".into(),
                Ty::Ptr => "ptr".into(),
            };
            sig.push(format!("$p{i}: {qt}"));
            args.push(place);
        }
        let rtn_ty = rtn.and_then(|t| self.t.var(t.var()).map(crate::sem::Var::ty));
        let ret = match &rtn_ty {
            Some(Ty::Dec(..)) => " -> i32".to_string(),
            Some(Ty::Lgl) => " -> ind".to_string(),
            Some(Ty::Ptr) => " -> ptr".to_string(),
            _ => String::new(),
        };
        let name = prc.trim_matches('\'');
        let decl = format!("extern proc {name}({}){ret}", sig.join(", "));
        if !self.externs.contains(&decl) {
            self.externs.push(decl);
        }
        match (rtn, rtn_ty) {
            (Some(t), Some(Ty::Dec(..))) => {
                let r = self.reg("r");
                self.ins(&format!("{r} = callb {name}({})", args.join(", ")));
                self.ins(&format!("dec.mov ${}, {r}", t.var()));
            }
            (Some(t), Some(Ty::Lgl | Ty::Ptr)) => {
                let r = self.reg("r");
                self.ins(&format!("{r} = callb {name}({})", args.join(", ")));
                self.ins(&format!("store ${}, {r}", t.var()));
            }
            _ => self.ins(&format!("callb {name}({})", args.join(", "))),
        }
    }

    /// `CALLSUBR`: the return index on the stack, then the branch.
    pub(super) fn callsubr(&mut self, s: &Stmt, name: &str, rtn: Option<&Target>) {
        if !self.t.subrs.contains_key(name) {
            self.err(
                s.line,
                ids::SUBR,
                format!("subroutine {name} is not defined"),
            );
            return;
        }
        let ret = self.fresh("RT");
        let idx = self.subr_returns.len();
        self.subr_returns.push(ret.clone());
        let sp = self.reg("sp");
        self.ins(&format!("{sp} = load $_rsp"));
        self.ins(&format!("store $_rs[{sp}], {idx}"));
        let sp2 = self.reg("sp");
        self.ins(&format!("{sp2} = add {sp}, 1"));
        self.ins(&format!("store $_rsp, {sp2}"));
        self.term(&format!("br SUBR_{name}"));
        self.label(&ret);
        if let Some(t) = rtn {
            if let Some(v) = self.t.var(t.var()) {
                match v.ty {
                    VarType::Dec | VarType::Int | VarType::Uint => {
                        self.ins(&format!("dec.mov {}, $_srv", v.place()));
                    }
                    _ => self.err(
                        s.line,
                        ids::TYPE_MISMATCH,
                        "RTNVAL needs a numeric variable",
                    ),
                }
            } else {
                self.err(
                    s.line,
                    ids::UNDECLARED,
                    format!("&{} is not declared", t.var()),
                );
            }
        }
    }

    /// `RCVF`, `SNDF`, `SNDRCVF`, `WAIT`, `ENDRCV`.
    pub(super) fn file_io(&mut self, s: &Stmt, op: FileOp, opnid: Option<&str>, fmt: Option<&str>) {
        let Some(f) = self
            .t
            .files
            .iter()
            .find(|f| opnid.is_none_or(|o| f.opnid == o))
            .cloned()
        else {
            self.err(
                s.line,
                ids::BAD_VALUE,
                "no file is declared for the operation",
            );
            return;
        };
        let fname = format!("F{}", f.n);
        let (k, format) = match fmt {
            Some(name) => match f.formats.iter().position(|m| m.name == name) {
                Some(k) => (k, f.formats[k].clone()),
                None => {
                    self.err(
                        s.line,
                        ids::BAD_VALUE,
                        format!("{name} is not a format of {}", f.name),
                    );
                    return;
                }
            },
            None => match f.formats.first() {
                Some(m) => (0, m.clone()),
                None => {
                    self.err(
                        s.line,
                        ids::BAD_VALUE,
                        format!("{} has no record format", f.name),
                    );
                    return;
                }
            },
        };
        let rec = format!("$_r{}_{k}", f.n);
        // A database file opens on first use; a display file opens
        // with its first operation.
        if !f.display {
            let open = self.file_open[&f.n].clone();
            let o = self.reg("o");
            self.ins(&format!("{o} = load {open}"));
            let opened = self.fresh("O");
            let go = self.fresh("G");
            self.term(&format!("brif {o}, {go}, {opened}"));
            self.label(&opened);
            let st = self.reg("s");
            self.ins(&format!("{st} = dbopen {fname}"));
            self.ins(&format!("store {open}, *ON"));
            self.label(&go);
        }
        if f.display {
            let verb = match op {
                FileOp::Rcvf | FileOp::Wait => "READ",
                FileOp::Sndf => "WRITE",
                FileOp::SndRcvf => "EXFMT",
                FileOp::EndRcv => return,
            };
            let writes = matches!(op, FileOp::Sndf | FileOp::SndRcvf);
            let reads = matches!(op, FileOp::Rcvf | FileOp::SndRcvf | FileOp::Wait);
            if writes {
                // The variables and &INnn into the output buffer.
                for fd in format.fields.iter().filter(|fd| fd.pos > 0) {
                    if let Some(v) = self.t.var(&fd.name) {
                        match v.ty {
                            VarType::Dec => {
                                self.ins(&format!("dec.mov {rec}.{}, {}", fd.name, v.place()));
                            }
                            _ => self.ins(&format!("mov {rec}.{}, {}", fd.name, v.place())),
                        }
                    }
                }
                for ind in &format.option_inds {
                    self.ins(&format!("mov {rec}._O{ind:02}, $IN{ind:02}"));
                }
            }
            // The runtime reports a failure (no display, no such
            // format) as the escape itself.
            let r = self.reg("s");
            self.ins(&format!(
                "{r} = dspio {fname} {verb} fmt({}) rec({rec})",
                format.name
            ));
            if reads {
                for ind in &format.response_inds {
                    self.ins(&format!("mov $IN{ind:02}, {rec}._R{ind:02}"));
                }
                for fd in format.fields.iter().filter(|fd| fd.in_pos.is_some()) {
                    if let Some(v) = self.t.var(&fd.name) {
                        match v.ty {
                            VarType::Dec => {
                                self.ins(&format!("dec.mov {}, {rec}._I{}", v.place(), fd.name));
                            }
                            _ => self.ins(&format!("mov {}, {rec}._I{}", v.place(), fd.name)),
                        }
                    }
                }
            }
            return;
        }
        match op {
            FileOp::Rcvf => {
                let r = self.reg("s");
                self.ins(&format!("{r} = dbio {fname} READ rec({rec})"));
                let eof = self.reg("e");
                self.ins(&format!("{eof} = cmp.eq {r}, 11"));
                let at_end = self.fresh("EF");
                let got = self.fresh("GT");
                self.term(&format!("brif {eof}, {at_end}, {got}"));
                self.label(&at_end);
                let d = self.temp(&Ty::Char(20));
                self.ins(&format!(
                    "mov {d}, {}",
                    qstr(&format!("{:<10}{:<10}", f.name, f.lib))
                ));
                self.ins(&format!("sndpgmmsg msgid(\"CPF0864\") msgf(\"QCPFMSG\") data({d}) type(*ESCAPE) to(*SAME)"));
                self.term(&format!("br {got}"));
                self.label(&got);
                for fd in &format.fields {
                    if let Some(v) = self.t.var(&fd.name) {
                        match v.ty {
                            VarType::Dec => {
                                self.ins(&format!("dec.mov {}, {rec}.{}", v.place(), fd.name));
                            }
                            _ => self.ins(&format!("mov {}, {rec}.{}", v.place(), fd.name)),
                        }
                    }
                }
            }
            FileOp::Sndf | FileOp::SndRcvf | FileOp::Wait | FileOp::EndRcv => {
                self.err(
                    s.line,
                    ids::BAD_VALUE,
                    "only RCVF applies to a database file",
                );
            }
        }
    }

    /// `CLOSE OPNID`.
    pub(super) fn close(&mut self, s: &Stmt, opnid: Option<&str>) {
        let Some(f) = self
            .t
            .files
            .iter()
            .find(|f| opnid.is_none_or(|o| f.opnid == o))
            .cloned()
        else {
            self.err(s.line, ids::BAD_VALUE, "no file is declared for CLOSE");
            return;
        };
        let open = self.file_open[&f.n].clone();
        let r = self.reg("s");
        self.ins(&format!("{r} = dbclose F{}", f.n));
        self.ins(&format!("store {open}, *OFF"));
    }

    /// `RTVDTAARA`: the whole area into a temporary, the substring
    /// into the variable.
    pub(super) fn rtvdtaara(&mut self, s: &Stmt, area: &DtaAra, target: &Target) {
        let Some(v) = self.t.var(target.var()).cloned() else {
            self.err(
                s.line,
                ids::UNDECLARED,
                format!("&{} is not declared", target.var()),
            );
            return;
        };
        let (name, lib) = Self::area_names(area);
        match v.ty {
            VarType::Dec => {
                self.ins(&format!("dtaara.read {name}, {lib}, {}", v.place()));
            }
            VarType::Char => {
                let whole = self.temp(&Ty::Char(2000));
                self.ins(&format!("dtaara.read {name}, {lib}, {whole}"));
                match &area.sub {
                    Some((start, len)) => {
                        let (st, ln) = match (self.int(start), self.int(len)) {
                            (Ok(a), Ok(b)) => (a, b),
                            (Err(e), _) | (_, Err(e)) => {
                                self.err(s.line, ids::EXPRESSION, e);
                                return;
                            }
                        };
                        self.ins(&format!("str.subst {}, {whole}, {st}, {ln}", v.place()));
                    }
                    None => self.ins(&format!("mov {}, {whole}", v.place())),
                }
            }
            VarType::Lgl => {
                let whole = self.temp(&Ty::Char(1));
                self.ins(&format!("dtaara.read {name}, {lib}, {whole}"));
                self.ins(&format!("cvt {}, {whole}", v.place()));
            }
            _ => self.err(
                s.line,
                ids::TYPE_MISMATCH,
                "RTNVAR must be *CHAR, *DEC or *LGL",
            ),
        }
    }

    /// `CHGDTAARA`: a substring is read, overlaid and written back.
    pub(super) fn chgdtaara(&mut self, s: &Stmt, area: &DtaAra, value: &Expr) {
        let (name, lib) = Self::area_names(area);
        match self.ty(value) {
            Ok(Ty::Dec(..)) => match self.num(value) {
                Ok(n) => {
                    let t = self.temp(&Ty::Dec(15, 5));
                    self.ins(&format!("dec.mov {t}, {n}"));
                    self.ins(&format!("dtaara.write {name}, {lib}, {t}"));
                }
                Err(e) => self.err(s.line, ids::EXPRESSION, e),
            },
            Ok(Ty::Char(_)) => {
                let src = match self.chr(value) {
                    Ok(c) => c,
                    Err(e) => {
                        self.err(s.line, ids::EXPRESSION, e);
                        return;
                    }
                };
                let whole = self.temp(&Ty::Char(2000));
                self.ins(&format!("dtaara.read {name}, {lib}, {whole}"));
                match &area.sub {
                    Some((start, len)) => {
                        let Expr::Lit(Lit::Num(n)) = len else {
                            self.err(s.line, ids::UNSUPPORTED, "CHGDTAARA with a variable length");
                            return;
                        };
                        let st = match self.int(start) {
                            Ok(a) => a,
                            Err(e) => {
                                self.err(s.line, ids::EXPRESSION, e);
                                return;
                            }
                        };
                        let p = self.reg("p");
                        self.ins(&format!("{p} = addr {whole}"));
                        let q = self.reg("q");
                        self.ins(&format!("{q} = ptradd {p}, {st}"));
                        let q2 = self.reg("q");
                        self.ins(&format!("{q2} = ptradd {q}, -1"));
                        self.ins(&format!("mov [{q2}]:char({n}), {src}"));
                    }
                    None => self.ins(&format!("mov {whole}, {src}")),
                }
                self.ins(&format!("dtaara.write {name}, {lib}, {whole}"));
            }
            Ok(_) => self.err(
                s.line,
                ids::TYPE_MISMATCH,
                "CHGDTAARA takes character or numeric data",
            ),
            Err(e) => self.err(s.line, ids::EXPRESSION, e),
        }
    }

    /// A data area's name and library as IR operands.
    fn area_names(area: &DtaAra) -> (String, String) {
        let name = qstr(&area.name);
        let lib = if area.name.starts_with('*') {
            qstr("*LIBL")
        } else {
            qstr(&area.lib)
        };
        (name, lib)
    }

    /// Any other command: analyzed against its definition; a `QSYS`
    /// command with plain values is called directly, the rest are run
    /// as a command string built at run time.
    pub(super) fn command(&mut self, s: &Stmt, toks: &Tokens) {
        let vars = self.t.vars.clone();
        let lookup = move |name: &str| -> Option<cmd::analyze::VarInfo> {
            let v = vars.get(name)?;
            let ty = match v.ty {
                VarType::Char => cmd::analyze::VarType::Char,
                VarType::Dec => cmd::analyze::VarType::Dec,
                VarType::Lgl => cmd::analyze::VarType::Lgl,
                VarType::Int => cmd::analyze::VarType::Int,
                VarType::Uint => cmd::analyze::VarType::Uint,
                VarType::Ptr => cmd::analyze::VarType::Ptr,
            };
            Some(cmd::analyze::VarInfo {
                ty,
                len: v.len,
                dec: v.dec,
            })
        };
        let r = {
            // A second environment over the same source, with the
            // program's variables, for this one analysis.
            let setting = self.env.setting;
            let mode = self.env.mode;
            let source: &mut dyn cmd::analyze::CmdSource = &mut *self.env.source;
            let mut env = Env::new(source);
            env.setting = setting;
            env.setting.program = true;
            env.mode = mode;
            env.vars = Some(&lookup);
            cmd::analyze::analyze_tokens(&mut env, toks)
        };
        let r = match r {
            Ok(r) => r,
            Err(e) => {
                for d in &e.diags {
                    let text = if d.detail.is_empty() {
                        d.msgid.clone()
                    } else {
                        format!("{} {}", d.msgid, d.detail)
                    };
                    self.err(s.line, ids::COMMAND, text);
                }
                return;
            }
        };
        match &r.name[..] {
            "SNDPGMMSG" => return self.sndpgmmsg(s, &r),
            "RCVMSG" => return self.rcvmsg(s, &r),
            "RMVMSG" => return self.rmvmsg(&r),
            "SNDUSRMSG" => return self.sndusrmsg(s, &r),
            _ => {}
        }
        let direct = r.lib == "QSYS"
            && sys_cpps::entry(&r.name).is_some()
            && r.parms.iter().all(|b| simple(b.value.as_ref()));
        if direct {
            self.direct(s, &r);
        } else {
            self.runtime(s, toks);
        }
    }

    /// The direct path: the block as a structure, variables patched in,
    /// the processing program called, return values copied back.
    fn direct(&mut self, s: &Stmt, r: &Resolved) {
        let built = match block::build(&r.def, &r.parms) {
            Ok(b) => b,
            Err(e) => {
                self.err(s.line, ids::COMMAND, e.to_string());
                return;
            }
        };
        let entry = sys_cpps::entry(&r.name).expect("checked");
        self.temps += 1;
        let tname = format!("C{}", self.temps);
        let vname = format!("$_c{}", self.temps);
        let mut fields = Vec::new();
        let mut bytes = Vec::new();
        let mut offsets = Vec::new();
        for seg in &built.segments {
            let seg = seg.clone().unwrap_or_default();
            let n = seg.len().max(1);
            offsets.push(bytes.len());
            fields.push(format!(
                "s{}: bytes({n}) @{}",
                offsets.len() - 1,
                bytes.len()
            ));
            bytes.extend_from_slice(&seg);
            if seg.is_empty() {
                bytes.push(0x40);
            }
        }
        let mut patches: Vec<(Patch, String)> = Vec::new();
        for (k, p) in built.patches.iter().enumerate() {
            let off = offsets[p.parm] + p.offset;
            let qt = match p.ty {
                ParmType::Dec => {
                    format!("packed({},{})", (p.len * 2).saturating_sub(1).max(1), p.dec)
                }
                ParmType::Int2 => "i16".into(),
                ParmType::Int4 => "i32".into(),
                ParmType::Uint2 => "u16".into(),
                ParmType::Uint4 => "u32".into(),
                ParmType::Lgl => "char(1)".into(),
                _ => format!("char({})", p.len.max(1)),
            };
            fields.push(format!("p{k}: {qt} @{off}"));
            patches.push((p.clone(), format!("{vname}.p{k}")));
        }
        self.types.push(format!(
            "type {tname} = struct({}) {{ {} }}",
            bytes.len(),
            fields.join(", ")
        ));
        self.decls
            .push(format!("  auto {vname}: {tname} inz({})", qhex(&bytes)));
        // Values in.
        for (p, field) in &patches {
            let Some(v) = self.t.var(&p.var) else {
                continue;
            };
            match (p.ty.clone(), v.ty) {
                (
                    ParmType::Dec
                    | ParmType::Int2
                    | ParmType::Int4
                    | ParmType::Uint2
                    | ParmType::Uint4,
                    _,
                ) => {
                    self.ins(&format!("dec.mov {field}, {}", v.place()));
                }
                (ParmType::Lgl, VarType::Lgl) => self.ins(&format!("cvt {field}, {}", v.place())),
                (_, VarType::Char) => self.ins(&format!("mov {field}, {}", v.place())),
                (_, VarType::Dec) => self.ins(&format!("cvt {field}, {}", v.place())),
                _ => self.ins(&format!("mov {field}, {}", v.place())),
            }
        }
        let args: Vec<String> = (0..built.segments.len())
            .map(|i| format!("{vname}.s{i}"))
            .collect();
        self.ins(&format!("call QSYS/{}({})", entry.cpp, args.join(", ")));
        // Return values out.
        for (p, field) in &patches {
            let parm = &r.def.parms[p.parm];
            if !parm.rtnval {
                continue;
            }
            let Some(v) = self.t.var(&p.var) else {
                continue;
            };
            match v.ty {
                VarType::Dec | VarType::Int | VarType::Uint => {
                    self.ins(&format!("dec.mov {}, {field}", v.place()));
                }
                VarType::Lgl => self.ins(&format!("cvt {}, {field}", v.place())),
                _ => self.ins(&format!("mov {}, {field}", v.place())),
            }
        }
    }

    /// The run-time path: the command text with `&n` placeholders and
    /// the values that fill them, handed to the runtime.
    fn runtime(&mut self, s: &Stmt, toks: &Tokens) {
        let mut pieces: Vec<Expr> = Vec::new();
        let mut text = String::new();
        if let Some(l) = &toks.lib {
            text.push_str(l);
            text.push('/');
        }
        text.push_str(&toks.name);
        for t in &toks.toks {
            text.push(' ');
            text.push_str(&template(t, &mut pieces));
        }
        let tl = self.temp(&Ty::Dec(10, 0));
        self.ins(&format!("dec.mov {tl}, {}", text.chars().count()));
        let tm = self.temp(&Ty::Char(
            u32::try_from(text.chars().count()).unwrap_or(1).max(1),
        ));
        self.ins(&format!("mov {tm}, {}", qstr(&text)));
        let n = self.temp(&Ty::Dec(10, 0));
        self.ins(&format!("dec.mov {n}, {}", pieces.len()));
        let mut args = vec![tl, tm, n];
        for e in &pieces {
            let ty = match self.ty(e) {
                Ok(t) => t,
                Err(err) => {
                    self.err(s.line, ids::EXPRESSION, err);
                    return;
                }
            };
            let desc = self.temp(&Ty::Char(16));
            let (code, len, dec) = match &ty {
                Ty::Char(n) => ('C', *n, 0),
                Ty::Dec(d, sc) => ('D', u32::from(*d), *sc),
                Ty::Lgl => ('L', 1, 0),
                Ty::Ptr => ('P', 16, 0),
            };
            self.ins(&format!(
                "mov {desc}, {}",
                qstr(&format!("{code}{len:05}{dec:02}"))
            ));
            let place = self.temp(&ty);
            if let Err(err) = self.into_place(&place, &ty, e) {
                self.err(s.line, ids::EXPRESSION, err);
                return;
            }
            args.push(desc);
            args.push(place);
        }
        self.ins(&format!("call QSYS/QCLEXEC({})", args.join(", ")));
    }

    /// `SNDPGMMSG`.
    fn sndpgmmsg(&mut self, s: &Stmt, r: &Resolved) {
        let get = |k: &str| r.get(k).and_then(|b| b.value.clone());
        let msgid = match get("MSGID") {
            Some(Value::Char(id)) => qstr(&id),
            Some(Value::Var(v)) => format!("${v}"),
            _ => qstr("CPF9898"),
        };
        let (msgf, msgf_lib) = match get("MSGF") {
            Some(Value::Qual(parts)) => (
                parts
                    .first()
                    .map_or_else(|| "QCPFMSG".into(), Value::source),
                parts.get(1).map_or_else(|| "*LIBL".into(), Value::source),
            ),
            _ => ("QCPFMSG".into(), "*LIBL".into()),
        };
        let data = match get("MSG").or_else(|| get("MSGDTA")) {
            Some(Value::Char(t)) => Some(qstr(&t)),
            Some(Value::Var(v)) => Some(format!("${v}")),
            Some(Value::Expr(toks)) => match crate::parse::expr::parse(&toks) {
                Ok(e) => match self.chr(&e).and_then(|c| Ok((c, self.ty(&e)?))) {
                    Ok((c, ty)) => Some(self.fixed(c, &ty)),
                    Err(err) => {
                        self.err(s.line, ids::EXPRESSION, err);
                        return;
                    }
                },
                Err(err) => {
                    self.err(s.line, ids::EXPRESSION, err);
                    return;
                }
            },
            _ => None,
        };
        let ty = match get("MSGTYPE") {
            Some(Value::Char(t)) => t,
            _ => "*INFO".into(),
        };
        let to = match get("TOPGMQ") {
            Some(Value::Elems(items)) => match items.first().cloned().flatten() {
                Some(v) => v.source(),
                None => "*PRV".into(),
            },
            Some(v) => v.source(),
            None => match get("TOMSGQ") {
                Some(Value::List(items))
                    if items.first().map(Value::source).as_deref() == Some("*SYSOPR") =>
                {
                    "*SYSOPR".into()
                }
                Some(v) if v.source() == "*SYSOPR" => "*SYSOPR".into(),
                Some(v) if v.source() == "*TOPGMQ" => "*PRV".into(),
                Some(v) => {
                    self.warn(
                        s.line,
                        ids::UNSUPPORTED,
                        format!(
                            "TOMSGQ({}) is sent to the external queue in this release",
                            v.source()
                        ),
                    );
                    "*EXT".into()
                }
                None => "*PRV".into(),
            },
        };
        let to = match to.as_str() {
            "*SAME" | "*PRV" | "*CTLBDY" | "*EXT" | "*SYSOPR" => to,
            other => {
                self.warn(
                    s.line,
                    ids::UNSUPPORTED,
                    format!("TOPGMQ({other}) is sent to the caller in this release"),
                );
                "*PRV".into()
            }
        };
        let key = match get("KEYVAR") {
            Some(Value::Var(v)) => format!(" key(${v})"),
            _ => String::new(),
        };
        let data = data.map(|d| format!(" data({d})")).unwrap_or_default();
        self.ins(&format!(
            "sndpgmmsg msgid({msgid}) msgf({}, {}){data} type({ty}) to({to}){key}",
            qstr(&msgf),
            qstr(&msgf_lib)
        ));
    }

    /// `RCVMSG`.
    fn rcvmsg(&mut self, s: &Stmt, r: &Resolved) {
        let get = |k: &str| r.get(k).and_then(|b| b.value.clone());
        let var = |v: Option<Value>| match v {
            Some(Value::Var(n)) => Some(n),
            _ => None,
        };
        let pgmq = match get("PGMQ") {
            Some(Value::Elems(items)) => items
                .first()
                .cloned()
                .flatten()
                .map_or_else(|| "*SAME".into(), |v| v.source()),
            Some(v) => v.source(),
            None => "*SAME".into(),
        };
        if get("MSGQ").is_some_and(|v| v.source() != "*PGMQ") {
            self.warn(
                s.line,
                ids::UNSUPPORTED,
                "RCVMSG MSGQ is received from the program's queue in this release",
            );
        }
        let pgmq = match pgmq.as_str() {
            "*SAME" | "*PRV" | "*EXT" | "*CTLBDY" => pgmq,
            _ => "*SAME".into(),
        };
        let ty = match get("MSGTYPE") {
            // `*EXCP` is the last exception: an escape here.
            Some(Value::Char(t)) if t == "*EXCP" => "*ESCAPE".into(),
            Some(Value::Char(t)) => t,
            _ => "*ANY".into(),
        };
        let rmv = !matches!(get("RMV"), Some(Value::Char(t)) if t == "*NO");
        let mut parts = vec![format!("pgmq({pgmq})"), format!("type({ty})")];
        if let Some(k) = var(get("MSGKEY")) {
            parts.push(format!("key(${k})"));
        }
        if rmv {
            parts.push("rmv".into());
        }
        for (kw, ir) in [
            ("MSG", "msg"),
            ("MSGID", "msgid"),
            ("MSGDTA", "data"),
            ("KEYVAR", "keyout"),
            ("SEV", "sev"),
        ] {
            if let Some(v) = var(get(kw)) {
                parts.push(format!("{ir}(${v})"));
            }
        }
        if get("WAIT").is_some_and(|v| v.source() != "0") {
            self.warn(
                s.line,
                ids::UNSUPPORTED,
                "RCVMSG WAIT does not wait in this release",
            );
        }
        let got = self.reg("g");
        self.ins(&format!("{got} = rcvmsg {}", parts.join(" ")));
        if let Some(v) = var(get("RTNTYPE")) {
            self.warn(
                s.line,
                ids::UNSUPPORTED,
                format!("RTNTYPE(&{v}) is not returned in this release"),
            );
        }
    }

    /// `RMVMSG`.
    fn rmvmsg(&mut self, r: &Resolved) {
        let get = |k: &str| r.get(k).and_then(|b| b.value.clone());
        let pgmq = match get("PGMQ") {
            Some(Value::Elems(items)) => items
                .first()
                .cloned()
                .flatten()
                .map_or_else(|| "*SAME".into(), |v| v.source()),
            Some(v) => v.source(),
            None => "*SAME".into(),
        };
        let pgmq = match pgmq.as_str() {
            "*SAME" | "*PRV" | "*EXT" | "*CTLBDY" => pgmq,
            _ => "*SAME".into(),
        };
        match get("MSGKEY") {
            Some(Value::Var(k)) => self.ins(&format!("rmvmsg pgmq({pgmq}) key(${k})")),
            _ => {
                let clear = match get("CLEAR") {
                    Some(Value::Char(c)) => c,
                    _ => "*ALL".into(),
                };
                self.ins(&format!("rmvmsg pgmq({pgmq}) clear({clear})"));
            }
        }
    }

    /// `SNDUSRMSG`: an inquiry reads its reply at the console, an
    /// informational message goes to the external queue.
    fn sndusrmsg(&mut self, s: &Stmt, r: &Resolved) {
        let get = |k: &str| r.get(k).and_then(|b| b.value.clone());
        let text = match get("MSG") {
            Some(Value::Char(t)) => qstr(&t),
            Some(Value::Var(v)) => format!("${v}"),
            _ => match get("MSGID") {
                Some(Value::Char(id)) => qstr(&id),
                _ => qstr("?"),
            },
        };
        let inq = matches!(get("MSGTYPE"), Some(Value::Char(t)) if t == "*INQ")
            || get("MSGTYPE").is_none();
        if inq {
            match get("MSGRPY") {
                Some(Value::Var(v)) => self.ins(&format!("dsply {text} reply(${v})")),
                _ => {
                    let t = self.temp(&Ty::Char(132));
                    self.ins(&format!("dsply {text} reply({t})"));
                }
            }
        } else {
            let d = self.temp(&Ty::Char(132));
            self.ins(&format!("mov {d}, {text}"));
            self.ins(&format!(
                "sndpgmmsg msgid(\"CPF9898\") msgf(\"QCPFMSG\") data({d}) type(*INFO) to(*EXT)"
            ));
        }
        let _ = s;
    }
}

/// Whether a bound value takes the direct path: constants and bare
/// variables only.
fn simple(v: Option<&Value>) -> bool {
    match v {
        None => true,
        Some(Value::Expr(_)) => false,
        Some(Value::Qual(parts)) => parts.iter().all(|p| simple(Some(p))),
        Some(Value::Elems(items)) => items.iter().all(|i| simple(i.as_ref())),
        Some(Value::List(items)) => items.iter().all(|i| simple(Some(i))),
        Some(_) => true,
    }
}

/// A token as command text with variables and expressions replaced by
/// `&n` placeholders, the expressions collected.
fn template(t: &Tok, pieces: &mut Vec<Expr>) -> String {
    match t {
        Tok::Var(v) => {
            pieces.push(Expr::Var(v.clone()));
            format!("&{}", pieces.len())
        }
        Tok::Builtin { .. } => match crate::parse::expr::parse(std::slice::from_ref(t)) {
            Ok(e) => {
                pieces.push(e);
                format!("&{}", pieces.len())
            }
            Err(_) => t.text(),
        },
        Tok::Paren(items) if cmd::analyze::expr::has_vars(items) => {
            match crate::parse::expr::parse(items) {
                Ok(e) if !items.iter().any(|i| matches!(i, Tok::Kw { .. })) => {
                    pieces.push(e);
                    format!("&{}", pieces.len())
                }
                _ => {
                    let inner: Vec<String> = items.iter().map(|i| template(i, pieces)).collect();
                    format!("({})", inner.join(" "))
                }
            }
        }
        Tok::Kw { name, pfx, args } => {
            let sel = pfx.map_or("", cmd::token::Sel::text);
            // A value that is one expression over variables becomes one
            // placeholder; anything else is templated piece by piece.
            let simple = args.len() > 1
                && cmd::analyze::expr::has_vars(args)
                && !args
                    .iter()
                    .any(|i| matches!(i, Tok::Kw { .. } | Tok::Paren(_)));
            if simple && let Ok(e) = crate::parse::expr::parse(args) {
                pieces.push(e);
                return format!("{sel}{name}(&{})", pieces.len());
            }
            let inner: Vec<String> = args.iter().map(|i| template(i, pieces)).collect();
            format!("{sel}{name}({})", inner.join(" "))
        }
        other => other.text(),
    }
}

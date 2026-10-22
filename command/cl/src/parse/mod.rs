// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Statements from their tokens: the CL control statements and
//! declarations are read here, every other command is kept for the
//! analyzer. Nested commands (`THEN`, `ELSE`, `EXEC`, `OTHERWISE`) are
//! statements of their own, parsed the same way.

pub mod expr;

use cmd::token::{Tok, Tokens, structure};

use crate::ast::{
    CallParm, CallTarget, DclF, Decl, DtaAra, Expr, FileOp, Kind, Lit, PrcParm, Program, Stg, Stmt,
    Target, VarType,
};
use crate::diag::{Diag, ids};
use crate::lex::Raw;

/// Parse the statements; diagnostics for what could not be read.
pub fn program(raws: &[Raw], errs: &mut Vec<Diag>) -> Program {
    let mut stmts = Vec::with_capacity(raws.len());
    for r in raws {
        match statement(&r.toks, r.n, r.line, &r.text) {
            Ok(s) => stmts.push(s),
            Err(e) => errs.push(Diag::at(r.line, ids::BAD_VALUE, e)),
        }
    }
    Program { stmts }
}

/// The keyword's arguments, if given.
fn kw<'a>(toks: &'a [Tok], name: &str) -> Option<&'a [Tok]> {
    toks.iter().find_map(|t| match t {
        Tok::Kw { name: n, args, .. } if n == name => Some(args.as_slice()),
        _ => None,
    })
}

/// The positional tokens (those before any keyword).
fn positional(toks: &[Tok]) -> &[Tok] {
    let n = toks
        .iter()
        .position(|t| matches!(t, Tok::Kw { .. }))
        .unwrap_or(toks.len());
    &toks[..n]
}

/// The keyword's arguments, or the `i`th positional token.
fn arg(toks: &[Tok], name: &str, i: usize) -> Option<Vec<Tok>> {
    if let Some(a) = kw(toks, name) {
        return Some(a.to_vec());
    }
    positional(toks).get(i).map(|t| match t {
        Tok::Paren(items) => items.clone(),
        other => vec![other.clone()],
    })
}

fn one_word(toks: &[Tok]) -> Option<String> {
    match toks {
        [one] => Some(one.text()),
        _ => None,
    }
}

fn var_name(toks: &[Tok]) -> Result<String, String> {
    match toks {
        [Tok::Var(v)] => Ok(v.clone()),
        other => Err(format!("{} is not a CL variable", Tok::join(other))),
    }
}

/// A nested command's tokens as a statement.
fn nested(toks: &[Tok], n: usize, line: usize) -> Result<Option<Box<Stmt>>, String> {
    if toks.is_empty() {
        return Ok(None);
    }
    let t = structure(toks.to_vec()).map_err(|e| e.to_string())?;
    if t.name.is_empty() {
        return Ok(None);
    }
    let text = Tok::join(toks);
    Ok(Some(Box::new(statement(&t, n, line, &text)?)))
}

/// A literal operand.
fn literal(t: &Tok) -> Result<Lit, String> {
    match t {
        Tok::Str(s) => Ok(Lit::Str(s.clone())),
        Tok::Num(n) => Ok(Lit::Num(n.clone())),
        Tok::Hex(b) => Ok(Lit::Hex(b.clone())),
        Tok::Word(w) => Ok(Lit::Str(w.clone())),
        other => Err(format!("{} is not a constant", other.text())),
    }
}

/// `&VAR`, `%SST(&VAR s l)` or `%BIN(&VAR s l)` as a target.
fn target(toks: &[Tok]) -> Result<Target, String> {
    match toks {
        [Tok::Var(v)] => Ok(Target::Var(v.clone())),
        [Tok::Builtin { name, args }]
            if matches!(name.as_str(), "SST" | "SUBSTRING" | "BIN" | "BINARY") =>
        {
            let v = match args.first() {
                Some(Tok::Var(v)) => v.clone(),
                _ => return Err(format!("%{name} needs a variable first")),
            };
            let (s, l) = match args.len() {
                3 => (expr::parse(&args[1..2])?, expr::parse(&args[2..3])?),
                // The whole variable: its own length, known later.
                1 if name.starts_with('B') => (
                    Expr::Lit(Lit::Num("1".into())),
                    Expr::Lit(Lit::Num("0".into())),
                ),
                _ => return Err(format!("%{name} takes a variable, a start and a length")),
            };
            if name.starts_with('B') {
                Ok(Target::Bin(v, Box::new(s), Box::new(l)))
            } else {
                Ok(Target::Sst(v, Box::new(s), Box::new(l)))
            }
        }
        other => Err(format!("{} cannot receive a value", Tok::join(other))),
    }
}

/// `LIB/NAME` into `(name, lib)`, `*LIBL` when unqualified.
fn qualified(toks: &[Tok]) -> Result<(String, String), String> {
    match toks {
        [Tok::Word(w)] => match w.split_once('/') {
            Some((l, n)) => Ok((n.to_string(), l.to_string())),
            None => Ok((w.clone(), "*LIBL".into())),
        },
        [Tok::Paren(items)] => qualified(items),
        [Tok::Word(n), Tok::Word(l)] => Ok((n.clone(), l.clone())),
        other => Err(format!("{} is not an object name", Tok::join(other))),
    }
}

/// `DTAARA((lib/name (start len)))` or `DTAARA(lib/name)`.
fn dtaara(toks: &[Tok]) -> Result<DtaAra, String> {
    let items: &[Tok] = match toks {
        [Tok::Paren(items)] => items,
        other => other,
    };
    let (name_tok, sub) = match items {
        [n] => (n, None),
        [n, Tok::Paren(se)] if se.len() == 2 => {
            (n, Some((expr::parse(&se[0..1])?, expr::parse(&se[1..2])?)))
        }
        [n, Tok::Paren(se)] if se.is_empty() => (n, None),
        [n, s, l] => (
            n,
            Some((
                expr::parse(std::slice::from_ref(s))?,
                expr::parse(std::slice::from_ref(l))?,
            )),
        ),
        other => return Err(format!("{} is not a data area", Tok::join(other))),
    };
    let (name, lib) = qualified(std::slice::from_ref(name_tok))?;
    Ok(DtaAra { name, lib, sub })
}

/// Parse one statement.
pub fn statement(t: &Tokens, n: usize, line: usize, text: &str) -> Result<Stmt, String> {
    let toks = &t.toks;
    let kind = match t.name.as_str() {
        "PGM" => Kind::Pgm(match kw(toks, "PARM") {
            Some(a) => a
                .iter()
                .map(|x| var_name(std::slice::from_ref(x)))
                .collect::<Result<_, _>>()?,
            None => Vec::new(),
        }),
        "ENDPGM" => Kind::EndPgm,
        "DCL" => Kind::Dcl(decl(toks)?),
        "DCLF" => {
            let (file, lib) = qualified(&arg(toks, "FILE", 0).ok_or("DCLF needs FILE")?)?;
            let rcdfmt = kw(toks, "RCDFMT")
                .map(|a| a.iter().map(Tok::text).filter(|f| f != "*ALL").collect())
                .unwrap_or_default();
            Kind::DclF(DclF {
                file,
                lib,
                rcdfmt,
                opnid: kw(toks, "OPNID").and_then(one_word),
            })
        }
        "DCLPRCOPT" => Kind::DclPrcOpt(
            toks.iter()
                .filter_map(|x| match x {
                    Tok::Kw { name, args, .. } => Some((name.clone(), args.clone())),
                    _ => None,
                })
                .collect(),
        ),
        "CHGVAR" => {
            let v = target(&arg(toks, "VAR", 0).ok_or("CHGVAR needs VAR")?)?;
            let e = expr::parse(&arg(toks, "VALUE", 1).ok_or("CHGVAR needs VALUE")?)?;
            Kind::ChgVar(v, e)
        }
        "IF" | "WHEN" => {
            let cond = expr::parse(&arg(toks, "COND", 0).ok_or("a condition is needed")?)?;
            let then = match kw(toks, "THEN") {
                Some(a) => nested(a, n, line)?,
                None => {
                    let pos = positional(toks);
                    nested(pos.get(1..).unwrap_or(&[]), n, line)?
                }
            };
            if t.name == "IF" {
                Kind::If(cond, then)
            } else {
                Kind::When(cond, then)
            }
        }
        "ELSE" | "OTHERWISE" => {
            let then = match kw(toks, "CMD") {
                Some(a) => nested(a, n, line)?,
                None => nested(positional(toks), n, line)?,
            };
            if t.name == "ELSE" {
                Kind::Else(then)
            } else {
                Kind::Otherwise(then)
            }
        }
        "DO" => Kind::Do,
        "ENDDO" => Kind::EndDo,
        "DOWHILE" => Kind::DoWhile(expr::parse(
            &arg(toks, "COND", 0).ok_or("DOWHILE needs COND")?,
        )?),
        "DOUNTIL" => Kind::DoUntil(expr::parse(
            &arg(toks, "COND", 0).ok_or("DOUNTIL needs COND")?,
        )?),
        "DOFOR" => Kind::DoFor(
            var_name(&arg(toks, "VAR", 0).ok_or("DOFOR needs VAR")?)?,
            expr::parse(&arg(toks, "FROM", 1).ok_or("DOFOR needs FROM")?)?,
            expr::parse(&arg(toks, "TO", 2).ok_or("DOFOR needs TO")?)?,
            match arg(toks, "BY", 3) {
                Some(a) => expr::parse(&a)?,
                None => Expr::Lit(Lit::Num("1".into())),
            },
        ),
        "LEAVE" => Kind::Leave(arg(toks, "CMDLBL", 0).and_then(|a| one_word(&a))),
        "ITERATE" => Kind::Iterate(arg(toks, "CMDLBL", 0).and_then(|a| one_word(&a))),
        "SELECT" => Kind::Select,
        "ENDSELECT" => Kind::EndSelect,
        "GOTO" => Kind::Goto(
            one_word(&arg(toks, "CMDLBL", 0).ok_or("GOTO needs a label")?)
                .ok_or("GOTO needs one label")?,
        ),
        "CALL" | "TFRCTL" => {
            let pgm = arg(toks, "PGM", 0).ok_or("CALL needs PGM")?;
            let target = match pgm.as_slice() {
                [Tok::Var(v)] => CallTarget::Var(v.clone()),
                other => {
                    let (name, lib) = qualified(other)?;
                    CallTarget::Name(name, lib)
                }
            };
            let parms = match arg(toks, "PARM", 1) {
                Some(a) => a.iter().map(call_parm).collect::<Result<_, _>>()?,
                None => Vec::new(),
            };
            if t.name == "CALL" {
                Kind::Call(target, parms)
            } else {
                Kind::TfrCtl(target, parms)
            }
        }
        "CALLPRC" => {
            let prc = match arg(toks, "PRC", 0).as_deref() {
                Some([Tok::Str(s)]) => s.clone(),
                Some([Tok::Word(w)]) => w.clone(),
                _ => return Err("CALLPRC needs PRC".into()),
            };
            let mut parms = Vec::new();
            if let Some(a) = arg(toks, "PARM", 1) {
                for p in a {
                    let (v, byval) = match &p {
                        Tok::Paren(items) => match items.as_slice() {
                            [v] => (expr::parse(std::slice::from_ref(v))?, false),
                            [v, Tok::Word(w)] if w == "*BYVAL" => {
                                (expr::parse(std::slice::from_ref(v))?, true)
                            }
                            [v, Tok::Word(w)] if w == "*BYREF" => {
                                (expr::parse(std::slice::from_ref(v))?, false)
                            }
                            other => (expr::parse(other)?, false),
                        },
                        other => (expr::parse(std::slice::from_ref(other))?, false),
                    };
                    parms.push(PrcParm { value: v, byval });
                }
            }
            let rtn = match kw(toks, "RTNVAL") {
                Some(a) if !matches!(a, [Tok::Word(w)] if w == "*NONE") => Some(target(a)?),
                _ => None,
            };
            Kind::CallPrc(prc, parms, rtn)
        }
        "RETURN" => Kind::Return,
        "MONMSG" => {
            let ids: Vec<String> = arg(toks, "MSGID", 0)
                .ok_or("MONMSG needs MSGID")?
                .iter()
                .map(Tok::text)
                .collect();
            let cmpdta = kw(toks, "CMPDTA").and_then(|a| match a {
                [Tok::Str(s)] => Some(s.clone()),
                [Tok::Word(w)] if w != "*NONE" => Some(w.clone()),
                _ => None,
            });
            let exec = match kw(toks, "EXEC") {
                Some(a) => nested(a, n, line)?,
                None => None,
            };
            Kind::MonMsg(ids, cmpdta, exec)
        }
        "SUBR" => Kind::Subr(
            one_word(&arg(toks, "SUBR", 0).ok_or("SUBR needs a name")?)
                .ok_or("SUBR needs one name")?,
        ),
        "ENDSUBR" => Kind::EndSubr(match arg(toks, "RTNVAL", 0) {
            Some(a) => Some(expr::parse(&a)?),
            None => None,
        }),
        "CALLSUBR" => Kind::CallSubr(
            one_word(&arg(toks, "SUBR", 0).ok_or("CALLSUBR needs SUBR")?)
                .ok_or("CALLSUBR needs one name")?,
            match kw(toks, "RTNVAL") {
                Some(a) => Some(target(a)?),
                None => None,
            },
        ),
        "RTNSUBR" => Kind::RtnSubr(match arg(toks, "RTNVAL", 0) {
            Some(a) => Some(expr::parse(&a)?),
            None => None,
        }),
        "RCVF" | "SNDF" | "SNDRCVF" | "WAIT" | "ENDRCV" => {
            let op = match t.name.as_str() {
                "RCVF" => FileOp::Rcvf,
                "SNDF" => FileOp::Sndf,
                "SNDRCVF" => FileOp::SndRcvf,
                "WAIT" => FileOp::Wait,
                _ => FileOp::EndRcv,
            };
            Kind::FileIo(
                op,
                kw(toks, "OPNID").and_then(one_word),
                kw(toks, "RCDFMT")
                    .and_then(one_word)
                    .filter(|f| f != "*FILE"),
            )
        }
        "CLOSE" => Kind::Close(arg(toks, "OPNID", 0).and_then(|a| one_word(&a))),
        "RTVDTAARA" => Kind::RtvDtaAra(
            dtaara(&arg(toks, "DTAARA", 0).ok_or("RTVDTAARA needs DTAARA")?)?,
            target(&arg(toks, "RTNVAR", 1).ok_or("RTVDTAARA needs RTNVAR")?)?,
        ),
        "CHGDTAARA" => Kind::ChgDtaAra(
            dtaara(&arg(toks, "DTAARA", 0).ok_or("CHGDTAARA needs DTAARA")?)?,
            expr::parse(&arg(toks, "VALUE", 1).ok_or("CHGDTAARA needs VALUE")?)?,
        ),
        "DMPCLPGM" => Kind::DmpClPgm,
        _ => Kind::Command(t.clone()),
    };
    Ok(Stmt {
        n,
        line,
        label: t.label.clone(),
        text: text.to_string(),
        kind,
    })
}

/// A `CALL` parameter.
fn call_parm(t: &Tok) -> Result<CallParm, String> {
    Ok(match t {
        Tok::Var(v) => CallParm::Var(v.clone()),
        Tok::Word(w) if w == "*N" => CallParm::Omit,
        Tok::Paren(items) => match items.as_slice() {
            [Tok::Var(v)] => CallParm::Var(v.clone()),
            other => CallParm::Expr(expr::parse(other)?),
        },
        other => CallParm::Lit(literal(other)?),
    })
}

/// `DCL VAR TYPE LEN VALUE STG DEFVAR BASPTR`.
fn decl(toks: &[Tok]) -> Result<Decl, String> {
    let var = var_name(&arg(toks, "VAR", 0).ok_or("DCL needs VAR")?)?;
    let ty = match arg(toks, "TYPE", 1).and_then(|a| one_word(&a)).as_deref() {
        Some("*CHAR") => VarType::Char,
        Some("*DEC") => VarType::Dec,
        Some("*LGL") => VarType::Lgl,
        Some("*INT") => VarType::Int,
        Some("*UINT") => VarType::Uint,
        Some("*PTR") => VarType::Ptr,
        Some(o) => return Err(format!("TYPE({o}) is not a CL type")),
        None => return Err("DCL needs TYPE".into()),
    };
    let (len, dec) = match arg(toks, "LEN", 2).as_deref() {
        None => (None, None),
        Some([Tok::Num(n)]) => (Some(n.parse().map_err(|_| "LEN is a number")?), None),
        Some([Tok::Num(n), Tok::Num(d)]) => (
            Some(n.parse().map_err(|_| "LEN is a number")?),
            Some(d.parse().map_err(|_| "decimal positions are a number")?),
        ),
        Some(other) => return Err(format!("LEN({}) is not valid", Tok::join(other))),
    };
    let value = match arg(toks, "VALUE", 3) {
        None => None,
        Some(a) => match a.as_slice() {
            [one] => Some(literal(one)?),
            other => return Err(format!("VALUE({}) is not a constant", Tok::join(other))),
        },
    };
    let stg = match kw(toks, "STG").and_then(one_word).as_deref() {
        None | Some("*AUTO") => Stg::Auto,
        Some("*DEFINED") => {
            let a = kw(toks, "DEFVAR").ok_or("STG(*DEFINED) needs DEFVAR")?;
            let (base, pos) = match a {
                [Tok::Var(b)] => (b.clone(), 1),
                [Tok::Var(b), Tok::Num(p)] => {
                    (b.clone(), p.parse().map_err(|_| "DEFVAR position")?)
                }
                _ => return Err("DEFVAR(&VAR position)".into()),
            };
            Stg::Defined(base, pos)
        }
        Some("*BASED") => Stg::Based(var_name(
            kw(toks, "BASPTR").ok_or("STG(*BASED) needs BASPTR")?,
        )?),
        Some(o) => return Err(format!("STG({o}) is not valid")),
    };
    Ok(Decl {
        var,
        ty,
        len,
        dec,
        value,
        stg,
    })
}

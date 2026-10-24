// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Free-form statements into syntax nodes: the declarations
//! (`CTL-OPT`, `DCL-F`, `DCL-S`, `DCL-C`, `DCL-DS`, `DCL-PR`, `DCL-PI`,
//! `DCL-PROC`, `DCL-ENUM` with their `END-` statements) and the
//! calculations, where an assignment is an implicit `EVAL` and a bare
//! call an implicit `CALLP`.

use crate::ast::{Arg, Def, DefKind, Expr, FileDecl, Keyword, Lit, Op, Pos, Proc, Stmt};
use crate::diag::{Diag, ids};
use crate::lex::{Kind, Tok};
use crate::parse::{Builder, FreeStmt, Item, expr, fixed, kw, ops};

/// The first word of a statement, with hyphenated words (`DCL-S`,
/// `ON-ERROR`, `EVAL-CORR`) joined when written without blanks; and
/// how many tokens it took.
fn head(toks: &[Tok]) -> (String, usize) {
    let Some(first) = toks.first() else {
        return (String::new(), 0);
    };
    let Kind::Name(w) = &first.kind else {
        return (String::new(), 0);
    };
    let mut word = w.clone();
    let mut n = 1;
    let mut end_col = first.at.col + u16::try_from(w.len()).unwrap_or(0);
    while let (Some(dash), Some(next)) = (toks.get(n), toks.get(n + 1)) {
        let joined = dash.is("-")
            && dash.at.line == first.at.line
            && dash.at.col == end_col
            && next.at.col == end_col + 1
            && next.name().is_some();
        if !joined {
            break;
        }
        let nw = next.name().unwrap_or_default();
        let candidate = format!("{word}-{nw}");
        if !is_hyphen_word(&candidate) {
            break;
        }
        word = candidate;
        end_col = next.at.col + u16::try_from(nw.len()).unwrap_or(0);
        n += 2;
    }
    (word, n)
}

/// Hyphenated words of the language.
fn is_hyphen_word(w: &str) -> bool {
    matches!(
        w,
        "CTL-OPT" | "DCL-F" | "DCL-S" | "DCL-C" | "DCL-DS" | "DCL-SUBF" | "END-DS" | "DCL-PR"
            | "END-PR" | "DCL-PI" | "END-PI" | "DCL-PARM" | "DCL-PROC" | "END-PROC" | "DCL-ENUM"
            | "END-ENUM" | "EVAL-CORR" | "ON-ERROR" | "ON-EXCP" | "ON-EXIT" | "SND-MSG" | "FOR-EACH"
            | "WHEN-IS" | "WHEN-IN" | "XML-INTO" | "XML-SAX" | "DATA-INTO" | "DATA-GEN" | "Z-ADD"
            | "Z-SUB" | "END-EXEC"
    )
}

/// The name of a declaration: a name or `*N`.
fn decl_name(t: Option<&Tok>) -> Option<String> {
    match t.map(|t| &t.kind) {
        Some(Kind::Name(n)) => Some(n.clone()),
        Some(Kind::Special(s)) if s == "*N" => Some("*N".into()),
        _ => None,
    }
}

/// Handle the statement at `k`; the index of the next item.
pub fn statement(b: &mut Builder<'_>, its: &[Item], k: usize, st: &FreeStmt) -> usize {
    let (word, n) = head(&st.toks);
    let rest = &st.toks[n..];
    match word.as_str() {
        "CTL-OPT" => {
            let kws = kw::keywords(rest, st.end, &mut b.diags);
            b.module.ctl.extend(kws);
            k + 1
        }
        "DCL-F" => {
            let Some(name) = decl_name(rest.first()) else {
                b.diags.push(Diag::new(ids::UNEXPECTED, 30, st.at).arg(word));
                return k + 1;
            };
            let kws = kw::keywords(&rest[1..], st.end, &mut b.diags);
            b.file(FileDecl {
                name,
                fixed: None,
                kws,
                at: st.at,
            });
            k + 1
        }
        "DCL-S" | "DCL-C" => {
            let Some(name) = decl_name(rest.first()) else {
                b.diags.push(Diag::new(ids::UNEXPECTED, 30, st.at).arg(word));
                return k + 1;
            };
            let mut d = Def {
                kind: if word == "DCL-S" {
                    DefKind::Standalone
                } else {
                    DefKind::Const
                },
                name,
                at: st.at,
                ..Def::default()
            };
            let body = &rest[1..];
            if d.kind == DefKind::Const {
                let is_kw = body.first().and_then(Tok::name) == Some("CONST") && body.get(1).is_some_and(|t| t.is("("));
                // The value, then any keywords (which a constant does not take).
                if is_kw {
                    let mut kws = kw::keywords(body, st.end, &mut b.diags);
                    d.value = kws.first().and_then(|k| k.arg(0)).and_then(|a| a.expr.clone());
                    kws.remove(0);
                    d.kws = kws;
                } else if !body.is_empty() {
                    let mut c = expr::Cursor::new(body, st.end);
                    d.value = expr::expr(&mut c, &mut b.diags);
                    d.kws = kw::keywords(c.rest(), st.end, &mut b.diags);
                }
            } else {
                d.kws = kw::keywords(body, st.end, &mut b.diags);
            }
            b.def(d);
            k + 1
        }
        "DCL-DS" => group(b, its, k, st, rest, DefKind::Ds, "END-DS"),
        "DCL-PR" => group(b, its, k, st, rest, DefKind::Pr, "END-PR"),
        "DCL-PI" => group(b, its, k, st, rest, DefKind::Pi, "END-PI"),
        "DCL-ENUM" => group(b, its, k, st, rest, DefKind::Enum, "END-ENUM"),
        "DCL-PROC" => {
            let Some(name) = decl_name(rest.first()) else {
                b.diags.push(Diag::new(ids::UNEXPECTED, 30, st.at).arg(word));
                return k + 1;
            };
            if let Some(open) = b.proc.take() {
                b.diags.push(Diag::new(ids::PROC_END_MISSING, 20, open.at));
                b.module.procs.push(open);
            }
            b.after_proc = true;
            let kws = kw::keywords(&rest[1..], st.end, &mut b.diags);
            b.proc = Some(Proc {
                name,
                kws,
                at: st.at,
                ..Proc::default()
            });
            k + 1
        }
        "END-PROC" => {
            b.end_proc(st.at);
            k + 1
        }
        "END-DS" | "END-PR" | "END-PI" | "END-ENUM" => {
            b.diags.push(Diag::new(ids::END_NOT_EXPECTED, 20, st.at).arg(word));
            k + 1
        }
        "DCL-SUBF" | "DCL-PARM" => {
            b.diags.push(Diag::new(ids::BLOCK_MISMATCH, 30, st.at).arg(word));
            k + 1
        }
        _ => {
            if let Some(s) = calc(b, st, &word, n) {
                b.stmt(s);
            }
            k + 1
        }
    }
}

/// A `DCL-DS`, `DCL-PR`, `DCL-PI` or `DCL-ENUM` group: the header,
/// then children up to the `END-` statement, unless the header ends
/// the group itself (`END-DS` in it, or `LIKEDS`/`LIKEREC`).
fn group(b: &mut Builder<'_>, its: &[Item], k: usize, st: &FreeStmt, rest: &[Tok], kind: DefKind, end_word: &str) -> usize {
    let Some(name) = decl_name(rest.first()) else {
        b.diags.push(Diag::new(ids::UNEXPECTED, 30, st.at));
        return k + 1;
    };
    let mut body = &rest[1..];
    // `END-xx` at the end of the header closes the group at once.
    let mut closed = false;
    if let Some(pos) = end_in(body, end_word) {
        closed = true;
        body = &body[..pos];
    }
    let mut d = Def {
        kind,
        name,
        at: st.at,
        ..Def::default()
    };
    d.kws = kw::keywords(body, st.end, &mut b.diags);
    if kind == DefKind::Ds && (d.kw("LIKEDS").is_some() || d.kw("LIKEREC").is_some()) {
        closed = true;
    }
    let mut j = k + 1;
    while !closed {
        let Some(Item::Free(child)) = its.get(j) else {
            b.diags.push(Diag::new(ids::GROUP_END_MISSING, 20, st.at).arg(end_word.to_string()));
            break;
        };
        let (w, n) = head(&child.toks);
        if w == end_word {
            j += 1;
            closed = true;
            continue;
        }
        // A structure inside a structure, one line with LIKEDS or
        // LIKEREC, is a subfield.
        let nested = kind == DefKind::Ds
            && w == "DCL-DS"
            && child.toks[n..].iter().any(|t| matches!(t.text().to_ascii_uppercase().as_str(), "LIKEDS" | "LIKEREC"));
        // A structure inside a structure, a group of its own: its
        // subfields become a template structure the subfield is LIKEDS,
        // with the group's DIM.
        if kind == DefKind::Ds && w == "DCL-DS" && !nested {
            let inner = child.toks[n..].to_vec();
            let iname = decl_name(inner.first()).unwrap_or_default();
            j = group(b, its, j, child, &inner, DefKind::Ds, "END-DS");
            let tname = format!("_NEST_{}_{}", d.name.to_ascii_uppercase(), iname.to_ascii_uppercase());
            let mut sub_kws = Vec::new();
            if let Some(t) = b.last_group().filter(|t| t.name.eq_ignore_ascii_case(&iname)) {
                t.name.clone_from(&tname);
                let (moved, kept): (Vec<_>, Vec<_>) = std::mem::take(&mut t.kws).into_iter().partition(|k| k.name == "DIM");
                t.kws = kept;
                for word in ["TEMPLATE", "QUALIFIED"] {
                    t.kws.push(Keyword { name: word.into(), args: Vec::new(), parens: false, at: child.at });
                }
                sub_kws = moved;
            }
            sub_kws.push(Keyword {
                name: "LIKEDS".into(),
                args: vec![Arg {
                    toks: Vec::new(),
                    expr: Some(Expr::Ref { name: tname, steps: Vec::new(), at: child.at }),
                }],
                parens: true,
                at: child.at,
            });
            d.children.push(Def {
                kind: DefKind::Subfield,
                name: iname,
                at: child.at,
                kws: sub_kws,
                ..Def::default()
            });
            continue;
        }
        // A declaration other than a child ends the group by mistake.
        if !nested && (w.starts_with("DCL-") && w != "DCL-SUBF" && w != "DCL-PARM" || w == "END-PROC") {
            b.diags.push(Diag::new(ids::GROUP_END_MISSING, 20, st.at).arg(end_word.to_string()));
            break;
        }
        let toks = if w == "DCL-SUBF" || w == "DCL-PARM" || nested { &child.toks[n..] } else { &child.toks[..] };
        if toks.is_empty() {
            b.diags.push(Diag::new(ids::UNEXPECTED, 30, child.at));
            j += 1;
            continue;
        }
        // A name that is not one is blanks; the rest are keywords.
        let cname = decl_name(toks.first()).unwrap_or_else(|| {
            b.diags.push(Diag::new(ids::BAD_DEF_NAME, 20, toks[0].at));
            String::new()
        });
        let ckind = match kind {
            DefKind::Ds => DefKind::Subfield,
            DefKind::Enum => DefKind::Const,
            _ => DefKind::Parm,
        };
        let mut c = Def {
            kind: ckind,
            name: cname,
            at: child.at,
            ..Def::default()
        };
        if kind == DefKind::Enum {
            c.value = expr::expr_all(&toks[1..], child.end, &mut b.diags);
        } else {
            c.kws = kw::keywords(&toks[1..], child.end, &mut b.diags);
        }
        d.children.push(c);
        j += 1;
    }
    b.def(d);
    j
}

/// Where `END-xx` begins in a header's tokens, if it ends it.
fn end_in(toks: &[Tok], end_word: &str) -> Option<usize> {
    let (a, z) = end_word.split_once('-')?;
    (0..toks.len()).find(|&i| {
        toks[i].name() == Some(a)
            && toks.get(i + 1).is_some_and(|t| t.is("-"))
            && toks.get(i + 2).and_then(Tok::name) == Some(z)
            && toks.len() <= i + 4
    })
}

/// The extenders after an operation code, `(E)`, when the parenthesis
/// holds only letters: the letters and the tokens they took.
fn extenders(toks: &[Tok]) -> Option<(String, usize)> {
    if !toks.first()?.is("(") {
        return None;
    }
    let letters = toks.get(1)?.name()?;
    if !toks.get(2)?.is(")") || !letters.chars().all(|c| "EHNPDTZMRA".contains(c)) {
        return None;
    }
    Some((letters.to_string(), 3))
}

/// Whether the tokens hold an assignment operator at the top level.
fn has_assignment(toks: &[Tok]) -> bool {
    let mut depth = 0;
    for t in toks {
        if t.is("(") {
            depth += 1;
        } else if t.is(")") {
            depth -= 1;
        } else if depth == 0 && ["=", "+=", "-=", "*=", "/=", "**="].iter().any(|p| t.is(p)) {
            return true;
        }
    }
    false
}

/// The source text from one position to another, for embedded SQL.
fn source_between(b: &Builder<'_>, from: Pos, to: Pos) -> String {
    let mut out = String::new();
    for l in b.lines.iter().filter(|l| l.no >= from.line && l.no <= to.line) {
        let (col0, area) = l.free_area();
        let chars: Vec<char> = area.chars().collect();
        let start = if l.no == from.line { usize::from(from.col.saturating_sub(col0)) } else { 0 };
        let end = if l.no == to.line {
            usize::from(to.col.saturating_sub(col0)).min(chars.len())
        } else {
            chars.len()
        };
        let piece: String = chars.get(start..end.max(start)).map_or(String::new(), |s| s.iter().collect());
        let piece = match piece.find("//") {
            Some(i) if !piece[..i].contains('\'') => piece[..i].to_string(),
            _ => piece,
        };
        if l.no > from.line {
            out.push('\n');
        }
        out.push_str(piece.trim());
    }
    out.trim_matches(' ').to_string()
}

/// A calculation statement.
fn calc(b: &mut Builder<'_>, st: &FreeStmt, word: &str, n: usize) -> Option<Stmt> {
    let toks = &st.toks;
    // EXEC SQL keeps its text for the precompiler.
    if word == "EXEC" && toks.get(1).and_then(Tok::name) == Some("SQL") {
        let from = toks.get(2).map_or(st.end, |t| t.at);
        // One line break per source line, from the statement's first
        // line, so the precompiler can tell lines apart.
        let text = if toks.len() > 2 {
            let lead = from.line.saturating_sub(st.at.line) as usize;
            format!("{}{}", "\n".repeat(lead), source_between(b, from, st.end))
        } else {
            String::new()
        };
        return Some(Stmt {
            at: st.at,
            op: Op {
                code: "EXEC SQL".into(),
                f2: vec![Expr::Lit(Lit::Char(text), from)],
                ..Op::default()
            },
            ..Stmt::default()
        });
    }
    let is_op = !word.is_empty() && ops::known(word) && {
        let after = &toks[n..];
        let takes_expr = ops::free_takes_expression(word) || matches!(word, "EVAL" | "EVALR" | "EVAL-CORR" | "FOR");
        let assigns_to_word = after
            .first()
            .is_some_and(|t| ["=", "+=", "-=", "*=", "/=", "**=", "."].iter().any(|p| t.is(p)));
        // `in(3) = x` assigns to an array named like an operation;
        // `chain(e) k f` and `if (a) = b` are operations.
        let indexed_target =
            after.first().is_some_and(|t| t.is("(")) && extenders(after).is_none() && has_assignment(after);
        !assigns_to_word && (takes_expr || !indexed_target)
    };
    if !is_op {
        // An assignment is an implicit EVAL, anything else a CALLP.
        let mut c = expr::Cursor::new(toks, st.end);
        let (code, x) = if has_assignment(toks) {
            ("EVAL", expr::assignment(&mut c, &mut b.diags))
        } else {
            ("CALLP", expr::expr(&mut c, &mut b.diags))
        };
        if !c.done() {
            b.diags.push(Diag::new(ids::END_OF_EXPRESSION, 20, c.pos()));
        }
        let x = x?;
        if code == "CALLP" && !matches!(x, Expr::Ref { .. }) {
            b.diags.push(Diag::new(ids::NOT_ASSIGNABLE, 30, st.at));
            return None;
        }
        return Some(Stmt {
            at: st.at,
            op: Op {
                code: code.into(),
                xf2: Some(x),
                ..Op::default()
            },
            ..Stmt::default()
        });
    }
    let mut op = Op {
        code: word.to_string(),
        ..Op::default()
    };
    let mut rest = &toks[n..];
    if let Some((e, m)) = extenders(rest) {
        op.ext = e;
        rest = &rest[m..];
    }
    if ops::fixed_only(word) {
        b.diags.push(Diag::new(ids::NOT_IN_FREE, 30, st.at).arg(word.to_string()));
        return None;
    }
    if matches!(word, "EVAL" | "EVALR" | "EVAL-CORR" | "FOR") || ops::free_takes_expression(word) {
        let (x, more) = fixed::extended_operand(word, rest, st.end, &mut b.diags);
        op.xf2 = x;
        op.more = more;
    } else {
        let mut c = expr::Cursor::new(rest, st.end);
        let mut list = Vec::new();
        let keyed = matches!(word, "CHAIN" | "SETLL" | "SETGT" | "READE" | "READPE" | "DELETE");
        while !c.done() {
            // A search argument in parentheses is a key list, even of one.
            let paren = keyed && list.is_empty() && c.peek().is_some_and(|t| t.is("("));
            let at = c.pos();
            if let Some(e) = expr::expr(&mut c, &mut b.diags) {
                let e = if paren && !matches!(e, Expr::List(..)) { Expr::List(vec![e], at) } else { e };
                list.push(e);
            } else {
                c.skip_all();
            }
        }
        ops::assign_free_roles(&mut op, list);
    }
    Some(Stmt {
        at: st.at,
        op,
        ..Stmt::default()
    })
}

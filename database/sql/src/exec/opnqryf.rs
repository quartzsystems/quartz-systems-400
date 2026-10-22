// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `OPNQRYF`: the `QRYSLT` expression grammar mapped onto the SQL
//! executor, the selected records put in `KEYFLD` order, and the result
//! left as the job's shared open data path over the file, which the
//! program's own open with `SHARE(*YES)` then reads. `CLOF` closes it.
//! `FORMAT`, `MAPFLD`, `GRPFLD`, `JFLD` and `OPTIMIZE` are not taken.

use db::format::FormatDef;
use db::odp::{OpenOpts, Usage};
use db::path::PathKey;
use dds::{FileRef, KeyField, KeyTransform};
use qsname::Name;
use store::txn::Txn;

use super::eval::Env;
use super::query::matching_rows;
use super::{SqlContext, TableInfo};
use crate::ast::{CmpOp, ColumnRef, Expr, Function, Ident, Literal, ObjectName};
use crate::error::{Result, SqlError};

/// What `OPNQRYF` takes.
#[derive(Clone, Debug)]
pub struct OpnQryF {
    /// `FILE`.
    pub file: ObjectName,
    /// `QRYSLT`, `*ALL` for every record.
    pub qryslt: String,
    /// `KEYFLD`: field names, each with `*DESCEND`.
    pub keyfld: Vec<(String, bool)>,
    /// `OPTION`.
    pub usage: Usage,
}

/// Open the query: how many records it selected.
pub fn opnqryf(txn: &mut Txn<'_>, ctx: &mut SqlContext, q: &OpnQryF) -> Result<usize> {
    let t = ctx.resolve_table(txn, &q.file)?;
    let cond = selection(&q.qryslt)?;
    let alias = q.file.name.value.clone();
    let (schema, rows) = matching_rows(txn, ctx, &[], &t, &alias, cond.as_ref(), &Env::default())?;
    let fmt = key_format(&t, &q.keyfld)?;
    let key_cols: Vec<(usize, usize)> = fmt
        .format
        .keys
        .iter()
        .map(|k| {
            let name = fmt.format.fields[k.field].name.to_string();
            let col = schema
                .cols
                .iter()
                .position(|c| !c.hidden && c.name == name)
                .expect("a field of the format");
            (col, k.field)
        })
        .collect();
    let mut plan = Vec::with_capacity(rows.len());
    for r in &rows {
        let rrn = u64::try_from(r[0].as_i64()?).unwrap_or(0);
        let mut vals = Vec::with_capacity(key_cols.len());
        for (col, field) in &key_cols {
            vals.push(r[*col].to_field(&fmt.format.fields[*field])?);
        }
        let key = db::path::values_key(&fmt, &vals).map_err(|e| SqlError::Conversion {
            what: e.to_string(),
        })?;
        plan.push(PathKey {
            key,
            format: 0,
            rrn,
        });
    }
    plan.sort_by(|a, b| a.key.cmp(&b.key).then(a.rrn.cmp(&b.rrn)));
    let n = plan.len();
    let opts = OpenOpts {
        usage: q.usage,
        share: true,
        commit: ctx.session.commit_def().is_some() && q.usage != Usage::INPUT,
        ..OpenOpts::input(ctx.job)
    };
    let id = t.id;
    let file = FileRef {
        library: qsname::LibSpec::Name(t.desc.library_name),
        file: t.desc.name(),
        format: None,
    };
    ctx.session
        .open_query(txn, &file, &mut |_| Ok(id), opts, plan, fmt)?;
    Ok(n)
}

/// `CLOF`: how many shared paths over the file closed.
pub fn clof(txn: &mut Txn<'_>, ctx: &mut SqlContext, file: &ObjectName) -> Result<usize> {
    let name = SqlContext::sys_name(&file.name)?;
    Ok(ctx.session.clof(txn, name))
}

/// The record format with `KEYFLD` as its keys, so the selected records
/// sort by encoded key bytes; `*NONE` keeps arrival order.
fn key_format(t: &TableInfo, keyfld: &[(String, bool)]) -> Result<FormatDef> {
    let mut rf = t.format.format.clone();
    rf.keys.clear();
    for (name, descend) in keyfld {
        let n = Name::parse(name).map_err(|e| SqlError::Conversion {
            what: format!("KEYFLD({name}): {e}"),
        })?;
        let (field, f) = rf
            .field(&n)
            .ok_or_else(|| SqlError::ColumnNotFound { name: name.clone() })?;
        rf.keys.push(KeyField {
            name: n,
            field,
            descend: *descend,
            transform: if f.data_type.is_numeric() {
                KeyTransform::Signed
            } else {
                KeyTransform::Unsigned
            },
            noaltseq: false,
            line: 0,
        });
    }
    FormatDef::new(rf, None, None).map_err(|e| SqlError::Conversion {
        what: e.to_string(),
    })
}

// The QRYSLT grammar: `field *EQ "text"`, `*AND`, `*OR`, `*NOT`,
// parentheses, `%RANGE`, `%VALUES`, `%WLDCRD`, `%SST`, `*CT`.

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Word(String),
    Str(String),
    Num(String),
    Open,
    Close,
}

fn lex(text: &str) -> Result<Vec<Tok>> {
    let bad = |what: String| SqlError::Syntax {
        line: 0,
        col: 0,
        found: what,
        expected: "a QRYSLT expression".into(),
    };
    let mut out = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            ' ' => i += 1,
            '(' => {
                out.push(Tok::Open);
                i += 1;
            }
            ')' => {
                out.push(Tok::Close);
                i += 1;
            }
            '"' | '\'' => {
                let quote = c;
                let mut s = String::new();
                i += 1;
                loop {
                    let Some(&d) = chars.get(i) else {
                        return Err(bad("an unterminated string".into()));
                    };
                    i += 1;
                    if d == quote {
                        if chars.get(i) == Some(&quote) {
                            s.push(quote);
                            i += 1;
                        } else {
                            break;
                        }
                    } else {
                        s.push(d);
                    }
                }
                out.push(Tok::Str(s));
            }
            _ => {
                let start = i;
                while i < chars.len() && !matches!(chars[i], ' ' | '(' | ')' | '"' | '\'') {
                    i += 1;
                }
                let w: String = chars[start..i].iter().collect();
                let numeric = w
                    .trim_start_matches(['+', '-'])
                    .chars()
                    .all(|ch| ch.is_ascii_digit() || ch == '.')
                    && w.chars().any(|ch| ch.is_ascii_digit());
                out.push(if numeric {
                    Tok::Num(w)
                } else {
                    Tok::Word(w.to_ascii_uppercase())
                });
            }
        }
    }
    Ok(out)
}

/// The selection parsed; `None` for `*ALL`. `selection` is the one to use.
fn qryslt(text: &str) -> Result<Option<Expr>> {
    let t = text.trim();
    if t.is_empty() || t.eq_ignore_ascii_case("*ALL") {
        return Ok(None);
    }
    let toks = lex(t)?;
    let mut p = Parser { toks, at: 0 };
    let e = p.or()?;
    if p.at != p.toks.len() {
        return Err(p.unexpected("the end of the selection"));
    }
    Ok(Some(e))
}

struct Parser {
    toks: Vec<Tok>,
    at: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.at)
    }

    fn word_is(&self, w: &str) -> bool {
        matches!(self.peek(), Some(Tok::Word(x)) if x == w)
    }

    fn unexpected(&self, expected: &str) -> SqlError {
        SqlError::Syntax {
            line: 0,
            col: u32::try_from(self.at).unwrap_or(0),
            found: match self.peek() {
                Some(Tok::Word(w) | Tok::Num(w)) => w.clone(),
                Some(Tok::Str(s)) => format!("\"{s}\""),
                Some(Tok::Open) => "(".into(),
                Some(Tok::Close) => ")".into(),
                None => "the end".into(),
            },
            expected: expected.into(),
        }
    }

    fn expect(&mut self, t: &Tok, what: &str) -> Result<()> {
        if self.peek() == Some(t) {
            self.at += 1;
            Ok(())
        } else {
            Err(self.unexpected(what))
        }
    }

    fn or(&mut self) -> Result<Expr> {
        let mut e = self.and()?;
        while self.word_is("*OR") {
            self.at += 1;
            let r = self.and()?;
            e = Expr::Or(Box::new(e), Box::new(r));
        }
        Ok(e)
    }

    fn and(&mut self) -> Result<Expr> {
        let mut e = self.not()?;
        while self.word_is("*AND") {
            self.at += 1;
            let r = self.not()?;
            e = Expr::And(Box::new(e), Box::new(r));
        }
        Ok(e)
    }

    fn not(&mut self) -> Result<Expr> {
        if self.word_is("*NOT") {
            self.at += 1;
            return Ok(Expr::Not(Box::new(self.not()?)));
        }
        self.comparison()
    }

    fn comparison(&mut self) -> Result<Expr> {
        if self.peek() == Some(&Tok::Open) {
            // A parenthesized expression, unless it is a single operand
            // followed by an operator.
            let save = self.at;
            self.at += 1;
            if let Ok(e) = self.or()
                && self.peek() == Some(&Tok::Close)
            {
                self.at += 1;
                return Ok(e);
            }
            self.at = save;
        }
        let left = self.operand()?;
        let op = match self.peek() {
            Some(Tok::Word(w)) => w.clone(),
            _ => return Err(self.unexpected("a comparison operator such as *EQ")),
        };
        self.at += 1;
        let cmp = |op: CmpOp, l: Expr, r: Expr| Expr::Compare(Box::new(l), op, Box::new(r), None);
        Ok(match op.as_str() {
            "*EQ" => cmp(CmpOp::Eq, left, self.right()?),
            "*NE" => cmp(CmpOp::Ne, left, self.right()?),
            "*GT" => cmp(CmpOp::Gt, left, self.right()?),
            "*LT" => cmp(CmpOp::Lt, left, self.right()?),
            "*GE" | "*NL" => cmp(CmpOp::Ge, left, self.right()?),
            "*LE" | "*NG" => cmp(CmpOp::Le, left, self.right()?),
            "*CT" => {
                let text = self.text("the text *CT looks for")?;
                Expr::Like(
                    Box::new(left),
                    Box::new(Expr::Literal(Literal::Str(format!("%{text}%")))),
                    None,
                    false,
                )
            }
            _ => return Err(self.unexpected("a comparison operator such as *EQ")),
        })
    }

    /// The right side of a comparison: an operand, or `%RANGE`,
    /// `%VALUES` or `%WLDCRD` after `*EQ`, which make their own test.
    fn right(&mut self) -> Result<Expr> {
        self.operand()
    }

    fn text(&mut self, what: &str) -> Result<String> {
        match self.peek().cloned() {
            Some(Tok::Str(s)) => {
                self.at += 1;
                Ok(s)
            }
            _ => Err(self.unexpected(what)),
        }
    }

    fn operand(&mut self) -> Result<Expr> {
        match self.peek().cloned() {
            Some(Tok::Str(s)) => {
                self.at += 1;
                Ok(Expr::Literal(Literal::Str(s)))
            }
            Some(Tok::Num(n)) => {
                self.at += 1;
                Ok(Expr::Literal(Literal::Num(n)))
            }
            Some(Tok::Word(w)) if w.starts_with('%') => {
                self.at += 1;
                self.function(&w)
            }
            Some(Tok::Word(w)) if !w.starts_with('*') => {
                self.at += 1;
                Ok(column(&w))
            }
            _ => Err(self.unexpected("a field, a constant or a %function")),
        }
    }

    /// `%SST(field start len)`; the others only follow `*EQ`.
    fn function(&mut self, name: &str) -> Result<Expr> {
        self.expect(&Tok::Open, "( after the function")?;
        let mut args = Vec::new();
        while self.peek() != Some(&Tok::Close) {
            args.push(self.operand()?);
        }
        self.at += 1;
        match name {
            "%SST" | "%SUBSTRING" if args.len() == 3 => Ok(Expr::Function(Function {
                name: "SUBSTR".into(),
                distinct: false,
                star: false,
                args,
            })),
            "%RANGE" | "%VALUES" | "%WLDCRD" => Ok(Expr::Function(Function {
                name: name.into(),
                distinct: false,
                star: false,
                args,
            })),
            _ => Err(SqlError::Unsupported {
                what: format!("QRYSLT function {name}"),
            }),
        }
    }
}

fn column(name: &str) -> Expr {
    Expr::Column(ColumnRef {
        table: None,
        column: Ident::new(name),
    })
}

/// `*EQ %RANGE(a b)`, `*EQ %VALUES(...)` and `*EQ %WLDCRD("a*b")` are
/// rewritten into `BETWEEN`, `IN` and `LIKE` after parsing.
fn rewrite(expr: Expr) -> Expr {
    match expr {
        Expr::Compare(left, CmpOp::Eq, right, None) => match *right {
            Expr::Function(f) if f.name == "%RANGE" && f.args.len() == 2 => {
                let mut a = f.args.into_iter();
                Expr::Between(
                    left,
                    Box::new(a.next().expect("two")),
                    Box::new(a.next().expect("two")),
                    false,
                )
            }
            Expr::Function(f) if f.name == "%VALUES" => Expr::InList(left, f.args, false),
            Expr::Function(f) if f.name == "%WLDCRD" => {
                let pat = match f.args.first() {
                    Some(Expr::Literal(Literal::Str(s))) => s.replace('*', "%"),
                    _ => String::new(),
                };
                Expr::Like(
                    left,
                    Box::new(Expr::Literal(Literal::Str(pat))),
                    None,
                    false,
                )
            }
            other => Expr::Compare(left, CmpOp::Eq, Box::new(other), None),
        },
        Expr::And(a, b) => Expr::And(Box::new(rewrite(*a)), Box::new(rewrite(*b))),
        Expr::Or(a, b) => Expr::Or(Box::new(rewrite(*a)), Box::new(rewrite(*b))),
        Expr::Not(a) => Expr::Not(Box::new(rewrite(*a))),
        other => other,
    }
}

/// The names `KEYFLD((F1) (F2 *DESCEND))` lists.
#[must_use]
pub fn keyfld(text: &str) -> Vec<(String, bool)> {
    let mut out = Vec::new();
    for group in text.split(')') {
        let g = group.trim().trim_start_matches('(').trim();
        if g.is_empty() || g.eq_ignore_ascii_case("*NONE") {
            continue;
        }
        let mut words = g.split_whitespace();
        let Some(name) = words.next() else { continue };
        let descend = words.any(|w| w.eq_ignore_ascii_case("*DESCEND"));
        out.push((name.to_ascii_uppercase(), descend));
    }
    out
}

/// `qryslt` with the `*EQ %function` forms rewritten.
pub fn selection(text: &str) -> Result<Option<Expr>> {
    Ok(qryslt(text)?.map(rewrite))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grammar() {
        let e = selection("CUSNO *EQ 3 *AND (NAME *EQ \"Bob\" *OR NAME *CT \"ar\")")
            .unwrap()
            .unwrap();
        assert!(matches!(e, Expr::And(_, _)));
        let e = selection("BAL *EQ %RANGE(1 100)").unwrap().unwrap();
        assert!(matches!(e, Expr::Between(..)));
        let e = selection("NAME *EQ %VALUES(\"A\" \"B\")").unwrap().unwrap();
        assert!(matches!(e, Expr::InList(_, v, false) if v.len() == 2));
        let e = selection("NAME *EQ %WLDCRD(\"A*\")").unwrap().unwrap();
        assert!(matches!(e, Expr::Like(..)));
        let e = selection("*NOT %SST(NAME 1 1) *EQ \"A\"").unwrap().unwrap();
        assert!(matches!(e, Expr::Not(_)));
        assert!(selection("*ALL").unwrap().is_none());
        assert!(selection("CUSNO *EQ").is_err());
        assert_eq!(
            keyfld("(CUSNO) (NAME *DESCEND)"),
            [("CUSNO".to_string(), false), ("NAME".to_string(), true)]
        );
    }
}

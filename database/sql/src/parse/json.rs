// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The JSON functions' clauses and `IS JSON`. Each call becomes a
//! function whose first argument is its clauses
//! (`crate::exec::json::Opts`), whose second is the `RETURNING` type as a
//! `CAST` of `NULL`, and whose others are its operands.

use super::Parser;
use crate::ast::{Expr, Function, Ident, Literal, ObjectName, TableRef, TypeSpec};
use crate::error::Result;
use crate::exec::json::Opts;

/// Calls whose result is JSON text: a value of one in a constructor is
/// `FORMAT JSON` without saying so.
const MAKES_JSON: &[&str] = &[
    "JSON_OBJECT",
    "JSON_ARRAY",
    "JSON_QUERY",
    "JSON_ARRAYAGG",
    "JSON_OBJECTAGG",
    "BSON_TO_JSON",
];

fn null() -> Expr {
    Expr::Literal(Literal::Null)
}

fn made(name: &str, opts: &Opts, ret: Option<TypeSpec>, operands: Vec<Expr>) -> Expr {
    let mut args = vec![
        Expr::Literal(Literal::Str(opts.text())),
        ret.map_or_else(null, |t| Expr::Cast(Box::new(null()), t)),
    ];
    args.extend(operands);
    Expr::Function(Function {
        name: name.into(),
        distinct: false,
        star: false,
        args,
    })
}

impl Parser {
    /// A JSON function's call after its `(`; `None` for other names.
    pub(crate) fn json_call(&mut self, name: &str) -> Result<Option<Expr>> {
        Ok(Some(match name {
            "JSON_OBJECT" => self.json_object()?,
            "JSON_ARRAY" => self.json_array()?,
            "JSON_VALUE" => self.json_value()?,
            "JSON_QUERY" => self.json_query()?,
            "JSON_EXISTS" => self.json_exists()?,
            "JSON_ARRAYAGG" => self.json_arrayagg()?,
            "JSON_OBJECTAGG" => self.json_objectagg()?,
            "XMLPARSE" => self.xml_parse()?,
            "XMLSERIALIZE" => self.xml_serialize()?,
            "JSON_TABLE" | "XMLQUERY" | "XMLEXISTS" | "XMLTABLE" | "XMLVALIDATE" | "XMLAGG"
            | "XMLELEMENT" | "XMLFOREST" | "XMLCONCAT" | "XMLATTRIBUTES" | "XMLDOCUMENT"
            | "XMLTEXT" | "XMLPI" | "XMLCOMMENT" | "XMLNAMESPACES" | "XMLGROUP" | "XMLROW"
            | "XSLTRANSFORM" => {
                return Err(Self::unsupported(&match name {
                    "JSON_TABLE" => "JSON_TABLE outside FROM".to_string(),
                    n => format!("the {n} function"),
                }));
            }
            // Not on IBM i 7.5 (pub400 answers SQL0204).
            "JSON_UPDATE" => {
                return Err(crate::error::SqlError::ObjectNotFound {
                    name: "JSON_UPDATE".into(),
                    kind: "function",
                });
            }
            _ => return Ok(None),
        }))
    }

    /// `FORMAT JSON | BSON` after a value: `J`, `B`, or `-` (or `J` for a
    /// call that makes JSON).
    fn json_format(&mut self, e: &Expr) -> &'static str {
        if self.eat_words(&["FORMAT", "JSON"]) {
            "J"
        } else if self.eat_words(&["FORMAT", "BSON"]) {
            "B"
        } else if matches!(e, Expr::Function(f) if MAKES_JSON.contains(&f.name.as_str())) {
            "J"
        } else {
            "-"
        }
    }

    /// A constructor's value: an untyped `NULL` is an unknown name
    /// (`SQL0206`), as the primary reports it.
    fn json_operand(&mut self) -> Result<Expr> {
        if self.is_word("NULL") && !self.is_word_at(1, "ON") {
            return Err(crate::error::SqlError::ColumnNotFound {
                name: "NULL".into(),
            });
        }
        self.expr()
    }

    /// The clauses after a constructor's values.
    fn json_tail(&mut self, opts: &mut Opts, unique: bool) -> Result<Option<TypeSpec>> {
        let mut ret = None;
        loop {
            if self.eat_words(&["NULL", "ON", "NULL"]) {
                opts.set("null", "null");
            } else if self.eat_words(&["ABSENT", "ON", "NULL"]) {
                opts.set("null", "absent");
            } else if unique && self.eat_words(&["WITH", "UNIQUE"]) {
                self.eat_word("KEYS");
                opts.set("unique", "1");
            } else if unique && self.eat_words(&["WITHOUT", "UNIQUE"]) {
                self.eat_word("KEYS");
                opts.set("unique", "0");
            } else if self.eat_word("RETURNING") {
                ret = Some(self.type_spec()?);
                if self.eat_words(&["FORMAT", "BSON"]) {
                    opts.set("out", "bson");
                } else {
                    self.eat_words(&["FORMAT", "JSON"]);
                }
            } else {
                return Ok(ret);
            }
        }
    }

    fn json_object(&mut self) -> Result<Expr> {
        let mut opts = Opts::default();
        let mut operands = Vec::new();
        let mut fmt = String::new();
        if !self.is_sym(")") {
            loop {
                if self.is_word("KEY") && !self.is_sym_at(1, "(") && !self.is_sym_at(1, ",") {
                    self.advance();
                }
                let k = self.expr()?;
                self.expect_word("VALUE")?;
                let v = self.json_operand()?;
                fmt.push_str(self.json_format(&v));
                operands.push(k);
                operands.push(v);
                if !self.eat_sym(",") {
                    break;
                }
            }
        }
        opts.set("fmt", fmt);
        let ret = self.json_tail(&mut opts, true)?;
        self.expect_sym(")")?;
        Ok(made("JSON_OBJECT", &opts, ret, operands))
    }

    fn json_array(&mut self) -> Result<Expr> {
        let mut opts = Opts::default();
        let mut operands = Vec::new();
        if self.starts_query() {
            operands.push(Expr::Subquery(Box::new(self.query()?)));
            opts.set("sub", "1");
        } else if !self.is_sym(")") && !self.is_word("RETURNING") {
            let mut fmt = String::new();
            loop {
                let v = self.json_operand()?;
                fmt.push_str(self.json_format(&v));
                operands.push(v);
                if !self.eat_sym(",") {
                    break;
                }
            }
            opts.set("fmt", fmt);
        }
        let ret = self.json_tail(&mut opts, false)?;
        self.expect_sym(")")?;
        Ok(made("JSON_ARRAY", &opts, ret, operands))
    }

    /// The context item and path of a query function: `ctx [FORMAT ..],
    /// path`; `PASSING` is refused.
    fn json_context(&mut self, opts: &mut Opts) -> Result<Vec<Expr>> {
        let item = self.expr()?;
        match self.json_format(&Expr::Literal(Literal::Null)) {
            "J" => opts.set("in", "json"),
            "B" => opts.set("in", "bson"),
            _ => {}
        }
        self.expect_sym(",")?;
        let path = self.expr()?;
        if self.is_word("PASSING") {
            return Err(Self::unsupported("PASSING variables in an SQL/JSON path"));
        }
        Ok(vec![item, path])
    }

    /// `NULL | ERROR | DEFAULT e | EMPTY ARRAY | EMPTY OBJECT | TRUE | FALSE
    /// | UNKNOWN ON EMPTY|ERROR`: the clause's word and its default, or
    /// `None` when the next words are not one.
    fn json_behavior(&mut self) -> Result<Option<(&'static str, &'static str, Option<Expr>)>> {
        let save = self.at;
        let (how, default) = if self.eat_word("NULL") {
            ("null", None)
        } else if self.eat_word("ERROR") {
            ("error", None)
        } else if self.eat_words(&["EMPTY", "ARRAY"]) {
            ("array", None)
        } else if self.eat_words(&["EMPTY", "OBJECT"]) {
            ("object", None)
        } else if self.eat_word("TRUE") {
            ("true", None)
        } else if self.eat_word("FALSE") {
            ("false", None)
        } else if self.eat_word("UNKNOWN") {
            ("unknown", None)
        } else if self.eat_word("DEFAULT") {
            ("default", Some(self.expr()?))
        } else {
            return Ok(None);
        };
        if !self.eat_word("ON") {
            self.at = save;
            return Ok(None);
        }
        let on = if self.eat_word("EMPTY") {
            "empty"
        } else {
            self.expect_word("ERROR")?;
            "error"
        };
        Ok(Some((on, how, default)))
    }

    /// `ON EMPTY` and `ON ERROR` clauses: their defaults, empty first.
    fn json_behaviors(&mut self, opts: &mut Opts) -> Result<[Expr; 2]> {
        let mut defaults = [null(), null()];
        while let Some((on, how, d)) = self.json_behavior()? {
            opts.set(on, how);
            if let Some(d) = d {
                defaults[usize::from(on == "error")] = d;
            }
        }
        Ok(defaults)
    }

    fn json_value(&mut self) -> Result<Expr> {
        let mut opts = Opts::default();
        let mut operands = self.json_context(&mut opts)?;
        let ret = if self.eat_word("RETURNING") {
            Some(self.type_spec()?)
        } else {
            None
        };
        let [empty, error] = self.json_behaviors(&mut opts)?;
        operands.push(empty);
        operands.push(error);
        self.expect_sym(")")?;
        Ok(made("JSON_VALUE", &opts, ret, operands))
    }

    fn json_query(&mut self) -> Result<Expr> {
        let mut opts = Opts::default();
        let operands = self.json_context(&mut opts)?;
        let mut ret = None;
        loop {
            if self.eat_word("RETURNING") {
                ret = Some(self.type_spec()?);
                if self.eat_words(&["FORMAT", "BSON"]) {
                    opts.set("out", "bson");
                } else {
                    self.eat_words(&["FORMAT", "JSON"]);
                }
            } else if self.eat_word("WITHOUT") {
                self.eat_word("ARRAY");
                self.expect_word("WRAPPER")?;
                opts.set("wrap", "none");
            } else if self.eat_word("WITH") {
                let how = if self.eat_word("CONDITIONAL") {
                    "cond"
                } else {
                    self.eat_word("UNCONDITIONAL");
                    "uncond"
                };
                self.eat_word("ARRAY");
                self.expect_word("WRAPPER")?;
                opts.set("wrap", how);
            } else if self.eat_words(&["KEEP", "QUOTES"]) {
                opts.set("quotes", "keep");
                self.eat_words(&["ON", "SCALAR", "STRING"]);
            } else if self.eat_words(&["OMIT", "QUOTES"]) {
                opts.set("quotes", "omit");
                self.eat_words(&["ON", "SCALAR", "STRING"]);
            } else {
                break;
            }
        }
        self.json_behaviors(&mut opts)?;
        self.expect_sym(")")?;
        Ok(made("JSON_QUERY", &opts, ret, operands))
    }

    fn json_exists(&mut self) -> Result<Expr> {
        let mut opts = Opts::default();
        let operands = self.json_context(&mut opts)?;
        self.json_behaviors(&mut opts)?;
        self.expect_sym(")")?;
        Ok(made("JSON_EXISTS", &opts, None, operands))
    }

    fn json_arrayagg(&mut self) -> Result<Expr> {
        let mut opts = Opts::default();
        let v = self.expr()?;
        if self.json_format(&v) == "J" {
            opts.set("fmt", "J");
        }
        let mut operands = vec![v];
        if self.eat_words(&["ORDER", "BY"]) {
            let mut ord = String::new();
            loop {
                operands.push(self.expr()?);
                ord.push(if self.eat_word("DESC") {
                    'D'
                } else {
                    self.eat_word("ASC");
                    'A'
                });
                if !self.eat_sym(",") {
                    break;
                }
            }
            opts.set("ord", ord);
        }
        let ret = self.json_tail(&mut opts, false)?;
        self.expect_sym(")")?;
        Ok(made("JSON_ARRAYAGG", &opts, ret, operands))
    }

    fn json_objectagg(&mut self) -> Result<Expr> {
        let mut opts = Opts::default();
        if self.is_word("KEY") && !self.is_sym_at(1, "(") {
            self.advance();
        }
        let k = self.expr()?;
        self.expect_word("VALUE")?;
        let v = self.expr()?;
        if self.json_format(&v) == "J" {
            opts.set("fmt", "J");
        }
        let ret = self.json_tail(&mut opts, true)?;
        self.expect_sym(")")?;
        Ok(made("JSON_OBJECTAGG", &opts, ret, vec![k, v]))
    }

    /// After `e IS [NOT]`: `JSON [VALUE | ARRAY | OBJECT | SCALAR]
    /// [FORMAT ..] [WITH | WITHOUT UNIQUE [KEYS]]`.
    pub(crate) fn is_json(&mut self, left: Expr, not: bool) -> Result<Expr> {
        self.expect_word("JSON")?;
        let mut opts = Opts::default();
        for kind in ["VALUE", "ARRAY", "OBJECT", "SCALAR"] {
            if self.eat_word(kind) {
                opts.set("kind", kind.to_ascii_lowercase());
            }
        }
        match self.json_format(&Expr::Literal(Literal::Null)) {
            "J" => opts.set("in", "json"),
            "B" => opts.set("in", "bson"),
            _ => {}
        }
        if self.eat_words(&["WITH", "UNIQUE"]) {
            self.eat_word("KEYS");
            opts.set("unique", "1");
        } else if self.eat_words(&["WITHOUT", "UNIQUE"]) {
            self.eat_word("KEYS");
        }
        let e = made("IS JSON", &opts, None, vec![left]);
        Ok(if not { Expr::Not(Box::new(e)) } else { e })
    }
}

impl Parser {
    /// `XMLPARSE(DOCUMENT e [STRIP | PRESERVE WHITESPACE])`.
    fn xml_parse(&mut self) -> Result<Expr> {
        if self.is_word("CONTENT") {
            return Err(Self::unsupported("XMLPARSE(CONTENT ...): only DOCUMENT"));
        }
        self.expect_word("DOCUMENT")?;
        let e = self.expr()?;
        let mut opts = Opts::default();
        if self.eat_words(&["PRESERVE", "WHITESPACE"]) {
            opts.set("ws", "preserve");
        } else {
            self.eat_words(&["STRIP", "WHITESPACE"]);
        }
        self.expect_sym(")")?;
        Ok(made("XMLPARSE", &opts, None, vec![e]))
    }

    /// `XMLSERIALIZE([CONTENT] e AS type [VERSION '1.0'] [INCLUDING |
    /// EXCLUDING XMLDECLARATION])`.
    fn xml_serialize(&mut self) -> Result<Expr> {
        self.eat_word("CONTENT");
        let e = self.expr()?;
        self.expect_word("AS")?;
        let t = self.type_spec()?;
        let mut opts = Opts::default();
        loop {
            if self.eat_word("VERSION") {
                self.advance();
            } else if self.eat_words(&["INCLUDING", "XMLDECLARATION"]) {
                opts.set("decl", "1");
            } else if !self.eat_words(&["EXCLUDING", "XMLDECLARATION"]) {
                break;
            }
        }
        self.expect_sym(")")?;
        Ok(made("XMLSERIALIZE", &opts, Some(t), vec![e]))
    }

    /// `JSON_TABLE(item, 'row path' COLUMNS (...)) [AS] x` after its `(`:
    /// a table function of `exec::json::table` whose arguments are the
    /// item and row path, then six for each column (its clauses, name,
    /// path, type as a `CAST` of `NULL`, and the `ON EMPTY` and `ON
    /// ERROR` defaults). It may name the columns before it.
    pub(crate) fn json_table(&mut self) -> Result<TableRef> {
        let mut opts = Opts::default();
        let mut args = self.json_context(&mut opts)?;
        self.expect_word("COLUMNS")?;
        self.expect_sym("(")?;
        loop {
            let name = self.ident()?;
            let mut col = Opts::default();
            let lit = |s: String| Expr::Literal(Literal::Str(s));
            if self.eat_words(&["FOR", "ORDINALITY"]) {
                col.set("k", "ord");
                args.extend([
                    lit(col.text()),
                    lit(name.value),
                    null(),
                    null(),
                    null(),
                    null(),
                ]);
            } else {
                let t = self.type_spec()?;
                if self.eat_words(&["FORMAT", "JSON"]) {
                    col.set("fmt", "J");
                }
                let path = if self.eat_word("PATH") {
                    self.string()?
                } else {
                    format!("$.{}", name.value.to_lowercase())
                };
                let [empty, error] = self.json_behaviors(&mut col)?;
                col.set("k", "val");
                args.extend([
                    lit(col.text()),
                    lit(name.value),
                    lit(path),
                    Expr::Cast(Box::new(null()), t),
                    empty,
                    error,
                ]);
            }
            if !self.eat_sym(",") {
                break;
            }
        }
        self.expect_sym(")")?;
        // `EMPTY ON ERROR` (the default) or `ERROR ON ERROR` for the table:
        // the latter a last pseudo-column entry (`k=terr`).
        if self.eat_words(&["ERROR", "ON", "ERROR"]) {
            let mut t = Opts::default();
            t.set("k", "terr");
            let lit = |s: String| Expr::Literal(Literal::Str(s));
            args.extend([lit(t.text()), null(), null(), null(), null(), null()]);
        } else if !self.eat_words(&["EMPTY", "ON", "ERROR"])
            && (self.is_word("NULL") || self.is_word("DEFAULT"))
        {
            // Not a table's behavior: SQL0199, as pub400.
            return Err(self.keyword());
        }
        self.expect_sym(")")?;
        let (alias, columns) = self.correlation()?;
        let alias = alias.ok_or_else(|| self.expected("a correlation name"))?;
        let name = ObjectName {
            schema: None,
            name: Ident::new("JSON_TABLE"),
        };
        Ok(TableRef::Lateral(Box::new(TableRef::Function {
            name,
            args,
            alias,
            columns,
        })))
    }
}

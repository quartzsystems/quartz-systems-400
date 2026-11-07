// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Array variables in the routine: each holds a handle; the statements
//! that read or assign one say so in their descriptor's `locs`
//! (`crate::embed` arrays), and `MAX_CARDINALITY` of a variable is its
//! type's cardinality, written into the text.

use std::collections::HashSet;

use super::{Flags, Gen};
use crate::ast::{Expr, Literal, PlExpr};
use crate::embed::{Desc, Op};
use crate::error::{Result, SqlError};
use crate::lex::{Tok, tokenize};
use crate::pl::rewrite;
use crate::pl::types::type_text;

/// A type's text for a `locs` entry: no blanks, `/` for commas.
fn locs_type(t: &crate::ast::TypeSpec) -> String {
    type_text(t)
        .replace(", ", "/")
        .replace(',', "/")
        .replace(' ', "")
}

impl Gen<'_> {
    /// The descriptor's array entries for these inputs and outputs, and
    /// the inputs with the outputs' old handles after them.
    pub(super) fn array_io(&self, d: &Desc, ins: &[usize], outs: &[usize]) -> (Vec<usize>, Desc) {
        let mut locs: Vec<String> = Vec::new();
        for (k, &v) in ins.iter().enumerate() {
            if self.vars[v].array.is_some() {
                locs.push(format!("A{k}"));
            }
        }
        let mut all = ins.to_vec();
        for (k, &v) in outs.iter().enumerate() {
            if let Some((t, max)) = &self.vars[v].array {
                locs.push(format!("B{k}~{}~{max}~{}", all.len(), locs_type(t)));
                all.push(v);
            }
        }
        let mut d = d.clone();
        if !locs.is_empty() {
            d.locs = locs.join(",");
        }
        (all, d)
    }

    /// `MAX_CARDINALITY(v)` of an array variable as its number.
    pub(super) fn max_cardinality(&self, text: &str) -> Result<String> {
        if !text.to_ascii_uppercase().contains("MAX_CARDINALITY") {
            return Ok(text.to_string());
        }
        let toks: Vec<Tok> = tokenize(text)?.into_iter().map(|t| t.tok).collect();
        let mut out = String::new();
        let mut i = 0;
        while i < toks.len() {
            if let (
                Tok::Word(f),
                Some(Tok::Sym("(")),
                Some(Tok::Word(v) | Tok::Delimited(v)),
                Some(Tok::Sym(")")),
            ) = (&toks[i], toks.get(i + 1), toks.get(i + 2), toks.get(i + 3))
                && f == "MAX_CARDINALITY"
                && let Some(k) = rewrite::Names::variable(self, v)
                && let Some((_, max)) = &self.vars[k].array
            {
                out.push(' ');
                out.push_str(&max.to_string());
                i += 4;
                continue;
            }
            out.push(' ');
            rewrite::render(&toks[i], &mut out);
            i += 1;
        }
        Ok(out.trim().to_string())
    }

    /// `SET a = e` of an array variable.
    pub(super) fn assign_array(&mut self, k: usize, e: &PlExpr) -> Result<()> {
        let (_, max) = self.vars[k].array.clone().expect("an array");
        if let Expr::Literal(Literal::Null) = &e.expr {
            self.free_arrays(&[k]);
            return Ok(());
        }
        // A constructor longer than the type: SQ20440 as the routine is made.
        if let Expr::Function(f) = &e.expr
            && f.name == "*ARRAY"
            && u64::try_from(f.args.len()).unwrap_or(u64::MAX) > max
        {
            return Err(SqlError::Coded {
                code: -20_440,
                state: "2202F",
                what: format!(
                    "an array of {} elements for an array of at most {max}",
                    f.args.len()
                ),
            });
        }
        let r = self.rewrite(&format!("VALUES ({})", e.text), &HashSet::new())?;
        self.run_vars(
            Op::Query,
            &r.text,
            &r.vars,
            &[k],
            Flags {
                quiet: true,
                ..Flags::default()
            },
        )
    }

    /// `SET a[i] = e`: the index into a temporary, then the element.
    pub(super) fn assign_element(
        &mut self,
        k: usize,
        index: &PlExpr,
        value: &PlExpr,
    ) -> Result<()> {
        let Some((elem, max)) = self.vars[k].array.clone() else {
            return Err(SqlError::Coded {
                code: -20_441,
                state: "428H2",
                what: "an element of a variable that is not an array".into(),
            });
        };
        // A constant index outside the type: SQ20439 as the routine is made.
        if let Expr::Literal(Literal::Num(n)) = &index.expr
            && n.parse::<i64>()
                .is_ok_and(|i| i < 1 || u64::try_from(i).is_ok_and(|i| i > max))
        {
            return Err(SqlError::Coded {
                code: -20_439,
                state: "2202E",
                what: format!("array index {n} is out of range"),
            });
        }
        let slot = self.auto("i32");
        let slot_ind = self.auto("i16");
        let ix = self.temp_var(&slot, &slot_ind, "INTEGER")?;
        let rw = self.rewrite(
            &format!("VALUES (CAST(({}) AS INTEGER))", index.text),
            &HashSet::new(),
        )?;
        self.run_vars(
            Op::Query,
            &rw.text,
            &rw.vars,
            &[ix],
            Flags {
                quiet: true,
                ..Flags::default()
            },
        )?;
        let rw = self.rewrite(
            &format!("VALUES (CAST(({}) AS {}))", value.text, type_text(&elem)),
            &HashSet::new(),
        )?;
        let d = Desc {
            op: Op::Query,
            text: rw.text.clone(),
            quiet: true,
            ..self.desc()
        };
        let (mut ins, mut d) = self.array_io(&d, &rw.vars, &[]);
        let at = ins.len();
        ins.push(ix);
        ins.push(k);
        let entry = format!("E0~{at}~{}~{max}~{}", at + 1, locs_type(&elem));
        d.locs = if d.locs.is_empty() {
            entry
        } else {
            format!("{},{entry}", d.locs)
        };
        // The element's value lands in `k`'s storage as its new handle.
        self.emit(&d, &ins, &[k])
    }

    /// The handles of these variables, the arrays among them, freed and
    /// cleared (a statement of the compiler's own, its failure ignored).
    pub(super) fn free_arrays(&mut self, ks: &[usize]) {
        for &k in ks {
            if self.vars[k].array.is_none() {
                continue;
            }
            let (place, ind) = (self.vars[k].place.clone(), self.vars[k].ind.clone());
            let ty = self.vars[k].ty.clone();
            let d = Desc {
                op: Op::Exec,
                text: "FREE LOCATOR ?".into(),
                quiet: true,
                ..self.desc()
            };
            let (t, len) = self.text_const(&d.encode()).expect("CCSID 37 text");
            let (go, skip) = (self.label(), self.label());
            self.ins(format!("%fh = load {place}"));
            self.ins("%fz = cmp.eq %fh, 0");
            self.term(format!("brif %fz, {skip}, {go}"));
            self.place(&go);
            self.ins("mov $uca, $sqlca");
            self.ins(format!(
                "callb _QRNX_SQLIN({place}, {}, {}, {ind}, 1)",
                ty.desc, ty.extra
            ));
            self.ins(format!(
                "callb _QRNX_SQLRUN($sqlca, {t}, {len}, $scope, {})",
                self.sig.program.len()
            ));
            self.ins("mov $sqlca, $uca");
            self.ins(format!("store {place}, 0"));
            self.ins(format!("store {ind}, -1"));
            self.place(&skip);
        }
    }
}

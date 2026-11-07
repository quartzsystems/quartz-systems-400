// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Recursive common table expressions: the initial rows, then the step
//! run on the rows the step before produced until it produces none. The
//! name stands for those rows while the step runs ([`Binding`]). With
//! `SEARCH` or `CYCLE` each row is expanded on its own, so its children,
//! its depth and its ancestors are known: `SEARCH DEPTH|BREADTH FIRST`
//! numbers the rows in that order (the `SET` column) and `CYCLE` marks a
//! row whose cycle columns repeat an ancestor's and does not expand it.
//! The `SET` columns are not part of `*`, as on Db2 for i.

use std::rc::Rc;

use store::txn::Txn;

use super::SqlContext;
use super::eval::{ColInfo, Env, Row, Schema};
use super::query::run_body;
use super::restype::SqlType;
use super::sort::{KeySpec, cmp_keys};
use crate::ast::{Query, RecursiveCte};
use crate::error::{Result, SqlError};
use crate::value::Value;

/// The levels a recursion may take before it is ended as a loop.
const MAX_LEVELS: usize = 100_000;

/// What a recursive expression's name stands for while its step runs.
#[derive(Clone)]
pub(crate) struct Binding {
    /// The name.
    pub name: String,
    /// The columns, named.
    pub schema: Rc<Schema>,
    /// The rows of the step before.
    pub rows: Rc<Vec<Row>>,
}

impl Binding {
    /// The columns under a correlation name.
    pub(crate) fn schema_as(&self, alias: &str) -> Schema {
        Schema {
            cols: self
                .schema
                .cols
                .iter()
                .map(|c| ColInfo {
                    table: Some(alias.to_string()),
                    ..c.clone()
                })
                .collect(),
        }
    }
}

/// One row of the result with what `SEARCH` and `CYCLE` need.
struct Node {
    row: Row,
    level: usize,
    parent: Option<usize>,
    cyclic: bool,
}

/// Run the bodies on the current binding: their visible values.
fn run_sides(
    txn: &mut Txn<'_>,
    ctx: &mut SqlContext,
    params: &[Value],
    sides: &[crate::ast::QueryBody],
    outer: &Env,
    explain_only: bool,
    width: Option<usize>,
) -> Result<(Option<Rc<Schema>>, Vec<Row>)> {
    let mut schema = None;
    let mut rows = Vec::new();
    for b in sides {
        let (s, produced) = run_body(txn, ctx, params, &Query::of(b.clone()), outer, explain_only)?;
        let visible: Vec<usize> = s
            .cols
            .iter()
            .enumerate()
            .filter(|(_, c)| !c.hidden)
            .map(|(i, _)| i)
            .collect();
        if let Some(w) = width
            && w != visible.len()
        {
            return Err(SqlError::ValueCount {
                what: "the sides of the recursive union have different numbers of columns".into(),
            });
        }
        for p in produced {
            rows.push(visible.iter().map(|&i| p.out[i].clone()).collect());
        }
        if schema.is_none() {
            schema = Some(Rc::new(Schema {
                cols: visible.iter().map(|&i| s.cols[i].clone()).collect(),
            }));
        }
    }
    Ok((schema, rows))
}

/// The rows of a recursive expression under `alias` with `columns`.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub(crate) fn run(
    txn: &mut Txn<'_>,
    ctx: &mut SqlContext,
    params: &[Value],
    cte: &RecursiveCte,
    alias: &str,
    columns: &[crate::ast::Ident],
    outer: &Env,
    explain_only: bool,
) -> Result<(Rc<Schema>, Vec<Row>)> {
    if cte.cycle.as_ref().is_some_and(|c| c.using.is_some()) {
        return Err(SqlError::Unsupported {
            what: "CYCLE ... USING".into(),
        });
    }
    let (seed_schema, seed) = run_sides(txn, ctx, params, &cte.seed, outer, explain_only, None)?;
    let seed_schema = seed_schema.unwrap_or_default();
    if seed_schema.cols.len() != cte.columns.len() {
        return Err(SqlError::ValueCount {
            what: format!(
                "{} names {} columns for {} values",
                cte.name.value,
                cte.columns.len(),
                seed_schema.cols.len()
            ),
        });
    }
    // The name's columns: the list's names, the initial rows' types, null-capable.
    let named = Rc::new(Schema {
        cols: seed_schema
            .cols
            .iter()
            .zip(&cte.columns)
            .map(|(c, n)| ColInfo {
                name: n.value.clone(),
                table: Some(cte.name.value.clone()),
                hidden: false,
                ..c.clone()
            })
            .collect(),
    });
    let width = named.cols.len();
    let index_of = |n: &crate::ast::Ident| -> Result<usize> {
        cte.columns
            .iter()
            .position(|c| c.value == n.value)
            .ok_or_else(|| SqlError::ColumnNotFound {
                name: n.value.clone(),
            })
    };
    let by: Vec<usize> = match &cte.search {
        Some(s) => s.by.iter().map(index_of).collect::<Result<_>>()?,
        None => Vec::new(),
    };
    let cyc: Vec<usize> = match &cte.cycle {
        Some(c) => c.columns.iter().map(index_of).collect::<Result<_>>()?,
        None => Vec::new(),
    };
    let step = |ctx: &mut SqlContext, txn: &mut Txn<'_>, rows: Vec<Row>| -> Result<Vec<Row>> {
        ctx.ctes.push(Binding {
            name: cte.name.value.clone(),
            schema: named.clone(),
            rows: Rc::new(rows),
        });
        let r = run_sides(
            txn,
            ctx,
            params,
            &cte.step,
            outer,
            explain_only,
            Some(width),
        );
        ctx.ctes.pop();
        Ok(r?.1)
    };
    let mut nodes: Vec<Node> = seed
        .into_iter()
        .map(|row| Node {
            row,
            level: 0,
            parent: None,
            cyclic: false,
        })
        .collect();
    let mut charge = ctx.charge();
    for n in &nodes {
        charge.row(&n.row)?;
    }
    if !explain_only {
        let one_by_one = cte.search.is_some() || cte.cycle.is_some();
        let mut frontier: Vec<usize> = (0..nodes.len()).collect();
        let mut level = 0;
        while !frontier.is_empty() {
            level += 1;
            if level > MAX_LEVELS {
                return Err(SqlError::ResourceLimit {
                    what: format!("{} recursed past {MAX_LEVELS} levels", cte.name.value),
                });
            }
            let mut next = Vec::new();
            if one_by_one {
                for &p in &frontier {
                    ctx.tick(txn)?;
                    if nodes[p].cyclic {
                        continue;
                    }
                    for row in step(ctx, txn, vec![nodes[p].row.clone()])? {
                        let cyclic = !cyc.is_empty() && {
                            let key: Vec<String> = cyc.iter().map(|&i| row[i].key()).collect();
                            let mut a = Some(p);
                            let mut hit = false;
                            while let Some(i) = a {
                                if cyc
                                    .iter()
                                    .map(|&c| nodes[i].row[c].key())
                                    .collect::<Vec<_>>()
                                    == key
                                {
                                    hit = true;
                                    break;
                                }
                                a = nodes[i].parent;
                            }
                            hit
                        };
                        charge.row(&row)?;
                        next.push(nodes.len());
                        nodes.push(Node {
                            row,
                            level,
                            parent: Some(p),
                            cyclic,
                        });
                    }
                }
            } else {
                let rows: Vec<Row> = frontier.iter().map(|&i| nodes[i].row.clone()).collect();
                for row in step(ctx, txn, rows)? {
                    charge.row(&row)?;
                    next.push(nodes.len());
                    nodes.push(Node {
                        row,
                        level,
                        parent: None,
                        cyclic: false,
                    });
                }
            }
            frontier = next;
        }
    }
    // The order the rows come out in, and the SEARCH sequence.
    let order = match &cte.search {
        Some(s) if s.depth => depth_first(&nodes, &by)?,
        Some(_) => breadth_first(&nodes, &by)?,
        None => (0..nodes.len()).collect(),
    };
    let mut cols: Vec<ColInfo> = named
        .cols
        .iter()
        .zip(columns.iter().map(Some).chain(std::iter::repeat(None)))
        .map(|(c, n)| ColInfo {
            name: n.map_or_else(|| c.name.clone(), |n| n.value.clone()),
            table: Some(alias.to_string()),
            ..c.clone()
        })
        .collect();
    if let Some(s) = &cte.search {
        cols.push(ColInfo {
            name: s.set.value.clone(),
            table: Some(alias.to_string()),
            field: Some(SqlType::bigint(false).to_field()),
            nullable: false,
            hidden: true,
        });
    }
    if let Some(c) = &cte.cycle {
        cols.push(ColInfo {
            name: c.set.value.clone(),
            table: Some(alias.to_string()),
            field: Some(SqlType::char(1, false).to_field()),
            nullable: false,
            hidden: true,
        });
    }
    let mut out = Vec::with_capacity(nodes.len());
    for (seq, &i) in order.iter().enumerate() {
        let mut row = nodes[i].row.clone();
        if cte.search.is_some() {
            row.push(Value::int(i64::try_from(seq + 1).expect("fits")));
        }
        if let Some(c) = &cte.cycle {
            row.push(Value::Str(if nodes[i].cyclic {
                c.to.clone()
            } else {
                c.default.clone()
            }));
        }
        out.push(row);
    }
    if explain_only {
        ctx.explain
            .push(format!("RECURSIVE {} ({alias})", cte.name.value));
    }
    Ok((Rc::new(Schema { cols }), out))
}

fn sort_by(nodes: &[Node], ids: &mut [usize], by: &[usize]) -> Result<()> {
    if by.is_empty() {
        return Ok(());
    }
    let specs = vec![
        KeySpec {
            desc: false,
            nulls_first: false
        };
        by.len()
    ];
    let key = |i: usize| -> Vec<Value> { by.iter().map(|&c| nodes[i].row[c].clone()).collect() };
    let mut failure = None;
    ids.sort_by(|a, b| match cmp_keys(&key(*a), &key(*b), &specs) {
        Ok(o) => o,
        Err(e) => {
            failure = Some(e);
            std::cmp::Ordering::Equal
        }
    });
    failure.map_or(Ok(()), Err)
}

/// Each row followed by its descendants, siblings by the `BY` columns.
fn depth_first(nodes: &[Node], by: &[usize]) -> Result<Vec<usize>> {
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); nodes.len()];
    let mut roots = Vec::new();
    for (i, n) in nodes.iter().enumerate() {
        match n.parent {
            Some(p) => children[p].push(i),
            None => roots.push(i),
        }
    }
    sort_by(nodes, &mut roots, by)?;
    for c in &mut children {
        sort_by(nodes, c, by)?;
    }
    let mut out = Vec::with_capacity(nodes.len());
    let mut stack: Vec<usize> = roots.into_iter().rev().collect();
    while let Some(i) = stack.pop() {
        out.push(i);
        stack.extend(children[i].iter().rev());
    }
    Ok(out)
}

/// Level by level, each by the `BY` columns.
fn breadth_first(nodes: &[Node], by: &[usize]) -> Result<Vec<usize>> {
    let deepest = nodes.iter().map(|n| n.level).max().unwrap_or(0);
    let mut out = Vec::with_capacity(nodes.len());
    for l in 0..=deepest {
        let mut ids: Vec<usize> = (0..nodes.len()).filter(|&i| nodes[i].level == l).collect();
        sort_by(nodes, &mut ids, by)?;
        out.extend(ids);
    }
    Ok(out)
}

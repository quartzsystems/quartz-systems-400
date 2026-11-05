// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What the planner keeps while a statement runs: the row goal
//! (`OPTIMIZE FOR n ROWS`, `FETCH FIRST`), the join methods it chose,
//! subquery answers it may reuse (`subq`), the estimates behind its
//! choices (`estimate`, from the column statistics of `stats`).

pub mod cache;
pub mod estimate;
pub mod explain;
pub mod govern;
pub mod keyonly;
pub mod pushdown;
pub mod stats;
pub mod statsave;
pub mod subq;

use super::join::Method;

/// Join methods a statement keeps for its monitor record and plan.
const KEPT_JOINS: usize = 64;

/// The planner's state for the statement running.
#[derive(Debug, Default)]
pub struct State {
    /// The rows the query is to give first, when it says.
    pub goal: Option<u64>,
    /// The join methods chosen, in the order the joins ran.
    pub joins: Vec<Method>,
    /// The rows `FETCH FIRST` (and `OFFSET`) of the query being planned
    /// take at most, for its `SELECT` to read no further.
    pub limit: Option<usize>,
    /// The columns the next table scan must give, when known.
    pub columns: Option<Vec<String>>,
    /// The rows the next table scan may stop after.
    pub stop: Option<usize>,
    /// The query governor has looked at the statement's query.
    pub governed: bool,
    /// The statement's subqueries and what they gave.
    pub subq: subq::Cache,
}

impl State {
    /// A statement starts.
    pub fn start(&mut self) {
        self.goal = None;
        self.limit = None;
        self.stop = None;
        self.columns = None;
        self.governed = false;
        self.joins.clear();
        self.subq.clear();
    }

    /// A join ran with `m`.
    pub fn joined(&mut self, m: Method) {
        if self.joins.len() < KEPT_JOINS {
            self.joins.push(m);
        }
    }

    /// The monitor's join method code (`QVC13`): the first join's other
    /// than a nested loop, else `N`; `None` with no join.
    #[must_use]
    pub fn join_code(&self) -> Option<char> {
        if self.joins.is_empty() {
            return None;
        }
        Some(self.joins.iter().find(|m| **m != Method::NestedLoop).map_or('N', |m| m.code()))
    }
}

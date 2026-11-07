// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Subquery answers as of a statement's start. A change whose condition
//! has a subquery reads first and writes after; the subqueries it ran
//! while reading are kept here, so a test made again once it has written
//! (a row another job changed meanwhile) never sees its own changes. Only
//! a subquery that reads no outer column is kept: one that does answers
//! for its outer row alone.

use std::collections::HashMap;
use std::rc::Rc;

use super::eval::Row;
use crate::ast::Query;

/// The answers of one statement.
#[derive(Debug, Default)]
pub struct Answers {
    /// Answering from what was kept, never running a subquery.
    replay: bool,
    /// Rows by subquery, with the subquery (an address may be reused).
    rows: HashMap<usize, (Query, Rc<Vec<Row>>)>,
    /// A replay met a subquery with no answer kept.
    missed: bool,
    /// The outermost frame a column read since the current subquery
    /// started.
    reach: usize,
}

/// What a subquery's run starts with: see [`Answers::enter`].
#[derive(Clone, Copy)]
pub struct Entered(usize);

fn key(q: &Query) -> usize {
    std::ptr::from_ref(q) as usize
}

impl Answers {
    /// Answers being kept, as the statement reads.
    #[must_use]
    pub fn recording() -> Self {
        Self {
            reach: usize::MAX,
            ..Self::default()
        }
    }

    /// From now on, answer from what was kept.
    pub fn replay(&mut self) {
        self.replay = true;
        self.missed = false;
    }

    /// Whether a replay since [`Self::replay`] needed an answer that was
    /// not kept: what it decided does not stand.
    #[must_use]
    pub fn missed(&self) -> bool {
        self.missed
    }

    /// The kept answer of `q`; in a replay an empty one, marked missed,
    /// when there is none. `None`: run it.
    pub fn lookup(&mut self, q: &Query) -> Option<Rc<Vec<Row>>> {
        if let Some((kept, r)) = self.rows.get(&key(q))
            && kept == q
        {
            return Some(Rc::clone(r));
        }
        if self.replay {
            self.missed = true;
            return Some(Rc::new(Vec::new()));
        }
        None
    }

    /// A column was read from frame `index`.
    pub fn touch(&mut self, index: usize) {
        self.reach = self.reach.min(index);
    }

    /// `q` starts to run.
    pub fn enter(&mut self) -> Entered {
        Entered(std::mem::replace(&mut self.reach, usize::MAX))
    }

    /// `q` ran over `floor` outer frames with `rows`: kept when it read
    /// none of them.
    pub fn leave(
        &mut self,
        entered: Entered,
        q: &Query,
        floor: usize,
        rows: Option<&Rc<Vec<Row>>>,
    ) {
        let reach = self.reach;
        self.reach = entered.0.min(reach);
        if reach >= floor
            && let Some(r) = rows
        {
            self.rows.insert(key(q), (q.clone(), Rc::clone(r)));
        }
    }
}

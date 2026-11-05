// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! How two inputs are joined, chosen by cost: a nested loop, a hash
//! join over the right rows, or a sort-merge join. Costs count row
//! operations: a pair the nested loop tries evaluates the condition, a
//! hash entry builds a key, a merge compares values. Every method finds
//! the same pairs in the same order (`join`), so the choice changes the
//! time and the working storage only.

/// A join method.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method {
    /// Every right row tried for each left row.
    NestedLoop,
    /// The right rows by the hash of the join column.
    Hash,
    /// Both sides sorted on the join column and merged.
    Merge,
}

impl Method {
    /// As the plan text and the database monitor name it.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::NestedLoop => "nested loop",
            Self::Hash => "hash join",
            Self::Merge => "sort-merge join",
        }
    }

    /// The join method code of the monitor's `QVC13`: `N` nested loop,
    /// `H` hash; `M` (Quartz's) sort-merge, which IBM's engine has not.
    #[must_use]
    pub fn code(self) -> char {
        match self {
            Self::NestedLoop => 'N',
            Self::Hash => 'H',
            Self::Merge => 'M',
        }
    }
}

/// What the choice weighs.
#[derive(Clone, Copy, Debug, Default)]
pub struct Inputs {
    /// Left rows.
    pub left: u64,
    /// Right rows.
    pub right: u64,
    /// The condition has an equality of a left and a right column.
    pub equality: bool,
    /// The left rows are in the join column's order already.
    pub left_sorted: bool,
    /// The right rows are.
    pub right_sorted: bool,
    /// Bytes the hash table of the right rows would hold.
    pub hash_bytes: u64,
    /// Bytes the statement may still hold.
    pub room: u64,
    /// `OPTIMIZE FOR n ROWS`: the rows wanted first.
    pub goal: Option<u64>,
}

/// What building a hash table or a sort costs before the first row.
const SETUP: f64 = 64.0;

/// The cost of each method; `None` where it cannot run.
#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn costs(i: &Inputs) -> [(Method, Option<f64>); 3] {
    let (l, r) = (i.left as f64, i.right as f64);
    // With a row goal only the left rows that give the first rows count.
    let lg = i.goal.map_or(l, |g| l.min(g as f64));
    let nl = 2.0 * lg * r;
    let (hash, merge) = if i.equality {
        let hash = (i.hash_bytes <= i.room).then_some(SETUP + 4.0 * r + 2.0 * lg);
        let sort = |n: f64, sorted: bool| if sorted { n } else { n * n.max(2.0).log2() + n };
        let merge = SETUP + sort(l, i.left_sorted) + sort(r, i.right_sorted);
        (hash, Some(merge))
    } else {
        (None, None)
    };
    [(Method::NestedLoop, Some(nl)), (Method::Hash, hash), (Method::Merge, merge)]
}

/// The cheapest method; a tie goes to the simpler one.
#[must_use]
pub fn choose(i: &Inputs) -> Method {
    let mut best = (Method::NestedLoop, f64::INFINITY);
    for (m, c) in costs(i) {
        if let Some(c) = c
            && c < best.1
        {
            best = (m, c);
        }
    }
    best.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inputs(left: u64, right: u64) -> Inputs {
        Inputs { left, right, equality: true, room: u64::MAX, ..Inputs::default() }
    }

    #[test]
    fn small_inputs_loop() {
        assert_eq!(choose(&inputs(5, 5)), Method::NestedLoop);
        assert_eq!(choose(&inputs(1, 10_000)), Method::NestedLoop);
        assert_eq!(choose(&Inputs { equality: false, ..inputs(1000, 1000) }), Method::NestedLoop);
    }

    #[test]
    fn large_inputs_hash_or_merge() {
        assert_eq!(choose(&inputs(1000, 1000)), Method::Hash);
        let sorted = Inputs { left_sorted: true, right_sorted: true, ..inputs(1000, 1000) };
        assert_eq!(choose(&sorted), Method::Merge);
        // No room for the hash table: the merge sorts instead.
        let tight = Inputs { hash_bytes: 10, room: 5, ..inputs(1000, 1000) };
        assert_eq!(choose(&tight), Method::Merge);
    }

    #[test]
    fn a_row_goal_favours_the_loop() {
        let goal = Inputs { goal: Some(1), ..inputs(1000, 20) };
        assert_eq!(choose(&goal), Method::NestedLoop);
        assert_eq!(choose(&inputs(1000, 20)), Method::Hash);
    }
}

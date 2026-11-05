// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What the planner chose for a monitored statement: one event per
//! table access and per sort, kept in the job's state while a monitor
//! watches it and written as `3000`, `3001` and `3003` records.

/// A table as the records name it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TableName {
    /// Library (system name).
    pub lib: String,
    /// File (system name).
    pub file: String,
    /// Member.
    pub member: String,
}

/// One choice the planner made.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlanEvent {
    /// The table read in arrival order (`3000`).
    Scan {
        /// The table.
        table: TableName,
        /// Rows in its member.
        rows: u64,
        /// Indexes exist over it (`T2`), else `T1`.
        indexes: bool,
        /// Rows are selected as they are read.
        selection: bool,
        /// The table's place in the join, from 1.
        position: u32,
    },
    /// The table read through a keyed path (`3001`).
    Index {
        /// The table.
        table: TableName,
        /// The file holding the path (the table itself for its own key
        /// or a constraint's).
        index: TableName,
        /// The constraint whose key it is.
        constraint: Option<String>,
        /// Rows in the table's member.
        rows: u64,
        /// Key columns positioned to.
        positioned: Vec<String>,
        /// The path gives the order asked for.
        ordering: bool,
        /// The table's place in the join, from 1.
        position: u32,
    },
    /// Rows sorted (`3003`).
    Sort {
        /// How many.
        rows: u64,
        /// IBM's reason code (`F1` grouping, `F2` ordering over a join,
        /// `F4` `DISTINCT`, `F5` `UNION`, `F7` ordering).
        reason: &'static str,
    },
}

// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The working storage one statement may hold. Every step that keeps
//! rows (scans, joins, grouping, `DISTINCT`, `UNION`) charges what it
//! keeps against the statement's meter, and gives it back when the rows
//! go. Past the limit the statement ends with `SQL0904` (SQLSTATE
//! `57011`) instead of growing until the process cannot allocate: the
//! storage service runs every job's SQL in one process.

use std::cell::Cell;
use std::rc::Rc;

use super::eval::Row;
use crate::error::{Result, SqlError};
use crate::value::Value;

/// The default [`super::SqlContext::memory_limit`]: 256 MiB of rows a
/// statement may hold at once, as estimated by [`row_bytes`].
pub const STATEMENT_MEMORY: usize = 256 << 20;

/// What is in use now, shared by every charge of a context.
pub type Meter = Rc<Cell<usize>>;

/// Bytes held by one step, given back when it drops.
pub struct Charge {
    meter: Meter,
    limit: usize,
    bytes: usize,
}

impl Charge {
    /// A charge of nothing yet against `meter`.
    #[must_use]
    pub fn new(meter: &Meter, limit: usize) -> Self {
        Self {
            meter: Rc::clone(meter),
            limit,
            bytes: 0,
        }
    }

    /// `n` more bytes; `SQL0904` when the statement would pass its limit.
    pub fn add(&mut self, n: usize) -> Result<()> {
        let used = self.meter.get().saturating_add(n);
        if used > self.limit {
            return Err(SqlError::ResourceLimit {
                what: format!(
                    "the statement needs more than {} MiB of working storage",
                    self.limit >> 20
                ),
            });
        }
        self.meter.set(used);
        self.bytes += n;
        Ok(())
    }

    /// Bytes the statement may still take.
    #[must_use]
    pub fn room(&self) -> usize {
        self.limit.saturating_sub(self.meter.get())
    }

    /// Everything given back.
    pub fn clear(&mut self) {
        self.meter.set(self.meter.get().saturating_sub(self.bytes));
        self.bytes = 0;
    }

    /// A row more.
    pub fn row(&mut self, r: &Row) -> Result<()> {
        self.add(row_bytes(r))
    }
}

impl Drop for Charge {
    fn drop(&mut self) {
        self.clear();
    }
}

/// What a row takes in memory, near enough: its vector, its values and
/// the text and bytes they own.
#[must_use]
pub fn row_bytes(r: &Row) -> usize {
    let heap: usize = r
        .iter()
        .map(|v| match v {
            Value::Str(s) => s.len(),
            Value::Bin(b) => b.len(),
            _ => 0,
        })
        .sum();
    std::mem::size_of::<Row>() + r.len() * std::mem::size_of::<Value>() + heap
}

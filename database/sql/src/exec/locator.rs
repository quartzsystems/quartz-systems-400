// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! LOB locators: handles a program holds for LOB values instead of the
//! values. A handle is a nonzero 4-byte number (IBM's `SQLTYPE` 960,
//! 964 and 968 carry it); the table maps it to the value it was given
//! for, in the unit of work: `COMMIT` and `ROLLBACK` free every handle
//! not held, `FREE LOCATOR` (and `HOLD LOCATOR`) one by one.
//!
//! The values here are the SQL values themselves (a LOB read from its
//! member's tree is already whole in memory when a row is fetched); a
//! later change can keep a member's LOB ID instead and read pieces with
//! `SUBSTR` on demand without changing the handles.
//!
//! BLOCKED(rel): locator host variables (`SQL TYPE IS CLOB_LOCATOR`),
//! `FREE LOCATOR` and `HOLD LOCATOR` statements and DRDA's locator
//! flows belong to the embedded-SQL and DRDA wave; this is the table
//! they use.

use std::collections::HashMap;

use crate::error::{Result, SqlError};
use crate::value::Value;

/// The locators of a job's unit of work.
#[derive(Debug, Default)]
pub struct Locators {
    next: u32,
    values: HashMap<u32, (Value, bool)>,
}

/// `SQL0423`: a handle that is not a live locator.
fn not_valid(h: u32) -> SqlError {
    SqlError::Coded {
        code: -423,
        state: "0F001",
        what: format!("locator {h} is not valid"),
    }
}

impl Locators {
    /// A new handle for `v`.
    pub fn assign(&mut self, v: Value) -> u32 {
        loop {
            self.next = self.next.wrapping_add(1).max(1);
            if !self.values.contains_key(&self.next) {
                self.values.insert(self.next, (v, false));
                return self.next;
            }
        }
    }

    /// The value of a handle.
    pub fn get(&self, h: u32) -> Result<&Value> {
        self.values
            .get(&h)
            .map(|(v, _)| v)
            .ok_or_else(|| not_valid(h))
    }

    /// `FREE LOCATOR h`.
    pub fn free(&mut self, h: u32) -> Result<()> {
        self.values
            .remove(&h)
            .map(|_| ())
            .ok_or_else(|| not_valid(h))
    }

    /// `HOLD LOCATOR h`: kept past the end of the unit of work.
    pub fn hold(&mut self, h: u32) -> Result<()> {
        self.values
            .get_mut(&h)
            .map(|e| e.1 = true)
            .ok_or_else(|| not_valid(h))
    }

    /// The unit of work ended: every handle not held is freed.
    pub fn end_unit_of_work(&mut self) {
        self.values.retain(|_, (_, held)| *held);
    }

    /// Live handles.
    #[must_use]
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Whether none is live.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}

impl crate::exec::SqlContext {
    /// `COMMIT` or `ROLLBACK` ended the unit of work: its isolation level
    /// and its locators not held go.
    pub(crate) fn unit_ended(&mut self) {
        self.isolation = None;
        self.locators.end_unit_of_work();
        self.loose_savepoints.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handles_live_to_the_end_of_the_unit_of_work() {
        let mut l = Locators::default();
        let a = l.assign(Value::Str("clob".into()));
        let b = l.assign(Value::Bin(vec![1, 2]));
        assert_ne!(a, b);
        assert_eq!(l.get(a).unwrap(), &Value::Str("clob".into()));
        l.hold(b).unwrap();
        l.end_unit_of_work();
        assert_eq!(l.get(a).unwrap_err().sqlcode(), -423);
        assert_eq!(l.get(b).unwrap(), &Value::Bin(vec![1, 2]));
        l.free(b).unwrap();
        assert!(l.is_empty());
        assert_eq!(l.free(b).unwrap_err().sqlcode(), -423);
    }
}

// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The CLI as bound procedures: C linkage (`ABI.md` §1), integers by
//! value, every pointer as a fat pointer by value, `SQLRETURN` back.
//! Each job's handles live here, by job; the program's storage is
//! reached through the job's spaces at each use.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use mi::{FatPtr, Job};
use sql::Value;
use sql::embed::{Reply, Request};

use crate::consts::rc;
use crate::exec::{Cli, Host};
use crate::model::handles::Ptr;

pub mod calls;

thread_local! {
    /// Each installed job's CLI, by job address.
    static STATE: RefCell<HashMap<usize, Rc<RefCell<Cli>>>> = RefCell::new(HashMap::new());
}

/// A pointer argument; `None` for null.
#[must_use]
pub fn ptr(v: u128) -> Ptr {
    let p = FatPtr::from_bytes(&v.to_le_bytes());
    (!p.is_null()).then_some(v)
}

/// The low 32 bits of an integer argument.
#[must_use]
#[allow(clippy::cast_possible_truncation)]
pub fn int(v: i64) -> i32 {
    v as i32
}

/// The low 16 bits of an integer argument.
#[must_use]
#[allow(clippy::cast_possible_truncation)]
pub fn short(v: i64) -> i16 {
    v as i16
}

/// The job's storage and SQL.
struct JobHost<'a> {
    job: &'a mut Job,
}

impl JobHost<'_> {
    fn extent(&self, p: u128) -> Option<(u64, usize)> {
        let (at, avail) = self.job.spaces.extent(FatPtr::from_bytes(&p.to_le_bytes())).ok()?;
        Some((at, usize::try_from(avail).unwrap_or(usize::MAX)))
    }
}

impl Host for JobHost<'_> {
    fn read(&self, p: u128, n: usize) -> Option<Vec<u8>> {
        let (at, avail) = self.extent(p)?;
        (n <= avail).then(|| mi::storage::view(at, n).to_vec())
    }

    fn avail(&self, p: u128) -> usize {
        self.extent(p).map_or(0, |(_, n)| n)
    }

    fn write(&mut self, p: u128, b: &[u8]) -> bool {
        let Some((at, avail)) = self.extent(p) else { return false };
        if b.len() > avail {
            return false;
        }
        mi::storage::view(at, b.len()).copy_from_slice(b);
        true
    }

    fn run(&mut self, req: &Request) -> (Reply, Vec<(usize, Value)>) {
        if self.job.sql.is_none() {
            let e = sql::SqlError::Database { what: "no database for SQL in this job".into(), msgid: None };
            return (Reply { sqlca: sql::Sqlca::from_error(&e), rows: Vec::new(), call: None }, Vec::new());
        }
        qrpgrt::sqlcall::run_request(self.job, req.clone())
    }
}

/// Run `f` over the job's CLI. A call made while one is in progress in
/// the job (a procedure the CLI called using the CLI again) fails.
pub fn with(job: &mut Job, f: impl FnOnce(&mut Cli, &mut dyn Host) -> i32) -> i32 {
    let key = std::ptr::from_ref(job) as usize;
    let cli = STATE.with(|m| Rc::clone(m.borrow_mut().entry(key).or_default()));
    let Ok(mut c) = cli.try_borrow_mut() else { return rc::ERROR };
    let mut host = JobHost { job };
    f(&mut c, &mut host)
}

/// Forget a job's handles.
pub fn drop_state(job: &Job) {
    STATE.with(|m| {
        m.borrow_mut().remove(&(std::ptr::from_ref(job) as usize));
    });
}

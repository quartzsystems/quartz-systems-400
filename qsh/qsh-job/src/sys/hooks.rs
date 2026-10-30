// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The utilities' hooks (`system`, `db2`, `nslookup` ...) as tasks for
//! the job's thread.

use qsh::sys::{
    ClOutcome, ClRequest, ClRunner, DnsAnswer, DnsQuery, Resolver, SqlOutcome, SqlRequest,
    SqlRunner, SysError,
};

use crate::broker::Broker;

/// The hooks over a broker.
pub struct Hooks {
    broker: Broker,
}

impl Hooks {
    /// Over `broker`.
    pub fn new(broker: Broker) -> Self {
        Self { broker }
    }
}

impl ClRunner for Hooks {
    fn run_cl(&self, req: &ClRequest) -> Result<ClOutcome, SysError> {
        let r = req.clone();
        self.broker.call(move |s| s.run_cl(&r))?
    }
}

impl SqlRunner for Hooks {
    fn run_sql(&self, req: &SqlRequest) -> Result<SqlOutcome, SysError> {
        let r = req.clone();
        self.broker.call(move |s| s.run_sql(&r))?
    }
}

impl Resolver for Hooks {
    fn query(&self, q: &DnsQuery) -> Result<DnsAnswer, SysError> {
        let r = q.clone();
        self.broker.call(move |s| s.resolve(&r))?
    }

    fn host_name(&self) -> Result<String, SysError> {
        self.broker.call(|s| s.host_name())?
    }
}

// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Embedded SQL in a job process: each statement runs in the service,
//! which keeps the job's cursors and prepared statements.

use sql::embed::{Embedded, Reply, Request, to_wire};

use crate::Shared;
use crate::proto::Client;

/// Embedded SQL over a link.
pub struct RemoteSql {
    link: Shared,
}

impl RemoteSql {
    /// Over a link.
    #[must_use]
    pub fn new(link: Shared) -> Self {
        Self { link }
    }
}

impl Embedded for RemoteSql {
    fn run(&mut self, req: &Request) -> Reply {
        let inputs = req.inputs.iter().map(to_wire).collect();
        let w = self
            .link
            .borrow_mut()
            .with(|l| l.sql_run(req.scope.clone(), req.desc.encode(), inputs));
        Reply::from_wire(&w)
    }
}

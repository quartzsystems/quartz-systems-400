// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The registration facility as a server job sees it: the exit
//! programs of a point, and refusals reported for the audit journal,
//! answered by the service. A server pairs it with its own job as the
//! `regfac::ProgramCaller` in `regfac::call_exit`.

use regfac::{Answer, Called, ExitProgram, ExitRegistry, Query, RegFail, RegResult};

use crate::Shared;
use crate::proto::Client;

/// The registry over a link.
pub struct RemoteRegistry {
    link: Shared,
}

impl RemoteRegistry {
    /// Over a link.
    #[must_use]
    pub fn new(link: Shared) -> Self {
        Self { link }
    }

    /// Ask the service.
    pub fn ask(&mut self, q: Query) -> RegResult<Answer> {
        self.link.borrow_mut().with(|l| l.regfac(q))
    }
}

impl ExitRegistry for RemoteRegistry {
    fn programs(&mut self, point: &str, format: &str) -> RegResult<Vec<ExitProgram>> {
        match self.ask(Query::Programs {
            name: point.into(),
            format: format.into(),
        })? {
            Answer::Programs { programs } => Ok(programs),
            _ => Err(RegFail::unavailable("the service answered something else")),
        }
    }

    fn refused(&mut self, call: &Called) {
        let _ = self.ask(Query::Refused { call: call.clone() });
    }
}

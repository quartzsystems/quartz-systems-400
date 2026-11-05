// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Server mode (`SQL_ATTR_SERVER_MODE`): each connection's statements
//! run in a job of their own, a `QSQSRVR` prestart job in `QSYSWRK`, as
//! IBM i runs them. The job process supplies the [`Connector`] that
//! gets one ([`set_connector`]); [`proto`] is what crosses between the
//! two jobs. A job without one refuses server-mode connections.

use std::cell::RefCell;
use std::rc::Rc;

use sql::Value;
use sql::embed::{Reply, Request};

use crate::model::diag::Diag;

pub mod proto;

/// Where a connection's statements run.
pub trait Runner {
    /// Run one statement; a `CALL` it plans is made there. The reply and
    /// the `OUT` and `INOUT` values by marker.
    fn run(&mut self, req: &Request) -> (Reply, Vec<(usize, Value)>);

    /// The connection ends: the job serves another.
    fn close(&mut self) {}
}

/// What a server-mode connection asks for.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ConnectReq {
    /// The user, when `SQLConnect` named one; the job's own otherwise.
    pub user: Option<String>,
    /// Its password.
    pub password: Option<String>,
}

/// Gets a server job for a connection, or why not.
pub type Connector = Rc<dyn Fn(&ConnectReq) -> Result<Box<dyn Runner>, Diag>>;

thread_local! {
    static CONNECTOR: RefCell<Option<Connector>> = const { RefCell::new(None) };
}

/// Set (or clear) this thread's connector.
pub fn set_connector(c: Option<Connector>) {
    CONNECTOR.with(|x| *x.borrow_mut() = c);
}

/// A server job for a connection.
pub fn connect(req: &ConnectReq) -> Result<Box<dyn Runner>, Diag> {
    match CONNECTOR.with(|c| c.borrow().clone()) {
        Some(c) => c(req),
        None => Err(Diag::cli("08001", "This job cannot start a server-mode connection.")),
    }
}

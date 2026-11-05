// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Server-mode CLI connections in the kit: each one's statements run
//! in an SQL session of its own over the kit's store, as a `QSQSRVR`
//! job's would. That session has no runtime of its own, so a `CALL` is
//! not made there.

use std::rc::Rc;

use sql::Value;
use sql::embed::{Embedded, Reply, Request, Session};
use sqlcli::server::Runner;

use super::files::KitDb;

/// A server job's session.
struct KitServer(Session);

impl Runner for KitServer {
    fn run(&mut self, req: &Request) -> (Reply, Vec<(usize, Value)>) {
        let mut req = req.clone();
        req.desc.calls = false;
        (self.0.run(&req), Vec::new())
    }
}

/// Install the CLI in the kit's job, server-mode connections over `db`.
pub fn install(job: &mut mi::Job, db: Option<&KitDb>) {
    sqlcli::install(job);
    let db = db.cloned();
    sqlcli::server::set_connector(db.map(|d| -> sqlcli::server::Connector {
        Rc::new(move |_req| Ok(Box::new(KitServer(d.sql_server())) as Box<dyn Runner>))
    }));
}

/// Forget the job's handles and the connector.
pub fn uninstall(job: &mi::Job) {
    sqlcli::uninstall(job);
    sqlcli::server::set_connector(None);
}

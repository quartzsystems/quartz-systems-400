// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The `Sql` service in the connection job: the engine through the
//! storage service as the job's embedded SQL reaches it (`sql_run`,
//! the job's own session, so as the signed-on profile with its
//! authority), over the job's own link for the lane and a secondary
//! thread's link for the catalog. A cancel of a running `Execute` or
//! `Fetch` is the file [`stg::sql_cancel_path`], which the engine looks
//! for between rows; the lane removes it when the operation ends.

use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

use qap_proto::ServerMessage;
use sql::embed::{Embedded, Reply, Request};
use stg::{Client, Link, Shared};

use crate::qapsql::Engine;

/// Nothing runs in the lane.
pub const IDLE: u8 = 0;
/// A command runs: a cancel ends it as System Request option 2 does.
pub const COMMAND: u8 = 1;
/// An SQL statement runs: a cancel asks the engine to end it.
pub const SQL: u8 = 2;

/// What runs in the lane now, for the canceller.
pub type Running = Arc<AtomicU8>;

/// The engine over the job's own link.
pub struct LaneDb<'a>(pub &'a Shared);

impl Engine for LaneDb<'_> {
    fn run(&mut self, req: &Request) -> Reply {
        stg::remote::RemoteSql::new(Rc::clone(self.0)).run(req)
    }

    fn message(&mut self, id: &str, data: &[&str]) -> ServerMessage {
        super::host::message(&mut self.0.borrow_mut(), id, data)
    }
}

/// The engine over a secondary thread's link.
pub struct ThreadDb<'a>(pub &'a mut Link);

impl Engine for ThreadDb<'_> {
    fn run(&mut self, req: &Request) -> Reply {
        let inputs = req.inputs.iter().map(sql::embed::to_wire).collect();
        let w = self.0.with(|l| l.sql_run(req.scope.clone(), req.desc.encode(), inputs));
        Reply::from_wire(&w)
    }

    fn message(&mut self, id: &str, data: &[&str]) -> ServerMessage {
        super::host::message(self.0, id, data)
    }
}

/// Ask the service to end job `number`'s running statement.
pub fn ask_cancel(number: u32) {
    let path = stg::sql_cancel_path(number);
    if let Some(dir) = std::path::Path::new(&path).parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Err(e) = std::fs::write(&path, b"") {
        eprintln!("qsjob: QAP cancel of job {number:06}: {e}");
    }
}

/// Run `f` as the lane's running statement: a cancel meanwhile asks
/// the engine to end it; the ask goes with the statement.
pub fn running<T>(running: &Running, number: u32, f: impl FnOnce() -> T) -> T {
    running.store(SQL, Ordering::SeqCst);
    let r = f();
    running.store(IDLE, Ordering::SeqCst);
    let _ = std::fs::remove_file(stg::sql_cancel_path(number));
    r
}

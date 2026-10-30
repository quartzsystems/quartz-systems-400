// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The data queue server against the `dtaq` trace, over queues in
//! memory.

mod common;

use common::mem::MemQueues;
use common::{Masks, TraceAuth, accept, replay, trace};
use hostserver::dtaq::DtaqServer;
use hostserver::{Handler, NoExits};

#[test]
fn dtaq() {
    for s in ["dtaq", "svrmap"] {
        let t = trace("dtaq", s);
        let auth = TraceAuth::of(&t);
        let diffs = replay(&t, &Masks::default(), &mut || {
            Box::new(DtaqServer::new(auth.clone(), MemQueues::default(), NoExits)) as Box<dyn Handler>
        });
        accept(&format!("dtaq-{s}"), &diffs);
    }
}

// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The file server against the `ifs` trace, over the IFS in memory
//! with `/home/QSTEST` holding what the oracle's did.

mod common;

use common::mem::ifs;
use common::{Masks, TraceAuth, accept, replay, trace};
use hostserver::file::FileServer;
use hostserver::{Handler, NoExits};

fn masks() -> Masks {
    Masks {
        template: vec![
            // A listing entry: the times, the file ID, the sizes.
            (0x8005, 2..30),
            (0x8005, 46..50),
            (0x8005, 61..69),
            // An open: the file ID and the times.
            (0x8002, 6..10),
            (0x8002, 18..42),
            (0x8002, 62..66),
        ],
        param: vec![(0x0010, 180..188), (0x000F, 32..56), (0x000F, 76..80), (0x000F, 136..140)],
        skip: Vec::new(),
    }
}

#[test]
fn ifs_trace() {
    for s in ["ifs", "svrmap"] {
        let t = trace("file", s);
        let auth = TraceAuth::of(&t);
        let diffs = replay(&t, &masks(), &mut || Box::new(FileServer::new(auth.clone(), ifs(), NoExits)) as Box<dyn Handler>);
        accept(&format!("file-{s}"), &diffs);
    }
}

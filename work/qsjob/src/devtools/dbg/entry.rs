// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Service entry points, the calling job's side: before a dynamic call
//! loads its program, a job not serviced asks whether an entry point
//! for the program and its user is set (QAP `Debug`, version 9). The
//! programs with entry points for the user are kept for a second, so
//! calls cost a request only when one may hit. A hit attaches the job
//! to the entry's owner (`stgsvc/dbg.rs`), puts the program under debug
//! and stops it at its first statement, as a serviced job stops.

use std::cell::RefCell;
use std::time::{Duration, Instant};

use mi::Job;
use stg::Client;

use crate::console::host::Host;

/// How long the list of entry points is trusted.
const FRESH: Duration = Duration::from_secs(1);

thread_local! {
    /// When the list was read, and the programs on it.
    static KNOWN: RefCell<(Option<Instant>, Vec<String>)> = const { RefCell::new((None, Vec::new())) };
}

/// The job is about to call `name` in `lib`.
pub fn calling(job: &mut Job, host: &Host, name: &str, lib: &str) {
    if super::remote::is_served() {
        return;
    }
    let short = name.trim().to_ascii_uppercase();
    let stale = KNOWN.with(|k| k.borrow().0.is_none_or(|at| at.elapsed() > FRESH));
    if stale {
        let list = host.link.borrow_mut().with(Client::srvent_list);
        KNOWN.with(|k| *k.borrow_mut() = (Some(Instant::now()), list));
    }
    let may = KNOWN.with(|k| {
        k.borrow()
            .1
            .iter()
            .any(|p| p.rsplit('/').next() == Some(short.as_str()))
    });
    if !may {
        return;
    }
    let hit = host
        .link
        .borrow_mut()
        .with(|l| l.srvent_hit(lib.trim().into(), short.clone()));
    if !hit.is_empty() {
        super::remote::attach_entry(job, &hit);
    }
}

// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `QSYS/QEJSCHED`, the job scheduler `QSYSSCD` runs: it asks the
//! storage service to submit the schedule entries that are due, logs
//! what was submitted, and waits until it is ended.

use std::time::Duration;

use stg::Shared;
use stg::remote::status;
use wm::job::Status;

use crate::console::Console;

/// How often the schedule is looked at.
const TICK: Duration = Duration::from_secs(5);

/// Run until the job is ended; the end code.
pub fn run(c: &mut Console, link: &Shared) -> u16 {
    loop {
        status(link, Status::Run, "PGM-QEJSCHED");
        let done = link.borrow_mut().with(stg::Client::scd_tick);
        for text in done {
            c.job().log_entry(wm::joblog::Entry {
                kind: "Information".into(),
                time: now_micros(),
                from: ("QEJSCHED".into(), "QSYS".into()),
                from_inst: "*N".into(),
                to: ("*EXT".into(), String::new()),
                to_inst: "*N".into(),
                text,
                external: true,
                ..wm::joblog::Entry::default()
            });
        }
        status(link, Status::Evtw, "");
        if !c.idle(TICK) {
            return 0;
        }
    }
}

fn now_micros() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_micros()).unwrap_or(u64::MAX))
}

// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `stgstat`: how many entries the service's tables hold, for the
//! soak's leak checks. The connections live in the serve loop, so the
//! loop counts them every few seconds (`refresh`) and the command reads
//! that count with the system's own tables.

use std::cell::RefCell;
use std::fmt::Write as _;
use std::time::{Duration, Instant};

use super::served::Conn;
use crate::ipl::system::System;

/// How often the loop counts its connections.
const EVERY: Duration = Duration::from_secs(5);

thread_local! {
    static LAST: RefCell<(Option<Instant>, String)> = const { RefCell::new((None, String::new())) };
}

/// Count the connections, at most every [`EVERY`].
pub fn refresh(conns: &[Conn]) {
    LAST.with(|l| {
        let mut l = l.borrow_mut();
        if l.0.is_some_and(|t| t.elapsed() < EVERY) {
            return;
        }
        let jobs: Vec<&super::served::JobCtx> = conns.iter().filter_map(|c| c.ctx.as_ref()).collect();
        let threads = jobs.iter().filter(|j| j.thread).count();
        let log: usize = jobs.iter().map(|j| j.joblog.len()).sum();
        let wrapped: usize = jobs.iter().map(|j| j.joblog_wrapped.len()).sum();
        let splfs: usize = jobs.iter().map(|j| j.splfs.len()).sum();
        let handles: usize = jobs.iter().map(|j| j.handles.len()).sum();
        let sql = jobs.iter().filter(|j| j.sql.is_some()).count();
        let console_sql = jobs.iter().filter(|j| j.svc.sql.is_some()).count();
        let xfer: usize = jobs.iter().map(|j| j.svc.xfer.len()).sum();
        l.1 = format!(
            "connections {} jobs {} threads {threads}\njob log entries {log} wrapped {wrapped} open splfs {splfs} \
             profile handles {handles} embedded SQL {sql} console SQL {console_sql} xfer {xfer}\n",
            conns.len(),
            jobs.len()
        );
        l.0 = Some(Instant::now());
    });
}

/// The report.
pub fn report(system: &System) -> String {
    let mut o = LAST.with(|l| l.borrow().1.clone());
    let st = &system.stats;
    let _ = writeln!(
        o,
        "active {} attrs {} job_seen {} queued {} pending {} jobq_attach {} end_requests {} self_ending {}",
        system.jobs.len(),
        system.attrs.len(),
        system.job_seen.len(),
        system.queued.len(),
        system.pending.len(),
        system.jobq_attach.len(),
        system.end_requests.len(),
        system.self_ending.len()
    );
    let _ = writeln!(
        o,
        "stats aux_io {} taken {} io {} txns {} inflight {}",
        st.aux_io.len(),
        st.taken.len(),
        st.io.len(),
        st.txns.len(),
        st.inflight.len()
    );
    let _ = writeln!(
        o,
        "spool files {} writers {} sbs {} ended_sbs {} break_alloc {} break_pending {} joblog_pending {}",
        system.spool.files.len(),
        system.writers.len(),
        system.sbs.len(),
        system.ended_sbs.len(),
        system.break_alloc.len(),
        system.break_pending.len(),
        system.joblog_pending.len()
    );
    let locks = system.locks.lock().map_or(0, |l| l.len());
    let _ = writeln!(
        o,
        "record locks {locks} journal job names {} host server tokens {} svc xfer {}",
        jrn::entry::named_jobs(),
        super::hostsvr::token_count(),
        system.svc.xfer.len()
    );
    if let Some(s) = system.storage.as_ref() {
        let c = s.store.stats().cache;
        let _ = writeln!(o, "page cache {} of {} pages", c.held, c.capacity);
    }
    o
}

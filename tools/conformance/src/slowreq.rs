// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Step 200, a slow request (QS/VS's finding T5): the storage service's
//! fault point holds one job's command for 30 s, past the 10 s request
//! that was followed by the service ending in a partition, while the
//! other jobs keep calling it; then holds the IFS device's writes for
//! 45 s, past the block layer's 30 s request timeout, under an IFS
//! write. The service, the system and every job stay: no `B1001060`, no
//! job loses the service, the same jobs active after as before, and the
//! stall logged.

use std::collections::BTreeSet;
use std::time::{Duration, Instant};

use crate::qemu::session::Session;
use crate::steps::{CMD, Suite};

/// The jobs active: name and number, from `wrkactjob VIEW(3)`.
fn active(s: &mut Session) -> Result<BTreeSet<(String, String)>, String> {
    let out = s.command("svc.wrkactjob VIEW(3)", CMD)?;
    let jobs: BTreeSet<_> = out
        .lines()
        .filter_map(|l| {
            let t: Vec<&str> = l.split_whitespace().collect();
            let n = t
                .iter()
                .find(|w| w.len() == 6 && w.bytes().all(|b| b.is_ascii_digit()))?;
            Some((t.first()?.to_string(), (*n).to_string()))
        })
        .collect();
    if jobs.is_empty() {
        return Err(format!("wrkactjob shows no jobs:\n{out}"));
    }
    Ok(jobs)
}

/// What the log says went wrong since `from`.
fn lost(s: &Session, from: usize) -> Option<String> {
    let tail = &s.log[from.min(s.log.len())..];
    tail.lines()
        .find(|l| {
            [
                "B1001060",
                "storage service lost",
                "panicked",
                "Filesystem has been shut down",
            ]
            .iter()
            .any(|w| l.contains(w))
                || (l.contains("slic: storage service") && l.contains(" ended: "))
        })
        .map(|l| format!("after the stall: {}", l.trim()))
}

impl Suite {
    /// Step 200.
    #[allow(clippy::unused_self)] // every step has the same shape
    pub(crate) fn slow_request(&self, s: &mut Session) -> Result<String, String> {
        let before = active(s)?;
        let from = s.log.len();

        // A command held 30 s in the service, as one whose disk stalls.
        let armed = s.command("svc.dbgstall REQ(RunCmdCalling) MS(30000)", CMD)?;
        if !armed.contains("fault point") {
            return Err(format!("dbgstall: {armed}"));
        }
        let at = Instant::now();
        let made = s.command(
            "CRTLIB LIB(SLOWREQ) TEXT('step 200')",
            Duration::from_secs(120),
        )?;
        let held = at.elapsed();
        if held < Duration::from_secs(29) {
            return Err(format!("the command was not held: {held:?}: {made}"));
        }
        if !made.contains("SLOWREQ") {
            return Err(format!("CRTLIB after the stall: {made}"));
        }
        if !s.log[from..].contains("slow request RunCmdCalling") {
            return Err("the slow request was not logged".into());
        }
        if let Some(e) = lost(s, from) {
            return Err(e);
        }

        // The IFS device's writes held 45 s under a write and a sync.
        let dev_from = s.log.len();
        let armed = s.command("svc.dbgstall REQ(IFSDEV) MS(45000)", CMD)?;
        if !armed.contains("fault point") {
            return Err(format!("dbgstall: {armed}"));
        }
        let at = Instant::now();
        let put = s.command(
            "svc.ifs put /tmp/slowreq.txt held by the fault point",
            Duration::from_secs(120),
        )?;
        let dev = s.command("svc.ifs stat /tmp/slowreq.txt", Duration::from_secs(120))?;
        let ifs_held = at.elapsed();
        if dev.contains("not found") || dev.contains("CPF") {
            return Err(format!("the IFS file after the stall: {put} / {dev}"));
        }
        // The held write comes with writeback (30 s at most); wait for it
        // to be served, the block layer's timer run out on it on the way.
        let until = Instant::now() + Duration::from_secs(150);
        while !s.log[dev_from..].contains("IFS device ") {
            if Instant::now() > until {
                return Err("the IFS device's held write never ended".into());
            }
            std::thread::sleep(Duration::from_secs(3));
            let _ = s.command("svc.ifs df", CMD)?;
        }
        if let Some(e) = lost(s, from) {
            return Err(e);
        }

        // Every job still has the service, and the system is the same.
        let after = active(s)?;
        let gone: Vec<_> = before.difference(&after).collect();
        if !gone.is_empty() {
            return Err(format!("jobs ended across the stall: {gone:?}"));
        }
        let dlt = s.command("DLTLIB LIB(SLOWREQ)", CMD)?;
        if dlt.contains("CPF") && !dlt.contains("CPC") {
            return Err(format!("DLTLIB: {dlt}"));
        }
        let _ = s.command("svc.ifs rm /tmp/slowreq.txt", CMD);
        Ok(format!(
            "a command held {:.1}s and the IFS device {:.1}s: {} jobs kept the service, none ended",
            held.as_secs_f64(),
            ifs_held.as_secs_f64(),
            before.len()
        ))
    }
}

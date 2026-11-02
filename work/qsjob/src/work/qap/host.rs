// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The connection job's [`qapd::Host`]: the system's description and
//! messages through its link, and the end of the job: its parent's
//! `End` (the server ending) or `ENDJOB` (an operator).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use ipc::chan::Chan;
use qap_proto::ServerMessage;
use qapd::{Ending, Host, SystemId};
use stg::{Client, Shared};
use wm::ctl::ToJob;

/// How often the job asks whether an operator ended it.
const CHECK: Duration = Duration::from_millis(500);

/// The connection job's reach.
pub struct JobHost {
    pub link: Shared,
    me: u32,
    told: Arc<AtomicBool>,
    checked: Instant,
}

impl JobHost {
    /// Over `link` for job `me`, watching `ctl` for the parent's `End`.
    pub fn new(link: Shared, me: u32, ctl: &Chan) -> Self {
        let told = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&told);
        let mut reader = ctl.try_clone().expect("ctl");
        std::thread::spawn(move || {
            while let Ok(m) = reader.recv::<ToJob>() {
                if matches!(m, ToJob::End { .. }) {
                    flag.store(true, Ordering::SeqCst);
                    return;
                }
            }
        });
        Self {
            link,
            me,
            told,
            checked: Instant::now(),
        }
    }

    /// A system value's text, trimmed.
    pub fn sysval_text(&self, name: &str) -> String {
        self.link
            .borrow_mut()
            .with(|l| l.sysval(name.to_string()))
            .map(|v| match v.kind {
                sysapi::SysValKind::Bin => v
                    .data
                    .get(..4)
                    .and_then(|b| b.try_into().ok())
                    .map(i32::from_be_bytes)
                    .unwrap_or_default()
                    .to_string(),
                sysapi::SysValKind::Char => ccsid::from_ebcdic(&v.data).trim().to_string(),
            })
            .unwrap_or_default()
    }
}

impl Host for JobHost {
    fn system_id(&mut self) -> SystemId {
        SystemId {
            system_name: self.sysval_text("QSYSNAME"),
            release: release::RELEASE.to_string(),
            time_zone: self.sysval_text("QTIMZON"),
            ccsid: self.sysval_text("QCCSID").parse().unwrap_or(65535),
        }
    }

    fn now_ms(&mut self) -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
    }

    /// Looked up in this job, so in its user's language once the job
    /// runs as the signed-on profile (its library list finds the
    /// message files).
    fn message(&mut self, id: &str, data: &[&str]) -> ServerMessage {
        let file = sysapi::param::file_of(id);
        let desc = self
            .link
            .borrow_mut()
            .with(|l| l.describe(id.to_string(), file.into(), "*LIBL".into()));
        match desc {
            Some(d) => {
                let bytes = d.data_of(data);
                ServerMessage {
                    id: id.to_string(),
                    severity: u8::try_from(d.severity).unwrap_or(99),
                    text: d.format(&bytes),
                    help: d.format_help(&bytes),
                    data: bytes,
                }
            }
            None => ServerMessage {
                id: id.to_string(),
                severity: 40,
                data: Vec::new(),
                text: format!("{id} {}", data.join(" ")).trim().to_string(),
                help: String::new(),
            },
        }
    }

    fn ending(&mut self) -> Option<Ending> {
        if self.told.load(Ordering::SeqCst) {
            return Some(Ending::Server);
        }
        if self.checked.elapsed() < CHECK {
            return None;
        }
        self.checked = Instant::now();
        let state = self.link.borrow_mut().with(Client::sbs_state);
        if state.ending.is_some() {
            Some(Ending::Server)
        } else if state.end_jobs.iter().any(|(n, _)| *n == self.me) {
            Some(Ending::Operator)
        } else {
            None
        }
    }
}

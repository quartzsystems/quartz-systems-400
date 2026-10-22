// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The job's log: `CPF1124` at the start, `CPF1164` at the end, the
//! full-log end, and `DSPJOBLOG`.

use mi::Job;
use qir::MsgType;
use stg::Client;
use wm::ctl::FromJob;
use wm::joblog::{self, Full, JobLog};

use super::Console;
use super::host::Host;

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

impl Console {
    fn who(&self) -> (String, String, u32) {
        let s = &self.host.start;
        (s.name.clone(), s.user.clone(), s.number)
    }

    /// The job is running: its log sized by `JOBMSGQMX` and
    /// `JOBMSGQFL`, and `CPF1124` in it.
    pub fn started(&mut self) {
        let a = self.host.link.borrow_mut().with(Client::rtvjoba);
        let (name, user, number) = self.who();
        let subsystem = self.host.start.subsystem.clone();
        let j = self.job();
        j.joblog = JobLog::new(a.jobmsgqmx, Full::parse(&a.jobmsgqfl));
        let t = now();
        let data = joblog::started_data((&name, &user, number), &subsystem, t, t);
        j.log_external("CPF1124", "QCPFMSG", &data, MsgType::Info);
    }

    /// The job ends: `CPF1164`, the log brought up to date in the
    /// service, and the end code reported; the service spools the log.
    pub fn ended(&mut self, code: u16) {
        let (name, user, number) = self.who();
        let j = self.job();
        j.close_printers();
        let data = joblog::ended_data((&name, &user, number), now(), 0, code);
        j.log_external("CPF1164", "QCPFMSG", &data, MsgType::Comp);
        self.sync_log();
        self.host.link.borrow_mut().with(|l| l.job_ending(code));
    }

    /// The job log full under `*NOWRAP`: `CPF2460`, and the job ends.
    pub(super) fn full(&mut self) -> ! {
        let (name, user, number) = self.who();
        let data = joblog::data(&[(&name, 10), (&user, 10), (&format!("{number:06}"), 6)]);
        let j = self.job();
        j.joblog.max_bytes = 0;
        j.log_external("CPF2460", "QCPFMSG", &data, MsgType::Escape);
        self.ended(30);
        let _ = self.host.ctl.borrow_mut().send(&FromJob::Ending {
            reason: "CPF2460 the job message queue is full".into(),
        });
        std::process::exit(1)
    }
}

/// `DSPJOBLOG`: the log as it stands, kept by the job's `LOG`.
pub fn dspjoblog(job: &mut Job, host: &Host) -> String {
    let a = host.attrs();
    let all: Vec<&joblog::Entry> = job.joblog.entries().collect();
    let kept = joblog::filter(&all, a.log_level.max(1), u16::from(a.log_sev));
    joblog::display(&kept)
}

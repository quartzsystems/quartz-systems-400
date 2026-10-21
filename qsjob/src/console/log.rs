// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The job log of the console job: the start and end messages, the
//! log size from the job's attributes, `DSPJOBLOG`, and the printed log
//! at the end.

use qir::MsgType;
use stg::Client;
use wm::ctl::FromJob;
use wm::joblog::{self, Full, Header, JobLog};

use super::Console;

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

impl Console {
    fn who(&self) -> (String, String, u32) {
        (
            self.start.name.clone(),
            self.start.user.clone(),
            self.start.number,
        )
    }

    /// The job is running: its log sized by `JOBMSGQMX` and
    /// `JOBMSGQFL`, and `CPF1124` in it.
    pub(super) fn started(&mut self) {
        let a = self.link.borrow_mut().with(Client::rtvjoba);
        let (name, user, number) = self.who();
        let subsystem = self.start.subsystem.clone();
        let j = self.job();
        j.joblog = JobLog::new(a.jobmsgqmx, Full::parse(&a.jobmsgqfl));
        let t = now();
        let data = joblog::started_data((&name, &user, number), &subsystem, t, t);
        j.log_external("CPF1124", "QCPFMSG", &data, MsgType::Info);
    }

    /// The job ends: `CPF1164`, then the printed log.
    /// `TODO(phase5)`: spool it as `QPJOBLOG` to the job's `OUTQ`.
    pub(super) fn ended(&mut self, code: u16) -> Vec<String> {
        let (name, user, number) = self.who();
        let j = self.job();
        let data = joblog::ended_data((&name, &user, number), now(), 0, code);
        j.log_external("CPF1164", "QCPFMSG", &data, MsgType::Comp);
        self.printed(code != 0)
    }

    /// The job log full under `*NOWRAP`: `CPF2460`, and the job ends.
    pub(super) fn full(&mut self) -> ! {
        let (name, user, number) = self.who();
        let data = joblog::data(&[(&name, 10), (&user, 10), (&format!("{number:06}"), 6)]);
        let j = self.job();
        j.joblog.max_bytes = 0;
        j.log_external("CPF2460", "QCPFMSG", &data, MsgType::Escape);
        self.ended(30);
        let _ = self.ctl.send(&FromJob::Ending {
            reason: "CPF2460 the job message queue is full".into(),
        });
        std::process::exit(1)
    }

    /// The printed log as `LOG(level severity text)` keeps it; nothing
    /// for `*NOLIST` unless the job ended abnormally.
    pub(super) fn printed(&mut self, abnormal: bool) -> Vec<String> {
        let a = self.link.borrow_mut().with(Client::rtvjoba);
        if a.log_text == "*NOLIST" && !abnormal {
            return Vec::new();
        }
        let seclvl = a.log_text != "*MSG";
        let (name, user, number) = self.who();
        let (jlib, jname) = a.jobd.split_once('/').unwrap_or(("", a.jobd.as_str()));
        let (date, time) = joblog::stamp_of(now().saturating_mul(1_000_000));
        let header = Header {
            job: (name, user, number),
            jobd: (jname.to_string(), jlib.to_string()),
            system: "QS400".into(),
            stamp: (date, time[..8].to_string()),
        };
        let j = self.job();
        let mut all: Vec<&joblog::Entry> = j.joblog_wrapped.iter().collect();
        all.extend(j.joblog.entries());
        let kept = joblog::filter(&all, a.log_level, u16::from(a.log_sev));
        joblog::print(&header, &kept, seclvl)
    }

    /// `DSPJOBLOG`: the log as it stands, kept by the job's `LOG`.
    pub(super) fn dspjoblog(&mut self) -> String {
        let a = self.link.borrow_mut().with(Client::rtvjoba);
        let j = self.job();
        let all: Vec<&joblog::Entry> = j.joblog.entries().collect();
        let kept = joblog::filter(&all, a.log_level.max(1), u16::from(a.log_sev));
        joblog::display(&kept)
    }
}

// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `navigator::env::System` over the job's thread: the common part
//! (`web::system::Core`) and System status, read as the user from what
//! `WRKSYSSTS` (`panel2_data SYSSTS`), `WRKDSKSTS` (`DSKSTS`),
//! `DSPSYSVAL` (`QSRLNBR`, `QMODEL`), `WRKACTJOB` (`active_jobs`),
//! `WRKSBS` (`sbs_list`), `WRKJOBQ`, `WRKSPLF` (`splf_list`) and
//! `DSPMSG QSYSOPR` (`msgq_list`) show.

use navigator::env::{Audit, Status, System};
use qsweb::msg::{Message, SecondLevel};
use stg::Client;
use wm::obj::jobq::JobQ;
use wm::obj::msgq::{MsgKind, Status as MsgStatus};

use super::Link;
use super::parse;
use crate::work::web::bridge::Main;
use crate::work::web::system::Core;

/// The system as the `NAVIGATOR` job sees it.
pub struct LiveSystem {
    /// Name, dates, sign-on, profiles, audit, messages.
    pub core: Core,
    /// The job's thread.
    pub link: Link,
}

fn sysval(m: &Main, name: &str) -> String {
    let n = name.to_string();
    m.link
        .borrow_mut()
        .with(|l| l.sysval(n))
        .map(|v| ccsid::from_ebcdic(&v.data).trim().to_string())
        .unwrap_or_default()
}

/// What System status shows, read as the signed-on user.
fn status(m: &mut Main, reset: bool) -> Status {
    let mut s = Status {
        serial: sysval(m, "QSRLNBR"),
        model: sysval(m, "QMODEL"),
        release: release::RELEASE.to_string(),
        processors: std::thread::available_parallelism().map_or(1, std::num::NonZero::get).to_string(),
        ..Status::default()
    };
    let arg = if reset { "*YES" } else { "*NO" };
    if let Ok((vars, pools)) = m.link.borrow_mut().with(|l| l.panel2_data("SYSSTS".into(), vec![arg.into()])) {
        let v = |k: &str| vars.iter().find(|(n, _)| n == k).map_or("", |(_, x)| x.as_str());
        s.cpu = parse::tenths(v("CPUPCT"));
        s.elapsed = parse::hms(v("ELAPSED"));
        s.jobs = v("JOBS").trim().parse().unwrap_or(0);
        s.asp_capacity = parse::size(v("SYSASP"));
        s.asp_used = parse::tenths(v("ASPPCT"));
        s.temp_used = parse::size(v("CURUNP"));
        s.temp_peak = parse::size(v("MAXUNP"));
        s.main_storage = parse::size(&format!("{} M", v("MAINSTG")));
        s.pools = u32::try_from(pools.len()).unwrap_or(0);
        s.machine_pool = pools
            .iter()
            .find(|p| p.iter().any(|(k, x)| k == "POOL" && x.trim() == "1"))
            .and_then(|p| p.iter().find(|(k, _)| k == "SIZE"))
            .map_or(0, |(_, x)| parse::size(&format!("{x} M")));
    }
    if let Ok((_, units)) = m.link.borrow_mut().with(|l| l.panel2_data("DSKSTS".into(), vec!["*NO".into()])) {
        s.units = u32::try_from(units.len()).unwrap_or(0);
    }
    let now = m.link.borrow_mut().with(Client::now);
    s.zone = now.zone;
    let jobs = m.link.borrow_mut().with(Client::active_jobs);
    s.active = u32::try_from(jobs.len()).unwrap_or(0);
    s.interactive = u32::try_from(jobs.iter().filter(|j| j.kind == 'I').count()).unwrap_or(0);
    s.msgw = u32::try_from(jobs.iter().filter(|j| j.job_status().code() == "MSGW").count()).unwrap_or(0);
    s.ipl = jobs.iter().find(|j| j.kind == 'X').map_or(0, |j| j.started);
    s.subsystems = u32::try_from(m.link.borrow_mut().with(Client::sbs_list).len()).unwrap_or(0);
    let queues = m
        .link
        .borrow_mut()
        .with(|l| l.list_objects("*ALL".into(), "*ALL".into(), "*JOBQ".into()));
    s.queued = queues
        .iter()
        .filter_map(|o| {
            m.link
                .borrow_mut()
                .with(|l| l.wm_object(format!("{}/{}", o.lib, o.name), "*JOBQ".into()))
                .ok()
                .and_then(|b| JobQ::decode(&b).ok())
        })
        .map(|q| u32::try_from(q.jobs.len()).unwrap_or(0))
        .sum();
    let files = m.link.borrow_mut().with(|l| l.splf_list(String::new(), String::new()));
    s.spooled = u32::try_from(files.iter().filter(|f| f.status.code() == "RDY").count()).unwrap_or(0);
    if let Ok(msgs) = m.link.borrow_mut().with(|l| l.msgq_list("QSYS/QSYSOPR".into())) {
        let msgs: Vec<_> = msgs.into_iter().filter(|q| q.kind != MsgKind::Rpy).collect();
        s.qsysopr = u32::try_from(msgs.len()).unwrap_or(0);
        s.inquiries = u32::try_from(
            msgs.iter()
                .filter(|q| q.kind == MsgKind::Inq && q.status == MsgStatus::Unanswered)
                .count(),
        )
        .unwrap_or(0);
    }
    s
}

impl System for LiveSystem {
    fn name(&self) -> String {
        self.core.name()
    }

    fn now(&self) -> u64 {
        self.core.now()
    }

    fn stamp(&self, secs: u64) -> (String, String) {
        self.core.stamp(secs)
    }

    fn sign_on(&self, user: &str, password: &str, _from: &str) -> Result<String, Message> {
        self.core.sign_on(user, password)
    }

    fn special(&self, user: &str) -> Vec<String> {
        self.core.special(user)
    }

    fn user_class(&self, user: &str) -> String {
        self.core.user_class(user)
    }

    fn audit(&self, entry: Audit) {
        match entry {
            Audit::Password { user, code } => self.core.audit("PW", user, code.to_string(), String::new()),
            Audit::Refused { user, object, need } => self.core.audit("AF", user, object, need),
        }
    }

    fn message(&self, id: &str, data: &[&str]) -> Option<SecondLevel> {
        self.core.message(id, data)
    }

    fn status(&self, user: &str, reset: bool) -> Status {
        self.link.read(user, move |m| status(m, reset))
    }
}

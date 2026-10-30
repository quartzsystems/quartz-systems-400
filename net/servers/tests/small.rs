// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `*NTP` and `*LPD` against a fake job: the jobs they start, with
//! which parts and names, and ending the numbered LPD listener.

use servers::attrs::ntp::NtpAttrs;
use servers::framework::{Lpd, Ntp};
use servers::{Env, Framework, Instance, JobStart, Request};
use tcpcfg::model::svr::{ServerAttrs, Servers};

#[derive(Default)]
struct Fake {
    cfg: Servers,
    running: Vec<JobStart>,
    ended: Vec<String>,
}

impl Env for Fake {
    fn active(&mut self, job: &str) -> bool {
        self.running.iter().any(|j| j.name == job)
    }
    fn start_job(&mut self, job: &JobStart) -> Result<(), String> {
        self.running.push(job.clone());
        Ok(())
    }
    fn end_job(&mut self, job: &str, _immediate: bool) -> Result<(), String> {
        self.running.retain(|j| j.name != job);
        self.ended.push(job.to_string());
        Ok(())
    }
    fn attrs(&mut self, server: &str) -> ServerAttrs {
        self.cfg.get(server).cloned().unwrap_or(ServerAttrs {
            server: server.into(),
            autostart: false,
            bag: Vec::new(),
        })
    }
    fn start_subsystem(&mut self, _: &str) -> Result<(), String> {
        Ok(())
    }
}

fn framework() -> Framework {
    let mut f = Framework::standard();
    f.register(Box::new(Ntp));
    f.register(Box::new(Lpd));
    f
}

#[test]
fn ntp_client_and_server() {
    let fw = framework();
    let mut env = Fake::default();
    assert_eq!(fw.start(&mut env, &Request::new("*NTP")).unwrap(), "TCP1A07 NTP server starting.");
    let j = &env.running[0];
    assert_eq!((j.name.as_str(), j.user.as_str(), j.role.as_str()), ("QTOTNTP", "QNTP", "NTP"));
    assert_eq!(NtpAttrs::parts(&j.rqsdta), (true, true));
    assert!(fw.active(&mut env, "*NTP"));
    assert_eq!(fw.end(&mut env, &Request::new("*NTP"), false).unwrap(), "TCP1A0A NTP server ending.");
    // With TCP/IP: only the parts whose autostart is on.
    let mut a = NtpAttrs::default();
    a.set("SVRAUTOSTR", "*YES").unwrap();
    let e = env.cfg.get_mut("*NTP");
    e.autostart = a.autostarts();
    e.bag = a.to_bag();
    let req = Request {
        instance: Instance::Autostart,
        ..Request::new("*NTP")
    };
    fw.start(&mut env, &req).unwrap();
    assert_eq!(NtpAttrs::parts(&env.running[0].rqsdta), (false, true));
}

#[test]
fn lpd_numbered_listener() {
    let fw = framework();
    let mut env = Fake::default();
    assert_eq!(fw.start(&mut env, &Request::new("*LPD")).unwrap(), "TCP1A07 LPD server starting.");
    let j = &env.running[0];
    assert_eq!((j.name.as_str(), j.user.as_str(), j.subsystem.as_str(), j.role.as_str()), ("QTLPD00001", "QTMPLPD", "QSYSWRK", "LPD"));
    assert_eq!(fw.start(&mut env, &Request::new("*LPD")).unwrap(), "TCP1A08 LPD server already started.");
    assert_eq!(fw.end(&mut env, &Request::new("*LPD"), true).unwrap(), "TCP1A0A LPD server ending.");
    assert_eq!(env.ended, ["QTLPD00001"]);
    assert_eq!(fw.end(&mut env, &Request::new("*LPD"), false).unwrap(), "TCP1A09 LPD server not active.");
}
